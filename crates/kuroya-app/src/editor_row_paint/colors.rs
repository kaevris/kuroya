use crate::{editor_pane_rows::EditorRowContext, editor_text_geometry::visual_width_for_char};
use eframe::egui::{self, Color32, pos2};
use kuroya_core::{EditorColorDecoratorsActivatedOn, EditorDefaultColorDecorators};

pub(super) fn paint_color_decorators(
    painter: &egui::Painter,
    rect: egui::Rect,
    text_pos: egui::Pos2,
    line_text: &str,
    row: &EditorRowContext<'_>,
    row_hovered: bool,
) {
    if !color_decorators_visible(
        row.color_decorators,
        row.default_color_decorators,
        row.color_decorators_activated_on,
        row_hovered,
    ) {
        return;
    }

    let size = row.char_width.clamp(6.0, 10.0);
    visit_hex_color_decorations(
        line_text,
        row.color_decorators_limit,
        row.tab_width,
        |decoration| {
            let left = text_pos.x + decoration.visual_column as f32 * row.char_width + 1.0;
            let top = rect.top() + ((row.row_height - size) * 0.5).max(1.0);
            let swatch = egui::Rect::from_min_size(pos2(left, top), egui::vec2(size, size));
            painter.rect_filled(swatch, 2.0, decoration.color);
            painter.rect_stroke(
                swatch,
                2.0,
                egui::Stroke::new(1.0_f32, Color32::from_rgba_premultiplied(0, 0, 0, 140)),
                egui::StrokeKind::Inside,
            );
            true
        },
    );
}

pub(crate) fn color_decorators_visible(
    enabled: bool,
    default_mode: EditorDefaultColorDecorators,
    activated_on: EditorColorDecoratorsActivatedOn,
    row_hovered: bool,
) -> bool {
    if !enabled || default_mode == EditorDefaultColorDecorators::Never {
        return false;
    }
    if default_mode == EditorDefaultColorDecorators::Always {
        return true;
    }

    match activated_on {
        EditorColorDecoratorsActivatedOn::ClickAndHover
        | EditorColorDecoratorsActivatedOn::Hover => row_hovered,
        EditorColorDecoratorsActivatedOn::Click => false,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HexColorDecoration {
    pub(crate) column: usize,
    pub(crate) visual_column: usize,
    pub(crate) color: Color32,
}

fn visit_hex_color_decorations(
    line_text: &str,
    limit: usize,
    tab_width: usize,
    mut visit: impl FnMut(HexColorDecoration) -> bool,
) {
    if limit == 0 {
        return;
    }

    let tab_width = tab_width.max(1);
    let mut decorations = 0usize;
    let bytes = line_text.as_bytes();
    let mut previous = None;
    let mut visual_column = 0usize;
    for (column, (byte_index, ch)) in line_text.char_indices().enumerate() {
        if decorations >= limit {
            break;
        }
        if ch != '#' || !hex_color_boundary(previous) {
            previous = Some(ch);
            visual_column =
                visual_column.saturating_add(visual_width_for_char(ch, visual_column, tab_width));
            continue;
        }

        let Some(color) = parse_hex_color_bytes(bytes, byte_index) else {
            previous = Some(ch);
            visual_column =
                visual_column.saturating_add(visual_width_for_char(ch, visual_column, tab_width));
            continue;
        };
        if bytes.get(byte_index + 7).is_some_and(u8::is_ascii_hexdigit) {
            previous = Some(ch);
            visual_column =
                visual_column.saturating_add(visual_width_for_char(ch, visual_column, tab_width));
            continue;
        }

        decorations += 1;
        if !visit(HexColorDecoration {
            column,
            visual_column,
            color,
        }) {
            return;
        }
        previous = Some(ch);
        visual_column =
            visual_column.saturating_add(visual_width_for_char(ch, visual_column, tab_width));
    }
}

#[cfg(test)]
pub(crate) fn hex_color_decorations(line_text: &str, limit: usize) -> Vec<HexColorDecoration> {
    let mut decorations = Vec::new();
    visit_hex_color_decorations(line_text, limit, 4, |decoration| {
        decorations.push(decoration);
        true
    });
    decorations
}

fn hex_color_boundary(previous: Option<char>) -> bool {
    !matches!(previous, Some('#' | '0'..='9' | 'a'..='f' | 'A'..='F'))
}

fn parse_hex_color_bytes(bytes: &[u8], hash_index: usize) -> Option<Color32> {
    let hex = bytes.get(hash_index + 1..hash_index + 7)?;
    if !hex.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = |index: usize| hex_value(hex[index]);
    let r = value(0)? << 4 | value(1)?;
    let g = value(2)? << 4 | value(3)?;
    let b = value(4)? << 4 | value(5)?;
    Some(Color32::from_rgb(r, g, b))
}

#[cfg(test)]
pub(crate) fn parse_hex_color(chars: &[char]) -> Option<Color32> {
    if chars.len() != 6 || !chars.iter().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let value = |index: usize| hex_value(chars[index] as u8);
    let r = value(0)? << 4 | value(1)?;
    let g = value(2)? << 4 | value(3)?;
    let b = value(4)? << 4 | value(5)?;
    Some(Color32::from_rgb(r, g, b))
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
