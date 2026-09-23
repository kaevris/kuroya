use crate::{
    background_image_animation::{
        LoadedGifBackground, load_animated_gif_background, path_is_animated_gif,
    },
    image_preview::{LoadedImagePreview, load_image_preview},
    workspace_state::paths_match_exact_or_lexically,
};
use eframe::egui::{
    self, Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions, pos2, vec2,
};
use image::{RgbaImage, imageops::FilterType};
use kuroya_core::{EditorBackgroundImageFit, EditorBackgroundImagePosition};
use std::path::{Path, PathBuf};

const MAX_BACKGROUND_IMAGE_PIXELS: usize = 4 * 1024 * 1024;

#[derive(Debug)]
pub(crate) enum LoadedBackgroundImage {
    Static(LoadedImagePreview),
    AnimatedGif(LoadedGifBackground),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BackgroundImageGeometry {
    pub(crate) image_rect: Rect,
    pub(crate) uv: Rect,
}

pub(crate) fn background_image_geometry(
    bounds: Rect,
    image_size: [usize; 2],
    fit: EditorBackgroundImageFit,
    position: EditorBackgroundImagePosition,
) -> Option<BackgroundImageGeometry> {
    if !bounds.is_positive() {
        return None;
    }

    let [image_width, image_height] = image_size;
    let image_width = image_width as f32;
    let image_height = image_height as f32;
    if image_width <= 0.0
        || image_height <= 0.0
        || !image_width.is_finite()
        || !image_height.is_finite()
    {
        return None;
    }

    let bounds_size = bounds.size();
    match fit {
        EditorBackgroundImageFit::Cover => {
            let scale = (bounds_size.x / image_width).max(bounds_size.y / image_height);
            if !scale.is_finite() || scale <= 0.0 {
                return None;
            }

            let visible_width = (bounds_size.x / (image_width * scale)).clamp(0.0, 1.0);
            let visible_height = (bounds_size.y / (image_height * scale)).clamp(0.0, 1.0);
            let uv_min_x = (1.0 - visible_width) * 0.5;
            let uv_min_y = vertical_crop_offset(visible_height, position);

            Some(BackgroundImageGeometry {
                image_rect: bounds,
                uv: Rect::from_min_max(
                    pos2(uv_min_x, uv_min_y),
                    pos2(uv_min_x + visible_width, uv_min_y + visible_height),
                ),
            })
        }
        EditorBackgroundImageFit::Contain => {
            let scale = (bounds_size.x / image_width).min(bounds_size.y / image_height);
            let image_size = vec2(image_width * scale, image_height * scale);
            if !scale.is_finite()
                || scale <= 0.0
                || !image_size.x.is_finite()
                || !image_size.y.is_finite()
            {
                return None;
            }

            let free_vertical_space = bounds_size.y - image_size.y;
            let min_y = bounds.top() + vertical_alignment_offset(free_vertical_space, position);
            Some(BackgroundImageGeometry {
                image_rect: Rect::from_min_size(
                    pos2(bounds.center().x - image_size.x * 0.5, min_y),
                    image_size,
                ),
                uv: full_uv_rect(),
            })
        }
        EditorBackgroundImageFit::Stretch => Some(BackgroundImageGeometry {
            image_rect: bounds,
            uv: full_uv_rect(),
        }),
    }
}

fn vertical_crop_offset(visible_height: f32, position: EditorBackgroundImagePosition) -> f32 {
    match position {
        EditorBackgroundImagePosition::Top => 0.0,
        EditorBackgroundImagePosition::Center => (1.0 - visible_height) * 0.5,
        EditorBackgroundImagePosition::Bottom => 1.0 - visible_height,
    }
}

fn vertical_alignment_offset(
    free_vertical_space: f32,
    position: EditorBackgroundImagePosition,
) -> f32 {
    match position {
        EditorBackgroundImagePosition::Top => 0.0,
        EditorBackgroundImagePosition::Center => free_vertical_space * 0.5,
        EditorBackgroundImagePosition::Bottom => free_vertical_space,
    }
}

fn full_uv_rect() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
}

pub(crate) struct BackgroundImageState {
    source_path: PathBuf,
    loaded: LoadedImagePreview,
    texture: Option<TextureHandle>,
}

impl BackgroundImageState {
    pub(crate) fn from_loaded(source_path: PathBuf, loaded: LoadedImagePreview) -> Self {
        Self {
            source_path,
            loaded: downscale_background_image_preview(loaded),
            texture: None,
        }
    }

    pub(crate) fn matches_source_path(&self, path: &Path) -> bool {
        paths_match_exact_or_lexically(&self.source_path, path)
    }

    pub(crate) fn replace_loaded(
        &mut self,
        ctx: &egui::Context,
        loaded: LoadedImagePreview,
    ) -> bool {
        let mut loaded = downscale_background_image_preview(loaded);
        let Some(image) = take_loaded_color_image(&mut loaded) else {
            return false;
        };

        if let Some(texture) = self.texture.as_mut() {
            texture.set(image, TextureOptions::LINEAR);
        } else {
            self.texture = Some(ctx.load_texture(
                "kuroya-editor-background-image",
                image,
                TextureOptions::LINEAR,
            ));
        }
        self.loaded = loaded;
        true
    }

    pub(crate) fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        bounds: Rect,
        fit: EditorBackgroundImageFit,
        position: EditorBackgroundImagePosition,
        dim: f32,
        base_color: Color32,
    ) {
        let Some(geometry) = background_image_geometry(
            bounds,
            [self.loaded.width, self.loaded.height],
            fit,
            position,
        ) else {
            return;
        };
        let Some(texture_id) = self.texture_id(ctx) else {
            return;
        };

        let painter = painter.with_clip_rect(bounds);
        painter.image(texture_id, geometry.image_rect, geometry.uv, Color32::WHITE);

        let dim = bounded_dim(dim);
        if dim > 0.0 {
            painter.rect_filled(bounds, 0.0, base_color_with_dim(base_color, dim));
        }
    }

    fn texture_id(&mut self, ctx: &egui::Context) -> Option<egui::TextureId> {
        if self.texture.is_none() {
            let image = take_loaded_color_image(&mut self.loaded)?;
            self.texture = Some(ctx.load_texture(
                "kuroya-editor-background-image",
                image,
                TextureOptions::LINEAR,
            ));
        }

        self.texture.as_ref().map(TextureHandle::id)
    }
}

fn take_loaded_color_image(loaded: &mut LoadedImagePreview) -> Option<ColorImage> {
    let rgba = loaded.rgba.take()?;
    let expected_len = expected_rgba_len(loaded.width, loaded.height)?;
    if rgba.len() != expected_len {
        loaded.rgba = Some(rgba);
        return None;
    }
    Some(ColorImage::from_rgba_unmultiplied(
        [loaded.width, loaded.height],
        &rgba,
    ))
}

pub(crate) async fn load_background_image_preview(
    path: &Path,
) -> Result<LoadedImagePreview, String> {
    let loaded = load_image_preview(path).await?;
    tokio::task::spawn_blocking(move || downscale_background_image_preview(loaded))
        .await
        .map_err(|error| format!("background image resize task failed: {error}"))
}

pub(crate) async fn load_background_image(
    path: &Path,
    repaint_context: Option<egui::Context>,
) -> Result<LoadedBackgroundImage, String> {
    if path_is_animated_gif(path) {
        load_animated_gif_background(path, repaint_context)
            .await
            .map(LoadedBackgroundImage::AnimatedGif)
    } else {
        load_background_image_preview(path)
            .await
            .map(LoadedBackgroundImage::Static)
    }
}

pub(crate) fn downscale_background_image_preview(
    mut loaded: LoadedImagePreview,
) -> LoadedImagePreview {
    let Some(pixel_count) = loaded.width.checked_mul(loaded.height) else {
        return loaded;
    };
    if pixel_count <= MAX_BACKGROUND_IMAGE_PIXELS {
        return loaded;
    }
    if loaded.width > u32::MAX as usize || loaded.height > u32::MAX as usize {
        return loaded;
    }

    let Some(expected_len) = loaded
        .width
        .checked_mul(loaded.height)
        .and_then(|pixels| pixels.checked_mul(4))
    else {
        return loaded;
    };
    let Some(rgba) = loaded.rgba.take() else {
        return loaded;
    };
    if rgba.len() != expected_len {
        loaded.rgba = Some(rgba);
        return loaded;
    }

    let Some(source) = RgbaImage::from_raw(loaded.width as u32, loaded.height as u32, rgba) else {
        return loaded;
    };
    let scale = (MAX_BACKGROUND_IMAGE_PIXELS as f64 / pixel_count as f64).sqrt();
    let mut target_width = ((loaded.width as f64 * scale).floor() as u32).max(1);
    let mut target_height = ((loaded.height as f64 * scale).floor() as u32).max(1);
    while u64::from(target_width) * u64::from(target_height) > MAX_BACKGROUND_IMAGE_PIXELS as u64 {
        if target_width >= target_height {
            target_width = target_width.saturating_sub(1).max(1);
        } else {
            target_height = target_height.saturating_sub(1).max(1);
        }
    }
    let resized =
        image::imageops::resize(&source, target_width, target_height, FilterType::Triangle);

    loaded.width = target_width as usize;
    loaded.height = target_height as usize;
    loaded.rgba = Some(resized.into_raw());
    loaded
}

fn expected_rgba_len(width: usize, height: usize) -> Option<usize> {
    let pixels = width.checked_mul(height)?;
    if pixels > MAX_BACKGROUND_IMAGE_PIXELS {
        return None;
    }
    pixels.checked_mul(4)
}

fn bounded_dim(dim: f32) -> f32 {
    if dim.is_nan() {
        0.0
    } else {
        dim.clamp(0.0, 1.0)
    }
}

fn base_color_with_dim(base_color: Color32, dim: f32) -> Color32 {
    let dim = bounded_dim(dim);
    let scaled = |channel: u8| (f32::from(channel) * dim).round() as u8;
    Color32::from_rgba_premultiplied(
        scaled(base_color.r()),
        scaled(base_color.g()),
        scaled(base_color.b()),
        scaled(base_color.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundImageState, EditorBackgroundImageFit, EditorBackgroundImagePosition,
        background_image_geometry, base_color_with_dim, bounded_dim,
        downscale_background_image_preview,
    };
    use crate::image_preview::LoadedImagePreview;
    use eframe::egui::{self, Color32, LayerId, Rect, pos2};
    use std::path::{Path, PathBuf};

    const BOUNDS: Rect = Rect {
        min: pos2(10.0, 20.0),
        max: pos2(210.0, 120.0),
    };

    #[test]
    fn cover_centers_horizontal_crop() {
        let geometry = background_image_geometry(
            BOUNDS,
            [400, 100],
            EditorBackgroundImageFit::Cover,
            EditorBackgroundImagePosition::Center,
        )
        .expect("valid geometry");

        assert_eq!(geometry.image_rect, BOUNDS);
        assert_eq!(geometry.uv.min, pos2(0.25, 0.0));
        assert_eq!(geometry.uv.max, pos2(0.75, 1.0));
    }

    #[test]
    fn cover_uses_vertical_position_for_crop() {
        let top = background_image_geometry(
            BOUNDS,
            [100, 400],
            EditorBackgroundImageFit::Cover,
            EditorBackgroundImagePosition::Top,
        )
        .expect("valid geometry");
        let center = background_image_geometry(
            BOUNDS,
            [100, 400],
            EditorBackgroundImageFit::Cover,
            EditorBackgroundImagePosition::Center,
        )
        .expect("valid geometry");
        let bottom = background_image_geometry(
            BOUNDS,
            [100, 400],
            EditorBackgroundImageFit::Cover,
            EditorBackgroundImagePosition::Bottom,
        )
        .expect("valid geometry");

        assert_eq!(top.uv, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 0.125)));
        assert_eq!(
            center.uv,
            Rect::from_min_max(pos2(0.0, 0.4375), pos2(1.0, 0.5625))
        );
        assert_eq!(
            bottom.uv,
            Rect::from_min_max(pos2(0.0, 0.875), pos2(1.0, 1.0))
        );
    }

    #[test]
    fn contain_preserves_full_image_and_aligns_vertically() {
        let top = background_image_geometry(
            BOUNDS,
            [100, 25],
            EditorBackgroundImageFit::Contain,
            EditorBackgroundImagePosition::Top,
        )
        .expect("valid geometry");
        let center = background_image_geometry(
            BOUNDS,
            [100, 25],
            EditorBackgroundImageFit::Contain,
            EditorBackgroundImagePosition::Center,
        )
        .expect("valid geometry");
        let bottom = background_image_geometry(
            BOUNDS,
            [100, 25],
            EditorBackgroundImageFit::Contain,
            EditorBackgroundImagePosition::Bottom,
        )
        .expect("valid geometry");

        assert_eq!(
            top.image_rect,
            Rect::from_min_max(pos2(10.0, 20.0), pos2(210.0, 70.0))
        );
        assert_eq!(
            center.image_rect,
            Rect::from_min_max(pos2(10.0, 45.0), pos2(210.0, 95.0))
        );
        assert_eq!(
            bottom.image_rect,
            Rect::from_min_max(pos2(10.0, 70.0), pos2(210.0, 120.0))
        );
        assert_eq!(top.uv, center.uv);
        assert_eq!(
            center.uv,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))
        );
    }

    #[test]
    fn contain_upscales_and_stretch_fills_bounds() {
        let contain = background_image_geometry(
            BOUNDS,
            [20, 10],
            EditorBackgroundImageFit::Contain,
            EditorBackgroundImagePosition::Center,
        )
        .expect("valid geometry");
        let stretch = background_image_geometry(
            BOUNDS,
            [20, 10],
            EditorBackgroundImageFit::Stretch,
            EditorBackgroundImagePosition::Bottom,
        )
        .expect("valid geometry");

        assert_eq!(contain.image_rect, BOUNDS);
        assert_eq!(contain.uv, stretch.uv);
        assert_eq!(stretch.image_rect, BOUNDS);
    }

    #[test]
    fn geometry_rejects_empty_or_invalid_bounds() {
        assert!(
            background_image_geometry(
                Rect::from_min_max(pos2(0.0, 0.0), pos2(0.0, 20.0)),
                [100, 100],
                EditorBackgroundImageFit::Cover,
                EditorBackgroundImagePosition::Center,
            )
            .is_none()
        );
        assert!(
            background_image_geometry(
                BOUNDS,
                [0, 100],
                EditorBackgroundImageFit::Contain,
                EditorBackgroundImagePosition::Center,
            )
            .is_none()
        );
    }

    #[test]
    fn downscale_reduces_large_image_to_bounded_aspect_preserving_dimensions() {
        let loaded = LoadedImagePreview {
            width: 2049,
            height: 2049,
            rgba: Some(vec![255; 2049 * 2049 * 4]),
            byte_len: 2049 * 2049 * 4,
        };

        let resized = downscale_background_image_preview(loaded);

        assert_eq!([resized.width, resized.height], [2048, 2048]);
        assert!(resized.width * resized.height <= 4 * 1024 * 1024);
        assert_eq!(resized.rgba.as_ref().map(Vec::len), Some(2048 * 2048 * 4));
    }

    #[test]
    fn downscale_leaves_small_image_unchanged() {
        let loaded = LoadedImagePreview {
            width: 3,
            height: 2,
            rgba: Some(vec![
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
                24,
            ]),
            byte_len: 24,
        };

        let unchanged = downscale_background_image_preview(loaded);

        assert_eq!(unchanged.width, 3);
        assert_eq!(unchanged.height, 2);
        assert_eq!(unchanged.byte_len, 24);
        assert_eq!(
            unchanged.rgba.as_deref(),
            Some(
                &[
                    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                    23, 24
                ][..]
            )
        );
    }

    #[test]
    fn state_retains_source_path_and_matches_lexical_equivalents() {
        let path = PathBuf::from("assets/background.png");
        let state = state_for_test(path.clone(), 2, 1);

        assert!(state.matches_source_path(Path::new("assets/./background.png")));
        assert!(state.matches_source_path(&path));
        assert!(!state.matches_source_path(Path::new("assets/other.png")));
    }

    #[test]
    fn state_uploads_once_and_drops_cpu_rgba() {
        let ctx = egui::Context::default();
        let mut state = state_for_test(PathBuf::from("background.png"), 2, 1);

        let first = state.texture_id(&ctx).expect("texture should upload");
        assert!(state.loaded.rgba.is_none());
        let second = state
            .texture_id(&ctx)
            .expect("texture should remain available");

        assert_eq!(first, second);
        assert!(state.texture.is_some());
    }

    #[test]
    fn animated_frame_reuses_texture_and_drops_cpu_rgba() {
        let ctx = egui::Context::default();
        let mut state = state_for_test(PathBuf::from("background.gif"), 2, 1);
        let texture_id = state.texture_id(&ctx).expect("texture should upload");

        assert!(state.replace_loaded(
            &ctx,
            LoadedImagePreview {
                width: 3,
                height: 1,
                rgba: Some(vec![0, 255, 0, 255, 0, 255, 0, 255, 0, 255, 0, 255]),
                byte_len: 24,
            },
        ));

        assert_eq!(state.loaded.width, 3);
        assert_eq!(state.loaded.height, 1);
        assert!(state.loaded.rgba.is_none());
        assert_eq!(
            state.texture.as_ref().expect("texture should remain").id(),
            texture_id
        );
    }

    #[test]
    fn paint_uploads_texture_and_emits_image_and_dim_shapes() {
        let ctx = egui::Context::default();
        let mut state = state_for_test(PathBuf::from("background.png"), 2, 1);

        let output = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(LayerId::background());
            state.paint(
                ctx,
                &painter,
                BOUNDS,
                EditorBackgroundImageFit::Cover,
                EditorBackgroundImagePosition::Center,
                0.5,
                Color32::from_rgb(20, 30, 40),
            );
        });

        let texture_id = state
            .texture
            .as_ref()
            .expect("paint should upload the texture")
            .id();
        assert!(state.loaded.rgba.is_none());
        assert!(
            output
                .textures_delta
                .set
                .iter()
                .any(|(id, _)| *id == texture_id)
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Mesh(_)))
        );
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Rect(_)))
        );
    }

    #[test]
    fn state_bounds_large_images_and_rejects_malformed_cpu_images() {
        let ctx = egui::Context::default();
        let mut too_large = state_for_test(PathBuf::from("large.png"), 2049, 2049);
        assert!(too_large.loaded.width * too_large.loaded.height <= 4 * 1024 * 1024);
        assert!(too_large.texture_id(&ctx).is_some());

        let mut malformed = BackgroundImageState::from_loaded(
            PathBuf::from("malformed.png"),
            LoadedImagePreview {
                width: 2,
                height: 2,
                rgba: Some(vec![255; 3]),
                byte_len: 3,
            },
        );
        assert!(malformed.texture_id(&ctx).is_none());
    }

    #[test]
    fn dim_is_bounded_and_zero_is_transparent() {
        assert_eq!(bounded_dim(-1.0), 0.0);
        assert_eq!(bounded_dim(2.0), 1.0);
        assert_eq!(bounded_dim(f32::NAN), 0.0);

        let base = Color32::from_rgba_unmultiplied(12, 34, 56, 200);
        assert_eq!(base_color_with_dim(base, 0.0), Color32::TRANSPARENT);
        assert_eq!(base_color_with_dim(base, 1.0), base);
    }

    fn state_for_test(path: PathBuf, width: usize, height: usize) -> BackgroundImageState {
        BackgroundImageState::from_loaded(
            path,
            LoadedImagePreview {
                width,
                height,
                rgba: Some(vec![255; width * height * 4]),
                byte_len: width * height * 4,
            },
        )
    }
}
