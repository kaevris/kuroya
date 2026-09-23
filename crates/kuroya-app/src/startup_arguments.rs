use std::{ffi::OsString, fs, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupTarget {
    File(PathBuf),
    Folder(PathBuf),
}

pub(crate) fn resolve_startup_target() -> Option<StartupTarget> {
    startup_target_from_arguments(std::env::args_os().skip(1))
}

fn startup_file_from_arguments(arguments: impl IntoIterator<Item = OsString>) -> Option<PathBuf> {
    arguments
        .into_iter()
        .map(PathBuf::from)
        .find(|path| !path.as_os_str().is_empty())
}

fn startup_target_from_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Option<StartupTarget> {
    startup_file_from_arguments(arguments).map(|path| {
        if fs::metadata(&path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            StartupTarget::Folder(path)
        } else {
            StartupTarget::File(path)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{StartupTarget, startup_file_from_arguments, startup_target_from_arguments};
    use std::{
        ffi::OsString,
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn startup_file_uses_first_non_empty_argument() {
        let first = PathBuf::from(r"C:\Users\Kuroya User\project\main.rs");
        let second = PathBuf::from(r"C:\Users\Kuroya User\project\other.rs");

        assert_eq!(
            startup_file_from_arguments([
                OsString::new(),
                first.clone().into_os_string(),
                second.into_os_string(),
            ]),
            Some(first)
        );
    }

    #[test]
    fn startup_file_preserves_unicode_path_text() {
        let path = PathBuf::from("C:\\workspace\\\u{30c6}\u{30b9}\u{30c8} file.rs");

        assert_eq!(
            startup_file_from_arguments([path.clone().into_os_string()]),
            Some(path)
        );
    }

    #[test]
    fn startup_file_is_absent_without_a_non_empty_argument() {
        assert_eq!(startup_file_from_arguments([]), None);
        assert_eq!(startup_file_from_arguments([OsString::new()]), None);
    }

    #[test]
    fn startup_target_detects_existing_directory_as_folder() {
        let root = unique_temp_root("folder");
        fs::create_dir_all(&root).unwrap();

        let target = startup_target_from_arguments([root.clone().into_os_string()]);

        assert_eq!(target, Some(StartupTarget::Folder(root.clone())));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_target_treats_regular_files_as_files() {
        let root = unique_temp_root("file");
        fs::create_dir_all(&root).unwrap();
        let file = root.join("main.rs");
        fs::write(&file, "fn main() {}\n").unwrap();

        let target = startup_target_from_arguments([file.clone().into_os_string()]);

        assert_eq!(target, Some(StartupTarget::File(file)));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_target_defaults_missing_paths_to_files_and_keeps_none_absent() {
        let missing = unique_temp_root("missing").join("absent.rs");

        assert_eq!(
            startup_target_from_arguments([missing.clone().into_os_string()]),
            Some(StartupTarget::File(missing))
        );
        assert_eq!(startup_target_from_arguments([]), None);
    }

    fn unique_temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kuroya-startup-arguments-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
