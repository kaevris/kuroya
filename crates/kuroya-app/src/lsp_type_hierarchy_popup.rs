use crate::{
    KuroyaApp,
    lsp_reference_popup::{
        LSP_POPUP_LABEL_MAX_CHARS, lsp_popup_item_location_label_into, lsp_popup_location_label,
    },
    popup_buttons::{PopupButtonKind, popup_button},
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Align, Context, Key, RichText, ScrollArea};
use kuroya_core::LspTypeHierarchyItem;

const TYPE_HIERARCHY_ROW_HEIGHT: f32 = 24.0;
const TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION: usize = 500;

impl KuroyaApp {
    pub(crate) fn render_type_hierarchy_popup(&mut self, ctx: &Context) {
        let mut close = false;
        let mut open_target = None;
        let prepared = PreparedTypeHierarchyPopup::build(
            self.type_hierarchy_root.as_ref(),
            self.type_hierarchy_path.as_deref(),
            self.type_hierarchy_line,
            self.type_hierarchy_column,
            &self.type_hierarchy_supertypes,
            &self.type_hierarchy_subtypes,
        );
        let row_count = prepared.row_count();
        let mut selected = self.type_hierarchy_selected;
        clamp_selection(&mut selected, row_count);
        let mut row_label = String::with_capacity(LSP_POPUP_LABEL_MAX_CHARS);

        egui::Window::new("Type Hierarchy")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 168.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 144.0])
            .default_size([680.0, 360.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(prepared.target_label())
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if popup_button(ui, "Close", PopupButtonKind::Secondary).clicked() {
                            close = true;
                        }
                    });
                });

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut selected,
                        row_count,
                        selection_page_step(TYPE_HIERARCHY_ROW_HEIGHT, viewport_height),
                    )
                });

                let visible_row_count = prepared.visible_row_count();
                ui.separator();
                if visible_row_count == 0 {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("No types");
                    });
                    return;
                }

                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    open_target = prepared.open_target_for_selection(selected);
                }

                let mut scroll_area = ScrollArea::vertical();
                if selection_changed {
                    scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                        prepared.visible_row_for_selection(selected),
                        visible_row_count,
                        TYPE_HIERARCHY_ROW_HEIGHT,
                        viewport_height,
                    ));
                }
                scroll_area.show_rows(
                    ui,
                    TYPE_HIERARCHY_ROW_HEIGHT,
                    visible_row_count,
                    |ui, rows| {
                        for row in rows {
                            match prepared.row(row) {
                                Some(PreparedTypeHierarchyRow::SupertypesHeader) => {
                                    ui.label(RichText::new("Supertypes").strong());
                                }
                                Some(PreparedTypeHierarchyRow::SupertypesEmpty) => {
                                    ui.label(RichText::new("No supertypes").small());
                                }
                                Some(PreparedTypeHierarchyRow::Supertype {
                                    selection_index,
                                    target,
                                }) => {
                                    render_prepared_type_hierarchy_row(
                                        ui,
                                        selection_index,
                                        target,
                                        &mut row_label,
                                        &mut selected,
                                        &mut open_target,
                                    );
                                }
                                Some(PreparedTypeHierarchyRow::SubtypesHeader) => {
                                    ui.label(RichText::new("Subtypes").strong());
                                }
                                Some(PreparedTypeHierarchyRow::SubtypesEmpty) => {
                                    ui.label(RichText::new("No subtypes").small());
                                }
                                Some(PreparedTypeHierarchyRow::Subtype {
                                    selection_index,
                                    target,
                                }) => {
                                    render_prepared_type_hierarchy_row(
                                        ui,
                                        selection_index,
                                        target,
                                        &mut row_label,
                                        &mut selected,
                                        &mut open_target,
                                    );
                                }
                                None => {}
                            }
                        }
                    },
                );
            });

        self.type_hierarchy_selected = selected;
        if close {
            self.clear_type_hierarchy();
            self.status = "Closed type hierarchy".to_owned();
        } else if let Some(item) = open_target {
            self.open_type_hierarchy_item(item);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PreparedTypeHierarchyPopup<'a> {
    target_label: String,
    supertypes: &'a [LspTypeHierarchyItem],
    subtypes: &'a [LspTypeHierarchyItem],
    row_count: usize,
    visible_row_count: usize,
}

impl<'a> PreparedTypeHierarchyPopup<'a> {
    fn build(
        root: Option<&'a LspTypeHierarchyItem>,
        target_path: Option<&std::path::Path>,
        target_line: usize,
        target_column: usize,
        supertypes: &'a [LspTypeHierarchyItem],
        subtypes: &'a [LspTypeHierarchyItem],
    ) -> Self {
        let supertypes_len = type_hierarchy_display_len(supertypes.len());
        let subtypes_len = type_hierarchy_display_len(subtypes.len());
        let supertypes = &supertypes[..supertypes_len];
        let subtypes = &subtypes[..subtypes_len];
        let visible_row_count = if root.is_some() || supertypes_len > 0 || subtypes_len > 0 {
            type_hierarchy_visible_row_count(supertypes_len, subtypes_len)
        } else {
            0
        };

        Self {
            target_label: type_hierarchy_target_label(
                root,
                target_path,
                target_line,
                target_column,
            ),
            supertypes,
            subtypes,
            row_count: supertypes_len.saturating_add(subtypes_len),
            visible_row_count,
        }
    }

    fn row_count(&self) -> usize {
        self.row_count
    }

    fn visible_row_count(&self) -> usize {
        self.visible_row_count
    }

    fn target_label(&self) -> &str {
        &self.target_label
    }

    fn row(&self, row: usize) -> Option<PreparedTypeHierarchyRow<'a>> {
        if row >= self.visible_row_count {
            return None;
        }

        match type_hierarchy_visible_row(self.supertypes.len(), self.subtypes.len(), row)? {
            TypeHierarchyVisibleRow::SupertypesHeader => {
                Some(PreparedTypeHierarchyRow::SupertypesHeader)
            }
            TypeHierarchyVisibleRow::SupertypesEmpty => {
                Some(PreparedTypeHierarchyRow::SupertypesEmpty)
            }
            TypeHierarchyVisibleRow::Supertype {
                target,
                selection_index,
            } => type_hierarchy_open_target_item(self.supertypes, self.subtypes, target).map(
                |item| PreparedTypeHierarchyRow::Supertype {
                    selection_index,
                    target: item,
                },
            ),
            TypeHierarchyVisibleRow::SubtypesHeader => {
                Some(PreparedTypeHierarchyRow::SubtypesHeader)
            }
            TypeHierarchyVisibleRow::SubtypesEmpty => Some(PreparedTypeHierarchyRow::SubtypesEmpty),
            TypeHierarchyVisibleRow::Subtype {
                target,
                selection_index,
            } => type_hierarchy_open_target_item(self.supertypes, self.subtypes, target).map(
                |item| PreparedTypeHierarchyRow::Subtype {
                    selection_index,
                    target: item,
                },
            ),
        }
    }

    fn visible_row_for_selection(&self, selected: usize) -> usize {
        if selected >= self.row_count {
            return 0;
        }

        type_hierarchy_visible_row_for_selection(
            self.supertypes.len(),
            self.subtypes.len(),
            selected,
        )
    }

    fn open_target_for_selection(&self, selected: usize) -> Option<LspTypeHierarchyItem> {
        selected_type_hierarchy_open_target(self.supertypes.len(), self.subtypes.len(), selected)
            .and_then(|target| {
                type_hierarchy_open_target_item(self.supertypes, self.subtypes, target)
            })
            .cloned()
    }
}

#[derive(Debug, Clone, PartialEq)]
enum PreparedTypeHierarchyRow<'a> {
    SupertypesHeader,
    SupertypesEmpty,
    Supertype {
        selection_index: usize,
        target: &'a LspTypeHierarchyItem,
    },
    SubtypesHeader,
    SubtypesEmpty,
    Subtype {
        selection_index: usize,
        target: &'a LspTypeHierarchyItem,
    },
}

fn type_hierarchy_target_label(
    root: Option<&LspTypeHierarchyItem>,
    target_path: Option<&std::path::Path>,
    target_line: usize,
    target_column: usize,
) -> String {
    root.map(type_hierarchy_item_label)
        .or_else(|| {
            target_path.map(|path| lsp_popup_location_label(path, target_line, target_column))
        })
        .unwrap_or_else(|| "No target".to_owned())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeHierarchyVisibleRow {
    SupertypesHeader,
    SupertypesEmpty,
    Supertype {
        target: TypeHierarchyOpenTarget,
        selection_index: usize,
    },
    SubtypesHeader,
    SubtypesEmpty,
    Subtype {
        target: TypeHierarchyOpenTarget,
        selection_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TypeHierarchyOpenTarget {
    Supertype(usize),
    Subtype(usize),
}

fn type_hierarchy_visible_row_count(supertypes_len: usize, subtypes_len: usize) -> usize {
    2usize
        .saturating_add(supertypes_len.max(1))
        .saturating_add(subtypes_len.max(1))
}

fn type_hierarchy_display_len(len: usize) -> usize {
    len.min(TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION)
}

fn type_hierarchy_visible_row_for_selection(
    supertypes_len: usize,
    subtypes_len: usize,
    selected: usize,
) -> usize {
    let row_count = supertypes_len.saturating_add(subtypes_len);
    if row_count == 0 {
        return 0;
    }

    let selected = selected.min(row_count.saturating_sub(1));
    if selected < supertypes_len {
        1usize.saturating_add(selected)
    } else {
        2usize
            .saturating_add(supertypes_len.max(1))
            .saturating_add(selected.saturating_sub(supertypes_len))
    }
}

fn type_hierarchy_visible_row(
    supertypes_len: usize,
    subtypes_len: usize,
    row: usize,
) -> Option<TypeHierarchyVisibleRow> {
    if row == 0 {
        return Some(TypeHierarchyVisibleRow::SupertypesHeader);
    }

    let supertype_rows = supertypes_len.max(1);
    if row < 1 + supertype_rows {
        return Some(if supertypes_len == 0 {
            TypeHierarchyVisibleRow::SupertypesEmpty
        } else {
            let item_index = row - 1;
            TypeHierarchyVisibleRow::Supertype {
                target: TypeHierarchyOpenTarget::Supertype(item_index),
                selection_index: item_index,
            }
        });
    }

    let subtypes_header = 1usize.saturating_add(supertype_rows);
    if row == subtypes_header {
        return Some(TypeHierarchyVisibleRow::SubtypesHeader);
    }

    let subtype_index = row.saturating_sub(subtypes_header.saturating_add(1));
    if subtype_index < subtypes_len.max(1) {
        return Some(if subtypes_len == 0 {
            TypeHierarchyVisibleRow::SubtypesEmpty
        } else {
            TypeHierarchyVisibleRow::Subtype {
                target: TypeHierarchyOpenTarget::Subtype(subtype_index),
                selection_index: supertypes_len + subtype_index,
            }
        });
    }

    None
}

fn render_prepared_type_hierarchy_row(
    ui: &mut egui::Ui,
    selection_index: usize,
    target: &LspTypeHierarchyItem,
    label: &mut String,
    selected: &mut usize,
    open_target: &mut Option<LspTypeHierarchyItem>,
) {
    type_hierarchy_item_label_into(label, target);
    let response = ui.selectable_label(selection_index == *selected, label.as_str());
    if response.clicked() {
        *selected = selection_index;
    }
    if response.double_clicked() {
        *open_target = Some(target.clone());
    }
}

#[cfg(test)]
fn selected_type_hierarchy_item<'a>(
    supertypes: &'a [LspTypeHierarchyItem],
    subtypes: &'a [LspTypeHierarchyItem],
    selected: usize,
) -> Option<&'a LspTypeHierarchyItem> {
    selected_type_hierarchy_open_target(supertypes.len(), subtypes.len(), selected)
        .and_then(|target| type_hierarchy_open_target_item(supertypes, subtypes, target))
}

fn selected_type_hierarchy_open_target(
    supertypes_len: usize,
    subtypes_len: usize,
    selected: usize,
) -> Option<TypeHierarchyOpenTarget> {
    if selected < supertypes_len {
        Some(TypeHierarchyOpenTarget::Supertype(selected))
    } else {
        let subtype_index = selected.checked_sub(supertypes_len)?;
        (subtype_index < subtypes_len).then_some(TypeHierarchyOpenTarget::Subtype(subtype_index))
    }
}

fn type_hierarchy_open_target_item<'a>(
    supertypes: &'a [LspTypeHierarchyItem],
    subtypes: &'a [LspTypeHierarchyItem],
    target: TypeHierarchyOpenTarget,
) -> Option<&'a LspTypeHierarchyItem> {
    match target {
        TypeHierarchyOpenTarget::Supertype(index) => supertypes.get(index),
        TypeHierarchyOpenTarget::Subtype(index) => subtypes.get(index),
    }
}

fn type_hierarchy_item_label(item: &LspTypeHierarchyItem) -> String {
    let mut label = String::with_capacity(LSP_POPUP_LABEL_MAX_CHARS);
    type_hierarchy_item_label_into(&mut label, item);
    label
}

fn type_hierarchy_item_label_into(label: &mut String, item: &LspTypeHierarchyItem) {
    label.reserve(LSP_POPUP_LABEL_MAX_CHARS.saturating_sub(label.capacity()));
    lsp_popup_item_location_label_into(label, &item.name, &item.path, item.line, item.column);
}

#[cfg(test)]
mod tests {
    use super::{
        PreparedTypeHierarchyPopup, PreparedTypeHierarchyRow,
        TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION, TypeHierarchyOpenTarget,
        TypeHierarchyVisibleRow, selected_type_hierarchy_item, selected_type_hierarchy_open_target,
        type_hierarchy_item_label, type_hierarchy_item_label_into, type_hierarchy_open_target_item,
        type_hierarchy_visible_row, type_hierarchy_visible_row_count,
        type_hierarchy_visible_row_for_selection,
    };
    use crate::lsp_reference_popup::LSP_POPUP_LABEL_MAX_CHARS;
    use kuroya_core::LspTypeHierarchyItem;
    use serde_json::json;
    use std::path::PathBuf;

    fn item(name: &str, line: usize, column: usize) -> LspTypeHierarchyItem {
        item_with_raw(name, line, column, json!({}))
    }

    fn item_with_raw(
        name: &str,
        line: usize,
        column: usize,
        raw: serde_json::Value,
    ) -> LspTypeHierarchyItem {
        LspTypeHierarchyItem {
            name: name.to_owned(),
            detail: None,
            kind: 5,
            path: PathBuf::from("src/main.rs"),
            line,
            column,
            end_line: line,
            end_column: column + 4,
            raw,
        }
    }

    #[test]
    fn selected_type_hierarchy_item_spans_supertypes_then_subtypes() {
        let supertypes = vec![item("Base", 2, 4)];
        let subtypes = vec![item("Derived", 8, 6)];

        assert_eq!(
            selected_type_hierarchy_item(&supertypes, &subtypes, 0)
                .unwrap()
                .name,
            "Base"
        );
        assert_eq!(
            selected_type_hierarchy_item(&supertypes, &subtypes, 1)
                .unwrap()
                .name,
            "Derived"
        );
        assert!(selected_type_hierarchy_item(&supertypes, &subtypes, 2).is_none());
    }

    #[test]
    fn selected_type_hierarchy_open_target_tracks_group_and_index() {
        assert_eq!(
            selected_type_hierarchy_open_target(2, 2, 0),
            Some(TypeHierarchyOpenTarget::Supertype(0))
        );
        assert_eq!(
            selected_type_hierarchy_open_target(2, 2, 2),
            Some(TypeHierarchyOpenTarget::Subtype(0))
        );
        assert_eq!(selected_type_hierarchy_open_target(2, 2, 4), None);
    }

    #[test]
    fn type_hierarchy_open_target_resolves_original_lsp_item() {
        let supertypes = vec![item_with_raw("Base", 2, 4, json!({ "data": "base" }))];
        let subtypes = vec![item_with_raw(
            "Derived\u{202e}\nName",
            8,
            6,
            json!({ "data": "derived" }),
        )];

        let selected = type_hierarchy_open_target_item(
            &supertypes,
            &subtypes,
            TypeHierarchyOpenTarget::Subtype(0),
        )
        .unwrap();

        assert_eq!(selected.name, "Derived\u{202e}\nName");
        assert_eq!(selected.raw, json!({ "data": "derived" }));
        assert_eq!(
            type_hierarchy_item_label(selected),
            "Derived Name  main.rs:8:6"
        );
    }

    #[test]
    fn type_hierarchy_item_label_displays_one_based_lsp_locations() {
        assert_eq!(
            type_hierarchy_item_label(&item("Handler", 11, 7)),
            "Handler  main.rs:11:7"
        );
    }

    #[test]
    fn type_hierarchy_item_label_into_reserves_reusable_row_buffer() {
        let item = item("Handler", 11, 7);
        let mut label = String::new();

        type_hierarchy_item_label_into(&mut label, &item);

        assert_eq!(label, "Handler  main.rs:11:7");
        assert!(label.capacity() >= LSP_POPUP_LABEL_MAX_CHARS);
    }

    #[test]
    fn type_hierarchy_item_label_sanitizes_control_and_bidi_name_display_only() {
        let item = item("Handler\u{202e}\nName\t", 3, 5);
        let original_name = item.name.clone();
        let label = type_hierarchy_item_label(&item);

        assert_eq!(label, "Handler Name  main.rs:3:5");
        assert_eq!(item.name, original_name);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
    }

    #[test]
    fn type_hierarchy_item_label_uses_blank_name_fallback() {
        assert_eq!(
            type_hierarchy_item_label(&item("\u{202e}\n\t", 3, 5)),
            "Unnamed  main.rs:3:5"
        );
    }

    #[test]
    fn type_hierarchy_item_label_caps_huge_names() {
        let label =
            type_hierarchy_item_label(&item(&"a".repeat(LSP_POPUP_LABEL_MAX_CHARS + 64), 3, 5));

        assert!(label.chars().count() <= LSP_POPUP_LABEL_MAX_CHARS);
        assert!(label.contains("..."));
    }

    #[test]
    fn prepared_type_hierarchy_keeps_empty_sections_visible_after_prepare() {
        let root = item("Root", 3, 5);
        let prepared = PreparedTypeHierarchyPopup::build(
            Some(&root),
            Some(root.path.as_path()),
            root.line,
            root.column,
            &[],
            &[],
        );

        assert_eq!(prepared.row_count(), 0);
        assert_eq!(prepared.visible_row_count(), 4);
        assert_eq!(
            prepared.row(0),
            Some(PreparedTypeHierarchyRow::SupertypesHeader)
        );
        assert_eq!(
            prepared.row(1),
            Some(PreparedTypeHierarchyRow::SupertypesEmpty)
        );
        assert_eq!(
            prepared.row(2),
            Some(PreparedTypeHierarchyRow::SubtypesHeader)
        );
        assert_eq!(
            prepared.row(3),
            Some(PreparedTypeHierarchyRow::SubtypesEmpty)
        );
        assert_eq!(prepared.open_target_for_selection(0), None);
    }

    #[test]
    fn prepared_type_hierarchy_opens_raw_target_and_rejects_stale_selection() {
        let root = item("Root", 3, 5);
        let raw_name = "Derived\u{202e}\nName\t".to_owned();
        let raw_payload = json!({ "data": "raw\npayload\u{202e}" });
        let subtypes = vec![item_with_raw(&raw_name, 8, 6, raw_payload.clone())];
        let prepared = PreparedTypeHierarchyPopup::build(Some(&root), None, 0, 0, &[], &subtypes);

        assert_eq!(prepared.visible_row_for_selection(0), 3);
        let opened = prepared
            .open_target_for_selection(0)
            .expect("prepared open target");
        assert_eq!(opened.name, raw_name);
        assert_eq!(opened.raw, raw_payload);
        assert_eq!(prepared.open_target_for_selection(1), None);
        assert_eq!(prepared.visible_row_for_selection(1), 0);

        let label = match prepared.row(3).expect("subtype row") {
            PreparedTypeHierarchyRow::Subtype { target, .. } => {
                assert!(std::ptr::eq(target, &subtypes[0]));
                assert_eq!(target.name, raw_name);
                assert_eq!(target.raw, raw_payload);
                type_hierarchy_item_label(target)
            }
            row => panic!("expected subtype row, got {row:?}"),
        };
        assert!(label.chars().count() <= LSP_POPUP_LABEL_MAX_CHARS);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
    }

    #[test]
    fn prepared_type_hierarchy_caps_display_rows_and_selection_targets() {
        let root = item("Root", 3, 5);
        let supertypes = (0..=TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION)
            .map(|index| item(&format!("supertype-{index}"), 2, 4))
            .collect::<Vec<_>>();
        let subtypes = (0..=TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION)
            .map(|index| item(&format!("subtype-{index}"), 8, 6))
            .collect::<Vec<_>>();

        let prepared =
            PreparedTypeHierarchyPopup::build(Some(&root), None, 0, 0, &supertypes, &subtypes);

        assert_eq!(
            prepared.row_count(),
            TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION * 2
        );
        assert_eq!(
            prepared.visible_row_count(),
            TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION * 2 + 2
        );
        assert_eq!(
            prepared
                .open_target_for_selection(TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION * 2 - 1)
                .expect("last displayed subtype")
                .name,
            format!(
                "subtype-{}",
                TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION - 1
            )
        );
        assert!(
            prepared
                .open_target_for_selection(TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION * 2)
                .is_none()
        );
        assert_eq!(
            prepared.visible_row_for_selection(TYPE_HIERARCHY_MAX_DISPLAY_ITEMS_PER_SECTION * 2),
            0
        );
    }

    #[test]
    fn type_hierarchy_visible_rows_keep_selection_indices_on_items_only() {
        assert_eq!(type_hierarchy_visible_row_count(2, 1), 5);
        assert_eq!(
            type_hierarchy_visible_row(2, 1, 0),
            Some(TypeHierarchyVisibleRow::SupertypesHeader)
        );
        assert_eq!(
            type_hierarchy_visible_row(2, 1, 1),
            Some(TypeHierarchyVisibleRow::Supertype {
                target: TypeHierarchyOpenTarget::Supertype(0),
                selection_index: 0,
            })
        );
        assert_eq!(
            type_hierarchy_visible_row(2, 1, 3),
            Some(TypeHierarchyVisibleRow::SubtypesHeader)
        );
        assert_eq!(
            type_hierarchy_visible_row(2, 1, 4),
            Some(TypeHierarchyVisibleRow::Subtype {
                target: TypeHierarchyOpenTarget::Subtype(0),
                selection_index: 2,
            })
        );
    }

    #[test]
    fn type_hierarchy_visible_rows_include_empty_group_placeholders() {
        assert_eq!(type_hierarchy_visible_row_count(0, 3), 6);
        assert_eq!(
            type_hierarchy_visible_row(0, 3, 1),
            Some(TypeHierarchyVisibleRow::SupertypesEmpty)
        );
        assert_eq!(
            type_hierarchy_visible_row(0, 3, 3),
            Some(TypeHierarchyVisibleRow::Subtype {
                target: TypeHierarchyOpenTarget::Subtype(0),
                selection_index: 0,
            })
        );
    }

    #[test]
    fn type_hierarchy_selection_maps_to_visible_rows_with_headers() {
        assert_eq!(type_hierarchy_visible_row_for_selection(2, 1, 0), 1);
        assert_eq!(type_hierarchy_visible_row_for_selection(2, 1, 2), 4);
        assert_eq!(type_hierarchy_visible_row_for_selection(0, 3, 0), 3);
        assert_eq!(type_hierarchy_visible_row_for_selection(0, 0, 9), 0);
    }

    #[test]
    fn type_hierarchy_visible_row_count_saturates() {
        assert_eq!(
            type_hierarchy_visible_row_count(usize::MAX, usize::MAX),
            usize::MAX
        );
    }
}
