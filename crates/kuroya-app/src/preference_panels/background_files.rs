use crate::{
    image_preview::SUPPORTED_RASTER_IMAGE_EXTENSIONS, native_paths::normalize_native_path,
    settings_form::optional_setting_path_from_input,
};

use std::path::{Path, PathBuf};

pub(super) fn choose_background_image_file(
    workspace_root: &Path,
    current: Option<&str>,
) -> Result<Option<String>, String> {
    let initial_dir = background_image_dialog_initial_dir(workspace_root, current);
    let Some(path) = pick_background_image_file(&initial_dir)? else {
        return Ok(None);
    };

    validated_selected_background_image_path(&path).map(Some)
}

fn pick_background_image_file(initial_dir: &Path) -> Result<Option<PathBuf>, String> {
    Ok(rfd::FileDialog::new()
        .set_title("Choose editor background image")
        .add_filter("Image files", SUPPORTED_RASTER_IMAGE_EXTENSIONS)
        .add_filter("All files", &["*"])
        .set_directory(initial_dir)
        .pick_file())
}

fn validated_selected_background_image_path(selected: &Path) -> Result<String, String> {
    let path = normalized_absolute_background_image_path(selected)?;
    if !path.is_file() {
        return Err("Selected background image is not a file".to_owned());
    }
    if !is_supported_background_image_path(&path) {
        return Err("Selected file is not a supported raster image".to_owned());
    }

    let path = path
        .to_str()
        .ok_or_else(|| "Selected background image path is not valid text".to_owned())?;
    optional_setting_path_from_input(path)
        .ok_or_else(|| "Selected background image path contains unsupported characters".to_owned())
}

fn normalized_absolute_background_image_path(selected: &Path) -> Result<PathBuf, String> {
    let selected = normalize_native_path(selected.to_path_buf());
    let absolute = if selected.is_absolute() {
        selected
    } else {
        std::env::current_dir()
            .map_err(|error| format!("Could not resolve the selected image path: {error}"))?
            .join(selected)
    };

    absolute
        .canonicalize()
        .map(normalize_native_path)
        .map_err(|error| format!("Could not resolve the selected image path: {error}"))
}

fn background_image_dialog_initial_dir(workspace_root: &Path, current: Option<&str>) -> PathBuf {
    let Some(current) = current.and_then(optional_setting_path_from_input) else {
        return workspace_root.to_path_buf();
    };
    let current = PathBuf::from(current);
    if current.is_dir() {
        current
    } else {
        current
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.to_path_buf())
    }
}

fn is_supported_background_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            SUPPORTED_RASTER_IMAGE_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
}

#[cfg(test)]
fn background_image_chooser_failure_status(error: &str) -> String {
    let error = crate::path_display::display_error_label_cow(error);
    format!(
        "Could not open editor background image chooser: {}",
        error.as_ref()
    )
}

#[cfg(test)]
mod tests {
    use super::{
        background_image_chooser_failure_status, background_image_dialog_initial_dir,
        is_supported_background_image_path, normalized_absolute_background_image_path,
        validated_selected_background_image_path,
    };
    use crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS;
    use std::{
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn selected_background_image_path_is_normalized_absolute_and_validated() {
        let root = temp_root("selected");
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("background.PNG");
        std::fs::write(&image, b"not-decoded-by-picker").unwrap();

        let selected = validated_selected_background_image_path(&image).unwrap();
        assert!(Path::new(&selected).is_absolute());
        assert_eq!(
            PathBuf::from(selected),
            normalized_absolute_background_image_path(&image).unwrap()
        );

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn selected_background_image_rejects_unsupported_extensions() {
        let root = temp_root("unsupported");
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("background.svg");
        std::fs::write(&image, b"<svg />").unwrap();

        assert_eq!(
            validated_selected_background_image_path(&image).unwrap_err(),
            "Selected file is not a supported raster image"
        );
        assert!(!is_supported_background_image_path(&image));

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn background_image_dialog_starts_from_current_image_parent() {
        assert_eq!(
            background_image_dialog_initial_dir(
                Path::new("workspace"),
                Some("C:/images/background.png"),
            ),
            Path::new("C:/images")
        );
    }

    #[test]
    fn background_image_chooser_failure_status_sanitizes_error_detail() {
        let status = background_image_chooser_failure_status(&format!(
            "first line\nsecond line \u{202e}{}",
            "image-detail-".repeat(DISPLAY_ERROR_LABEL_MAX_CHARS * 2)
        ));

        assert!(status.starts_with("Could not open editor background image chooser: first line "));
        assert!(!status.contains('\n'));
        assert!(!status.contains('\u{202e}'));
        assert!(status.contains("..."));
    }

    fn temp_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!(
            "kuroya-background-image-picker-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
