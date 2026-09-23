use std::{
    borrow::Cow,
    path::{Component, Path, PathBuf},
};

pub fn normalize_child_path(root: &Path, path: &Path) -> Option<PathBuf> {
    if path_contains_control(root) || path_contains_control(path) {
        return None;
    }

    let root = lexical_normalize_cow(root);
    normalize_child_path_with_normalized_root(root.as_ref(), path)
}

pub(crate) fn normalize_child_path_with_normalized_root(
    root: &Path,
    path: &Path,
) -> Option<PathBuf> {
    if path_contains_control(root) || path_contains_control(path) {
        return None;
    }

    #[cfg(windows)]
    if is_ambiguous_windows_path(path) {
        return None;
    }

    let candidate: Cow<'_, Path> = if path.is_absolute() {
        Cow::Borrowed(path)
    } else {
        Cow::Owned(root.join(path))
    };
    let containment = path_stays_within_root_lexically(root, &candidate)?;
    let candidate = if containment.requires_normalization {
        lexical_normalize_slow(candidate.as_ref())
    } else {
        candidate.into_owned()
    };
    if root == Path::new(".") {
        debug_assert!(path_is_relative_to_current_dir(&candidate));
        return Some(candidate);
    }
    path_starts_with_lexically(&candidate, root).then_some(candidate)
}

fn path_contains_control(path: &Path) -> bool {
    path.components()
        .any(|component| os_str_contains_control(component.as_os_str()))
}

#[cfg(windows)]
fn os_str_contains_control(value: &std::ffi::OsStr) -> bool {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().any(|unit| {
        unit <= 0x001f
            || (0x007f..=0x009f).contains(&unit)
            || utf16_unit_is_hidden_format_control(unit)
    })
}

#[cfg(unix)]
fn os_str_contains_control(value: &std::ffi::OsStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let bytes = value.as_bytes();
    bytes.iter().any(|byte| matches!(*byte, 0x00..=0x1f | 0x7f))
        || std::str::from_utf8(bytes)
            .is_ok_and(|text| text.chars().any(path_char_is_hidden_control))
}

#[cfg(not(any(unix, windows)))]
fn os_str_contains_control(value: &std::ffi::OsStr) -> bool {
    value
        .to_str()
        .is_some_and(|text| text.chars().any(path_char_is_hidden_control))
}

#[cfg(windows)]
fn utf16_unit_is_hidden_format_control(unit: u16) -> bool {
    matches!(
        unit,
        0x00ad
            | 0x034f
            | 0x061c
            | 0x180e
            | 0x200b..=0x200f
            | 0x202a..=0x202e
            | 0x2060..=0x206f
            | 0xfeff
    )
}

#[cfg(not(windows))]
fn path_char_is_hidden_control(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{00ad}'
                | '\u{034f}'
                | '\u{061c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
        )
}

#[cfg(windows)]
fn is_ambiguous_windows_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .any(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
}

pub(crate) fn lexical_normalize(path: &Path) -> PathBuf {
    lexical_normalize_cow(path).into_owned()
}

pub(crate) fn lexical_normalize_cow(path: &Path) -> Cow<'_, Path> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        Cow::Owned(lexical_normalize_slow(path))
    } else {
        Cow::Borrowed(path)
    }
}

fn lexical_normalize_slow(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    let mut has_root = false;
    let mut normal_depth = 0usize;
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => {
                has_root = true;
                normalized.push(component.as_os_str());
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if normal_depth > 0 {
                    normalized.pop();
                    normal_depth -= 1;
                } else if !has_root {
                    normalized.push("..");
                }
            }
            Component::Normal(part) => {
                normalized.push(part);
                normal_depth += 1;
            }
        }
    }

    if normalized.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        normalized
    }
}

fn path_is_relative_to_current_dir(path: &Path) -> bool {
    path.is_relative()
        && !matches!(
            path.components().next(),
            Some(Component::ParentDir | Component::Prefix(_) | Component::RootDir)
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PathContainment {
    requires_normalization: bool,
}

fn path_stays_within_root_lexically(root: &Path, path: &Path) -> Option<PathContainment> {
    let mut requires_normalization = path.as_os_str().is_empty();

    if root == Path::new(".") {
        return remaining_components_stay_within_root(path.components(), requires_normalization);
    }

    let mut path_components = path.components();
    for root_component in root
        .components()
        .filter(|component| !matches!(*component, Component::CurDir))
    {
        let path_component =
            next_non_current_dir_component(&mut path_components, &mut requires_normalization)?;
        if !components_match_lexically(path_component, root_component) {
            return None;
        }
    }

    remaining_components_stay_within_root(path_components, requires_normalization)
}

fn next_non_current_dir_component<'a>(
    components: &mut impl Iterator<Item = Component<'a>>,
    requires_normalization: &mut bool,
) -> Option<Component<'a>> {
    loop {
        match components.next()? {
            Component::CurDir => *requires_normalization = true,
            component @ Component::ParentDir => {
                *requires_normalization = true;
                return Some(component);
            }
            component => return Some(component),
        }
    }
}

fn remaining_components_stay_within_root<'a>(
    components: impl IntoIterator<Item = Component<'a>>,
    mut requires_normalization: bool,
) -> Option<PathContainment> {
    let mut depth = 0usize;
    for component in components {
        match component {
            Component::CurDir => requires_normalization = true,
            Component::Normal(_) => depth += 1,
            Component::ParentDir if depth == 0 => return None,
            Component::ParentDir => {
                requires_normalization = true;
                depth -= 1;
            }
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    Some(PathContainment {
        requires_normalization,
    })
}

fn components_match_lexically(left: Component<'_>, right: Component<'_>) -> bool {
    #[cfg(windows)]
    {
        components_match_windows_case(left, right)
    }
    #[cfg(not(windows))]
    {
        left == right
    }
}

pub(crate) fn path_starts_with_lexically(path: &Path, root: &Path) -> bool {
    if path.starts_with(root) {
        return true;
    }

    #[cfg(windows)]
    {
        path_starts_with_windows_case(path, root)
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
fn path_starts_with_windows_case(path: &Path, root: &Path) -> bool {
    let mut path_components = path.components();
    for root_component in root.components() {
        let Some(path_component) = path_components.next() else {
            return false;
        };
        if !components_match_windows_case(path_component, root_component) {
            return false;
        }
    }
    true
}

#[cfg(windows)]
fn components_match_windows_case(left: Component<'_>, right: Component<'_>) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            prefix_components_match_windows_case(left, right)
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::CurDir, Component::CurDir) => true,
        (Component::ParentDir, Component::ParentDir) => true,
        (Component::Normal(left), Component::Normal(right)) => os_str_eq_windows_case(left, right),
        _ => false,
    }
}

#[cfg(windows)]
fn prefix_components_match_windows_case(
    left: std::path::PrefixComponent<'_>,
    right: std::path::PrefixComponent<'_>,
) -> bool {
    use std::path::Prefix;

    match (left.kind(), right.kind()) {
        (Prefix::Disk(left), Prefix::Disk(right))
        | (Prefix::Disk(left), Prefix::VerbatimDisk(right))
        | (Prefix::VerbatimDisk(left), Prefix::Disk(right))
        | (Prefix::VerbatimDisk(left), Prefix::VerbatimDisk(right)) => {
            left.eq_ignore_ascii_case(&right)
        }
        (Prefix::UNC(left_server, left_share), Prefix::UNC(right_server, right_share))
        | (Prefix::UNC(left_server, left_share), Prefix::VerbatimUNC(right_server, right_share))
        | (Prefix::VerbatimUNC(left_server, left_share), Prefix::UNC(right_server, right_share))
        | (
            Prefix::VerbatimUNC(left_server, left_share),
            Prefix::VerbatimUNC(right_server, right_share),
        ) => {
            os_str_eq_windows_case(left_server, right_server)
                && os_str_eq_windows_case(left_share, right_share)
        }
        _ => os_str_eq_windows_case(left.as_os_str(), right.as_os_str()),
    }
}

#[cfg(windows)]
fn os_str_eq_windows_case(left: &std::ffi::OsStr, right: &std::ffi::OsStr) -> bool {
    if left == right {
        return true;
    }
    if let (Some(left), Some(right)) = (left.to_str(), right.to_str()) {
        if left.is_ascii() && right.is_ascii() {
            return left.eq_ignore_ascii_case(right);
        }

        return left
            .chars()
            .flat_map(char::to_lowercase)
            .eq(right.chars().flat_map(char::to_lowercase));
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_child_path_accepts_relative_children_and_collapses_dot_segments() {
        assert_eq!(
            normalize_child_path(Path::new("workspace"), Path::new("src/./main.rs")),
            Some(PathBuf::from("workspace").join("src/main.rs"))
        );
        assert_eq!(
            normalize_child_path(Path::new("workspace"), Path::new("src/../src/lib.rs")),
            Some(PathBuf::from("workspace").join("src/lib.rs"))
        );
    }

    #[test]
    fn normalize_child_path_rejects_parent_and_sibling_escapes() {
        assert!(normalize_child_path(Path::new("workspace"), Path::new("../outside.rs")).is_none());
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("src/../../outside.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new("workspace"),
                Path::new("../../../workspace/secret.rs")
            )
            .is_none()
        );
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("../workspace/secret.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new("workspace"),
                Path::new("src/../../workspace/secret.rs")
            )
            .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new("/workspace/app"),
                Path::new("/workspace/app-sibling")
            )
            .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new("/workspace/app"),
                Path::new("/workspace/app/../app/secret.rs")
            )
            .is_none()
        );
    }

    #[test]
    fn normalize_child_path_rejects_control_characters() {
        assert!(normalize_child_path(Path::new("workspace"), Path::new("src/\0main.rs")).is_none());
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("src/\u{001b}main.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("src/\u{0085}main.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(Path::new("workspace\nremoved/.."), Path::new("src/main.rs"))
                .is_none()
        );
    }

    #[test]
    fn normalize_child_path_rejects_hidden_format_controls() {
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("src/\u{202e}main.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(Path::new("workspace"), Path::new("src/\u{200b}main.rs"))
                .is_none()
        );
        assert!(
            normalize_child_path(Path::new("workspace\u{2066}"), Path::new("src/main.rs"))
                .is_none()
        );
    }

    #[test]
    fn normalize_child_path_handles_current_dir_root() {
        assert_eq!(
            normalize_child_path(Path::new("."), Path::new("src/lib.rs")),
            Some(PathBuf::from("src/lib.rs"))
        );
        assert_eq!(
            normalize_child_path(Path::new("."), Path::new("src/../lib.rs")),
            Some(PathBuf::from("lib.rs"))
        );
        assert!(normalize_child_path(Path::new("."), Path::new("../outside.rs")).is_none());
        assert!(normalize_child_path(Path::new("."), Path::new("src/../../lib.rs")).is_none());
    }

    #[cfg(not(windows))]
    #[test]
    fn normalize_child_path_preserves_clean_absolute_child_path() {
        let child = PathBuf::from("/workspace/app/src/main.rs");

        assert_eq!(
            normalize_child_path(Path::new("/workspace/app"), &child),
            Some(child)
        );
    }

    #[test]
    fn lexical_normalize_preserves_stacked_relative_parents() {
        assert_eq!(
            lexical_normalize(Path::new("../..")),
            PathBuf::from("../..")
        );
        assert_eq!(
            lexical_normalize(Path::new("../../x")),
            PathBuf::from("../../x")
        );
        assert_eq!(
            lexical_normalize(Path::new("a/../../../b")),
            PathBuf::from("../../b")
        );
        assert_eq!(
            lexical_normalize(Path::new("a/b/c/../../../d")),
            PathBuf::from("d")
        );
    }

    #[test]
    fn lexical_normalize_cow_borrows_clean_paths_and_owns_adjusted_paths() {
        let clean = Path::new("workspace/src/main.rs");
        assert!(matches!(lexical_normalize_cow(clean), Cow::Borrowed(path) if path == clean));

        match lexical_normalize_cow(Path::new("workspace/src/../lib.rs")) {
            Cow::Owned(path) => assert_eq!(path, PathBuf::from("workspace/lib.rs")),
            Cow::Borrowed(_) => panic!("adjusted path should be owned"),
        }
        match lexical_normalize_cow(Path::new("")) {
            Cow::Owned(path) => assert_eq!(path, PathBuf::from(".")),
            Cow::Borrowed(_) => panic!("empty path should be normalized to owned current dir"),
        }
    }

    #[test]
    fn lexical_containment_reports_required_normalization() {
        assert_eq!(
            path_stays_within_root_lexically(
                Path::new("workspace"),
                Path::new("workspace/src/main.rs")
            ),
            Some(PathContainment {
                requires_normalization: false
            })
        );
        assert_eq!(
            path_stays_within_root_lexically(
                Path::new("workspace"),
                Path::new("workspace/src/../lib.rs")
            ),
            Some(PathContainment {
                requires_normalization: true
            })
        );
        assert_eq!(
            path_stays_within_root_lexically(Path::new("."), Path::new("./src/lib.rs")),
            Some(PathContainment {
                requires_normalization: true
            })
        );
        assert_eq!(
            path_stays_within_root_lexically(
                Path::new("workspace"),
                Path::new("workspace/../workspace/lib.rs")
            ),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_child_path_matches_windows_components_case_insensitively() {
        assert_eq!(
            normalize_child_path(
                Path::new(r"C:\Workspace\Project"),
                Path::new(r"c:\workspace\project\src\main.rs")
            ),
            Some(PathBuf::from(r"c:\workspace\project\src\main.rs"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_child_path_keeps_windows_roots_and_components_distinct() {
        assert!(
            normalize_child_path(
                Path::new(r"C:\Workspace\Project"),
                Path::new(r"C:\Workspace\ProjectSibling\main.rs")
            )
            .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new(r"C:\Workspace\Project"),
                Path::new(r"D:\Workspace\Project\main.rs")
            )
            .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_child_path_rejects_ambiguous_windows_paths() {
        assert!(
            normalize_child_path(
                Path::new(r"C:\Workspace\Project"),
                Path::new(r"\Workspace\Project\src\main.rs")
            )
            .is_none()
        );
        assert!(
            normalize_child_path(
                Path::new(r"C:\Workspace\Project"),
                Path::new(r"C:src\main.rs")
            )
            .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_child_path_accepts_windows_verbatim_aliases_without_reentry() {
        let disk_root = Path::new(r"C:\Workspace\Project");
        let disk_child = Path::new(r"\\?\c:\Workspace\Project\src\main.rs");

        assert_eq!(
            normalize_child_path(disk_root, disk_child),
            Some(disk_child.to_path_buf())
        );
        assert!(
            normalize_child_path(
                disk_root,
                Path::new(r"\\?\C:\Workspace\Project\..\Project\src\main.rs")
            )
            .is_none()
        );

        let unc_root = Path::new(r"\\Server\Share\Workspace\Project");
        let unc_child = Path::new(r"\\?\UNC\server\share\Workspace\Project\src\main.rs");

        assert_eq!(
            normalize_child_path(unc_root, unc_child),
            Some(unc_child.to_path_buf())
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_case_matching_uses_unicode_case_without_normalizing_distinct_text() {
        assert!(super::os_str_eq_windows_case(
            std::ffi::OsStr::new("\u{0130}"),
            std::ffi::OsStr::new("i\u{307}")
        ));
        assert!(!super::os_str_eq_windows_case(
            std::ffi::OsStr::new("\u{00e9}"),
            std::ffi::OsStr::new("e\u{301}")
        ));
    }
}
