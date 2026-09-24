use std::path::PathBuf;

#[cfg(windows)]
pub(crate) fn normalize_native_path(path: PathBuf) -> PathBuf {
    let path_text = path.as_os_str().to_string_lossy();
    if let Some(path) = path_text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{path}"));
    }
    if let Some(path) = path_text.strip_prefix(r"\\?\") {
        return PathBuf::from(path.to_owned());
    }
    path
}

#[cfg(not(windows))]
pub(crate) fn normalize_native_path(path: PathBuf) -> PathBuf {
    path
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    use super::normalize_native_path;
    #[cfg(windows)]
    use std::path::PathBuf;

    #[cfg(windows)]
    #[test]
    fn normalize_native_path_strips_windows_verbatim_disk_prefix() {
        assert_eq!(
            normalize_native_path(PathBuf::from(r"\\?\C:\Users\kuroya\project")),
            PathBuf::from(r"C:\Users\kuroya\project")
        );
    }

    #[cfg(windows)]
    #[test]
    fn normalize_native_path_strips_windows_verbatim_unc_prefix() {
        assert_eq!(
            normalize_native_path(PathBuf::from(r"\\?\UNC\server\share\project")),
            PathBuf::from(r"\\server\share\project")
        );
    }
}
