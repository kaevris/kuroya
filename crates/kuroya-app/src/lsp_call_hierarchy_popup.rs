use crate::{
    KuroyaApp,
    lsp_reference_popup::{
        LSP_POPUP_LABEL_MAX_CHARS, lsp_popup_bound_label, lsp_popup_item_location_label_into,
        lsp_popup_location_label_into,
    },
    popup_buttons::{PopupButtonKind, popup_button},
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Align, Context, Key, RichText, ScrollArea};
use kuroya_core::LspCallHierarchyCall;
use std::fmt::Write;

const CALL_HIERARCHY_ROW_HEIGHT: f32 = 24.0;
const CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION: usize = 500;

impl KuroyaApp {
    pub(crate) fn render_call_hierarchy_popup(&mut self, ctx: &Context) {
        let mut close = false;
        let mut open_target = None;
        let prepared = PreparedCallHierarchyPopup::build(
            self.call_hierarchy_root.as_ref(),
            self.call_hierarchy_path.as_deref(),
            self.call_hierarchy_line,
            self.call_hierarchy_column,
            &self.call_hierarchy_incoming,
            &self.call_hierarchy_outgoing,
        );
        let row_count = prepared.row_count();
        clamp_selection(&mut self.call_hierarchy_selected, row_count);

        egui::Window::new("Call Hierarchy")
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
                        &mut self.call_hierarchy_selected,
                        row_count,
                        selection_page_step(CALL_HIERARCHY_ROW_HEIGHT, viewport_height),
                    )
                });

                let visible_row_count = prepared.visible_row_count();
                ui.separator();
                if visible_row_count == 0 {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("No calls");
                    });
                    return;
                }

                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    open_target = prepared.open_target_for_selection(self.call_hierarchy_selected);
                }

                let mut scroll_area = ScrollArea::vertical();
                if selection_changed {
                    scroll_area = scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                        prepared.visible_row_for_selection(self.call_hierarchy_selected),
                        visible_row_count,
                        CALL_HIERARCHY_ROW_HEIGHT,
                        viewport_height,
                    ));
                }
                let mut row_display_cache = CallHierarchyRowDisplayCache::default();
                scroll_area.show_rows(
                    ui,
                    CALL_HIERARCHY_ROW_HEIGHT,
                    visible_row_count,
                    |ui, rows| {
                        for row in rows {
                            match prepared.row(row) {
                                Some(PreparedCallHierarchyRow::IncomingHeader) => {
                                    ui.label(RichText::new("Incoming Calls").strong());
                                }
                                Some(PreparedCallHierarchyRow::IncomingEmpty) => {
                                    ui.label(RichText::new("No incoming calls").small());
                                }
                                Some(PreparedCallHierarchyRow::IncomingCall {
                                    selection_index,
                                    target,
                                })
                                | Some(PreparedCallHierarchyRow::OutgoingCall {
                                    selection_index,
                                    target,
                                }) => {
                                    let label = row_display_cache.call_label(target);
                                    render_prepared_call_hierarchy_row(
                                        ui,
                                        selection_index,
                                        target,
                                        label,
                                        &mut self.call_hierarchy_selected,
                                        &mut open_target,
                                    );
                                }
                                Some(PreparedCallHierarchyRow::OutgoingHeader) => {
                                    ui.label(RichText::new("Outgoing Calls").strong());
                                }
                                Some(PreparedCallHierarchyRow::OutgoingEmpty) => {
                                    ui.label(RichText::new("No outgoing calls").small());
                                }
                                None => {}
                            }
                        }
                    },
                );
            });

        if close {
            self.clear_call_hierarchy();
            self.status = "Closed call hierarchy".to_owned();
        } else if let Some(call) = open_target {
            self.open_call_hierarchy_call(call);
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct PreparedCallHierarchyPopup<'a> {
    target_label: String,
    incoming: &'a [LspCallHierarchyCall],
    outgoing: &'a [LspCallHierarchyCall],
    row_count: usize,
    visible_row_count: usize,
}

impl<'a> PreparedCallHierarchyPopup<'a> {
    fn build(
        root: Option<&kuroya_core::LspCallHierarchyItem>,
        target_path: Option<&std::path::Path>,
        target_line: usize,
        target_column: usize,
        incoming: &'a [LspCallHierarchyCall],
        outgoing: &'a [LspCallHierarchyCall],
    ) -> Self {
        let incoming_len = call_hierarchy_display_len(incoming.len());
        let outgoing_len = call_hierarchy_display_len(outgoing.len());
        let incoming = &incoming[..incoming_len];
        let outgoing = &outgoing[..outgoing_len];
        let visible_row_count = if root.is_some() || incoming_len > 0 || outgoing_len > 0 {
            call_hierarchy_visible_row_count(incoming_len, outgoing_len)
        } else {
            0
        };

        Self {
            target_label: call_hierarchy_target_label(
                root,
                target_path,
                target_line,
                target_column,
            ),
            incoming,
            outgoing,
            row_count: incoming_len.saturating_add(outgoing_len),
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

    fn row(&self, row: usize) -> Option<PreparedCallHierarchyRow<'a>> {
        if row >= self.visible_row_count {
            return None;
        }

        match call_hierarchy_visible_row(self.incoming.len(), self.outgoing.len(), row)? {
            CallHierarchyVisibleRow::IncomingHeader => {
                Some(PreparedCallHierarchyRow::IncomingHeader)
            }
            CallHierarchyVisibleRow::IncomingEmpty => Some(PreparedCallHierarchyRow::IncomingEmpty),
            CallHierarchyVisibleRow::IncomingCall {
                call_index,
                selection_index,
            } => Some(PreparedCallHierarchyRow::IncomingCall {
                selection_index,
                target: self.incoming.get(call_index)?,
            }),
            CallHierarchyVisibleRow::OutgoingHeader => {
                Some(PreparedCallHierarchyRow::OutgoingHeader)
            }
            CallHierarchyVisibleRow::OutgoingEmpty => Some(PreparedCallHierarchyRow::OutgoingEmpty),
            CallHierarchyVisibleRow::OutgoingCall {
                call_index,
                selection_index,
            } => Some(PreparedCallHierarchyRow::OutgoingCall {
                selection_index,
                target: self.outgoing.get(call_index)?,
            }),
        }
    }

    fn visible_row_for_selection(&self, selected: usize) -> usize {
        if selected >= self.row_count {
            return 0;
        }

        call_hierarchy_visible_row_for_selection(self.incoming.len(), self.outgoing.len(), selected)
    }

    fn open_target_for_selection(&self, selected: usize) -> Option<LspCallHierarchyCall> {
        if selected >= self.row_count {
            return None;
        }

        let target = call_hierarchy_open_target_for_selection(
            self.incoming.len(),
            self.outgoing.len(),
            selected,
        )?;
        call_hierarchy_call_for_target(self.incoming, self.outgoing, target).cloned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum PreparedCallHierarchyRow<'a> {
    IncomingHeader,
    IncomingEmpty,
    IncomingCall {
        selection_index: usize,
        target: &'a LspCallHierarchyCall,
    },
    OutgoingHeader,
    OutgoingEmpty,
    OutgoingCall {
        selection_index: usize,
        target: &'a LspCallHierarchyCall,
    },
}

fn call_hierarchy_target_label(
    root: Option<&kuroya_core::LspCallHierarchyItem>,
    target_path: Option<&std::path::Path>,
    target_line: usize,
    target_column: usize,
) -> String {
    if let Some(item) = root {
        call_hierarchy_item_label(item)
    } else if let Some(path) = target_path {
        let mut target = String::new();
        lsp_popup_location_label_into(&mut target, path, target_line, target_column);
        target
    } else {
        "No target".to_owned()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallHierarchyVisibleRow {
    IncomingHeader,
    IncomingEmpty,
    IncomingCall {
        call_index: usize,
        selection_index: usize,
    },
    OutgoingHeader,
    OutgoingEmpty,
    OutgoingCall {
        call_index: usize,
        selection_index: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CallHierarchyOpenTarget {
    Incoming(usize),
    Outgoing(usize),
}

fn call_hierarchy_visible_row_count(incoming_len: usize, outgoing_len: usize) -> usize {
    2usize
        .saturating_add(incoming_len.max(1))
        .saturating_add(outgoing_len.max(1))
}

fn call_hierarchy_display_len(len: usize) -> usize {
    len.min(CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION)
}

fn call_hierarchy_visible_row_for_selection(
    incoming_len: usize,
    outgoing_len: usize,
    selected: usize,
) -> usize {
    let row_count = incoming_len + outgoing_len;
    if row_count == 0 {
        return 0;
    }

    let selected = selected.min(row_count - 1);
    if selected < incoming_len {
        1 + selected
    } else {
        2 + incoming_len.max(1) + selected.saturating_sub(incoming_len)
    }
}

fn call_hierarchy_visible_row(
    incoming_len: usize,
    outgoing_len: usize,
    row: usize,
) -> Option<CallHierarchyVisibleRow> {
    if row == 0 {
        return Some(CallHierarchyVisibleRow::IncomingHeader);
    }

    let incoming_rows = incoming_len.max(1);
    if row < 1 + incoming_rows {
        return Some(if incoming_len == 0 {
            CallHierarchyVisibleRow::IncomingEmpty
        } else {
            let call_index = row - 1;
            CallHierarchyVisibleRow::IncomingCall {
                call_index,
                selection_index: call_index,
            }
        });
    }

    let outgoing_header = 1usize.saturating_add(incoming_rows);
    if row == outgoing_header {
        return Some(CallHierarchyVisibleRow::OutgoingHeader);
    }

    let outgoing_index = row.saturating_sub(outgoing_header.saturating_add(1));
    if outgoing_index < outgoing_len.max(1) {
        return Some(if outgoing_len == 0 {
            CallHierarchyVisibleRow::OutgoingEmpty
        } else {
            CallHierarchyVisibleRow::OutgoingCall {
                call_index: outgoing_index,
                selection_index: incoming_len + outgoing_index,
            }
        });
    }

    None
}

#[derive(Default)]
struct CallHierarchyRowDisplayCache {
    label: String,
}

impl CallHierarchyRowDisplayCache {
    fn call_label(&mut self, call: &LspCallHierarchyCall) -> &str {
        call_hierarchy_call_label_into(&mut self.label, call);
        self.label.as_str()
    }

    #[cfg(test)]
    fn visible_call_row(
        &mut self,
        incoming: &[LspCallHierarchyCall],
        outgoing: &[LspCallHierarchyCall],
        visible_row: CallHierarchyVisibleRow,
    ) -> Option<CallHierarchyCallRowDisplay<'_>> {
        match visible_row {
            CallHierarchyVisibleRow::IncomingCall {
                call_index,
                selection_index,
            } => Some(self.call_row(
                selection_index,
                CallHierarchyOpenTarget::Incoming(call_index),
                incoming.get(call_index)?,
            )),
            CallHierarchyVisibleRow::OutgoingCall {
                call_index,
                selection_index,
            } => Some(self.call_row(
                selection_index,
                CallHierarchyOpenTarget::Outgoing(call_index),
                outgoing.get(call_index)?,
            )),
            _ => None,
        }
    }

    #[cfg(test)]
    fn call_row(
        &mut self,
        selection_index: usize,
        open_target: CallHierarchyOpenTarget,
        call: &LspCallHierarchyCall,
    ) -> CallHierarchyCallRowDisplay<'_> {
        let label = self.call_label(call);
        CallHierarchyCallRowDisplay {
            selection_index,
            open_target,
            label,
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CallHierarchyCallRowDisplay<'a> {
    selection_index: usize,
    open_target: CallHierarchyOpenTarget,
    label: &'a str,
}

fn render_prepared_call_hierarchy_row(
    ui: &mut egui::Ui,
    selection_index: usize,
    target: &LspCallHierarchyCall,
    label: &str,
    selected: &mut usize,
    open_target: &mut Option<LspCallHierarchyCall>,
) {
    let response = ui.selectable_label(selection_index == *selected, label);
    if response.clicked() {
        *selected = selection_index;
    }
    if response.double_clicked() {
        *open_target = Some(target.clone());
    }
}

fn call_hierarchy_open_target_for_selection(
    incoming_len: usize,
    outgoing_len: usize,
    selected: usize,
) -> Option<CallHierarchyOpenTarget> {
    if selected < incoming_len {
        return Some(CallHierarchyOpenTarget::Incoming(selected));
    }

    let outgoing_index = selected.checked_sub(incoming_len)?;
    if outgoing_index < outgoing_len {
        Some(CallHierarchyOpenTarget::Outgoing(outgoing_index))
    } else {
        None
    }
}

fn call_hierarchy_call_for_target<'a>(
    incoming: &'a [LspCallHierarchyCall],
    outgoing: &'a [LspCallHierarchyCall],
    target: CallHierarchyOpenTarget,
) -> Option<&'a LspCallHierarchyCall> {
    match target {
        CallHierarchyOpenTarget::Incoming(index) => incoming.get(index),
        CallHierarchyOpenTarget::Outgoing(index) => outgoing.get(index),
    }
}

#[cfg(test)]
fn selected_call_hierarchy_call<'a>(
    incoming: &'a [LspCallHierarchyCall],
    outgoing: &'a [LspCallHierarchyCall],
    selected: usize,
) -> Option<&'a LspCallHierarchyCall> {
    let target =
        call_hierarchy_open_target_for_selection(incoming.len(), outgoing.len(), selected)?;
    call_hierarchy_call_for_target(incoming, outgoing, target)
}

fn call_hierarchy_item_label(item: &kuroya_core::LspCallHierarchyItem) -> String {
    let mut label = String::new();
    call_hierarchy_item_label_into(&mut label, item);
    label
}

fn call_hierarchy_item_label_into(label: &mut String, item: &kuroya_core::LspCallHierarchyItem) {
    lsp_popup_item_location_label_into(label, &item.name, &item.path, item.line, item.column);
}

#[cfg(test)]
fn call_hierarchy_call_label(call: &LspCallHierarchyCall) -> String {
    let mut label = String::new();
    call_hierarchy_call_label_into(&mut label, call);
    label
}

fn call_hierarchy_call_label_into(label: &mut String, call: &LspCallHierarchyCall) {
    label.clear();
    let label_capacity = call_hierarchy_call_label_capacity(call);
    if label.capacity() < label_capacity {
        label.reserve(label_capacity - label.capacity());
    }
    call_hierarchy_item_label_into(label, &call.item);
    let range_count = call.ranges.len();
    if range_count == 1 {
        label.push_str("  1 call site");
    } else {
        let _ = write!(label, "  {range_count} call sites");
    }
    lsp_popup_bound_label(label, LSP_POPUP_LABEL_MAX_CHARS);
}

fn call_hierarchy_call_label_capacity(call: &LspCallHierarchyCall) -> usize {
    let path_len = call
        .item
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .map_or(1, |name| name.len().min(LSP_POPUP_LABEL_MAX_CHARS));
    let location_len = decimal_digits(call.item.line) + decimal_digits(call.item.column) + 2;
    let call_site_len = if call.ranges.len() == 1 {
        "  1 call site".len()
    } else {
        "   call sites".len() + decimal_digits(call.ranges.len())
    };

    call.item
        .name
        .len()
        .min(LSP_POPUP_LABEL_MAX_CHARS)
        .saturating_add("  ".len())
        .saturating_add(path_len)
        .saturating_add(location_len)
        .saturating_add(call_site_len)
        .min(LSP_POPUP_LABEL_MAX_CHARS.saturating_add(call_site_len))
}

fn decimal_digits(value: usize) -> usize {
    let mut digits = 1;
    let mut remaining = value;
    while remaining >= 10 {
        remaining /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::{
        CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION, CallHierarchyCallRowDisplay,
        CallHierarchyOpenTarget, CallHierarchyRowDisplayCache, CallHierarchyVisibleRow,
        PreparedCallHierarchyPopup, PreparedCallHierarchyRow, call_hierarchy_call_label,
        call_hierarchy_call_label_capacity, call_hierarchy_item_label,
        call_hierarchy_open_target_for_selection, call_hierarchy_visible_row,
        call_hierarchy_visible_row_count, call_hierarchy_visible_row_for_selection,
        selected_call_hierarchy_call,
    };
    use crate::lsp_reference_popup::LSP_POPUP_LABEL_MAX_CHARS;
    use kuroya_core::{LspCallHierarchyCall, LspCallHierarchyItem, LspCallHierarchyRange};
    use serde_json::json;
    use std::path::PathBuf;

    fn item(name: &str, line: usize, column: usize) -> LspCallHierarchyItem {
        LspCallHierarchyItem {
            name: name.to_owned(),
            detail: None,
            kind: 12,
            path: PathBuf::from("src/main.rs"),
            line,
            column,
            end_line: line,
            end_column: column + 4,
            raw: json!({}),
        }
    }

    fn call(name: &str) -> LspCallHierarchyCall {
        LspCallHierarchyCall {
            item: item(name, 3, 5),
            ranges: Vec::new(),
        }
    }

    #[test]
    fn selected_call_hierarchy_call_spans_incoming_then_outgoing() {
        let incoming = vec![call("incoming")];
        let outgoing = vec![call("outgoing")];

        assert_eq!(
            selected_call_hierarchy_call(&incoming, &outgoing, 0)
                .unwrap()
                .item
                .name,
            "incoming"
        );
        assert_eq!(
            selected_call_hierarchy_call(&incoming, &outgoing, 1)
                .unwrap()
                .item
                .name,
            "outgoing"
        );
        assert!(selected_call_hierarchy_call(&incoming, &outgoing, 2).is_none());
    }

    #[test]
    fn call_hierarchy_open_target_for_selection_checks_group_bounds() {
        assert_eq!(
            call_hierarchy_open_target_for_selection(2, 1, 0),
            Some(CallHierarchyOpenTarget::Incoming(0))
        );
        assert_eq!(
            call_hierarchy_open_target_for_selection(2, 1, 2),
            Some(CallHierarchyOpenTarget::Outgoing(0))
        );
        assert_eq!(
            call_hierarchy_open_target_for_selection(0, 1, 0),
            Some(CallHierarchyOpenTarget::Outgoing(0))
        );
        assert_eq!(call_hierarchy_open_target_for_selection(2, 1, 3), None);
        assert_eq!(call_hierarchy_open_target_for_selection(0, 0, 0), None);
    }

    #[test]
    fn call_hierarchy_item_label_uses_one_based_lsp_locations_directly() {
        assert_eq!(
            call_hierarchy_item_label(&item("handler", 12, 8)),
            "handler  main.rs:12:8"
        );
    }

    #[test]
    fn call_hierarchy_item_label_sanitizes_control_and_bidi_name_display_only() {
        let item = item("handler\u{202e}\nname\t", 3, 5);
        let original_name = item.name.clone();
        let label = call_hierarchy_item_label(&item);

        assert_eq!(label, "handler name  main.rs:3:5");
        assert_eq!(item.name, original_name);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
    }

    #[test]
    fn call_hierarchy_item_label_uses_blank_name_fallback() {
        assert_eq!(
            call_hierarchy_item_label(&item("\u{202e}\n\t", 3, 5)),
            "Unnamed  main.rs:3:5"
        );
    }

    #[test]
    fn call_hierarchy_call_label_caps_huge_names() {
        let label = call_hierarchy_call_label(&call(&"a".repeat(LSP_POPUP_LABEL_MAX_CHARS + 64)));

        assert!(label.chars().count() <= LSP_POPUP_LABEL_MAX_CHARS);
        assert!(label.contains("..."));
        assert!(label.contains("0 call sites"));
    }

    #[test]
    fn call_hierarchy_call_label_capacity_covers_visible_label_text() {
        let mut call = call("handler");
        call.ranges = vec![
            LspCallHierarchyRange {
                line: 1,
                column: 2,
                end_line: 1,
                end_column: 3,
            },
            LspCallHierarchyRange {
                line: 4,
                column: 5,
                end_line: 4,
                end_column: 6,
            },
        ];

        let label = call_hierarchy_call_label(&call);

        assert_eq!(label, "handler  main.rs:3:5  2 call sites");
        assert!(call_hierarchy_call_label_capacity(&call) >= label.len());
    }

    #[test]
    fn call_hierarchy_visible_call_row_display_reuses_sanitized_labels_for_selection() {
        let incoming = vec![call("incoming\u{202e}\nname\t")];
        let outgoing = vec![call("outgoing\u{202e}\nname\t")];
        let incoming_name = incoming[0].item.name.clone();
        let outgoing_name = outgoing[0].item.name.clone();
        let mut cache = CallHierarchyRowDisplayCache::default();

        let incoming_label = {
            let display = cache
                .visible_call_row(
                    &incoming,
                    &outgoing,
                    CallHierarchyVisibleRow::IncomingCall {
                        call_index: 0,
                        selection_index: 0,
                    },
                )
                .unwrap();

            assert_eq!(
                display,
                CallHierarchyCallRowDisplay {
                    selection_index: 0,
                    open_target: CallHierarchyOpenTarget::Incoming(0),
                    label: "incoming name  main.rs:3:5  0 call sites",
                }
            );
            display.label.to_owned()
        };

        let outgoing_label = {
            let display = cache
                .visible_call_row(
                    &incoming,
                    &outgoing,
                    CallHierarchyVisibleRow::OutgoingCall {
                        call_index: 0,
                        selection_index: 1,
                    },
                )
                .unwrap();

            assert_eq!(
                display,
                CallHierarchyCallRowDisplay {
                    selection_index: 1,
                    open_target: CallHierarchyOpenTarget::Outgoing(0),
                    label: "outgoing name  main.rs:3:5  0 call sites",
                }
            );
            display.label.to_owned()
        };

        assert_eq!(incoming[0].item.name, incoming_name);
        assert_eq!(outgoing[0].item.name, outgoing_name);
        for label in [incoming_label, outgoing_label] {
            assert!(!label.chars().any(char::is_control));
            assert!(!label.contains('\u{202e}'));
        }
    }

    #[test]
    fn prepared_call_hierarchy_keeps_empty_sections_visible_after_prepare() {
        let root = item("Root", 3, 5);
        let prepared = PreparedCallHierarchyPopup::build(
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
            Some(PreparedCallHierarchyRow::IncomingHeader)
        );
        assert_eq!(
            prepared.row(1),
            Some(PreparedCallHierarchyRow::IncomingEmpty)
        );
        assert_eq!(
            prepared.row(2),
            Some(PreparedCallHierarchyRow::OutgoingHeader)
        );
        assert_eq!(
            prepared.row(3),
            Some(PreparedCallHierarchyRow::OutgoingEmpty)
        );
        assert_eq!(prepared.open_target_for_selection(0), None);
    }

    #[test]
    fn prepared_call_hierarchy_opens_raw_target_and_rejects_stale_selection() {
        let root = item("Root", 3, 5);
        let raw_name = "incoming\u{202e}\nname\t".to_owned();
        let raw_payload = json!({ "data": "raw\npayload\u{202e}" });
        let mut incoming_call = call(&raw_name);
        incoming_call.item.raw = raw_payload.clone();
        let incoming = vec![incoming_call.clone()];
        let prepared = PreparedCallHierarchyPopup::build(Some(&root), None, 0, 0, &incoming, &[]);

        assert_eq!(prepared.visible_row_for_selection(0), 1);
        let opened = prepared
            .open_target_for_selection(0)
            .expect("prepared open target");
        assert_eq!(opened.item.name, raw_name);
        assert_eq!(opened.item.raw, raw_payload);
        assert_eq!(prepared.open_target_for_selection(1), None);

        let target = match prepared.row(1).expect("incoming call row") {
            PreparedCallHierarchyRow::IncomingCall {
                selection_index,
                target,
            } => {
                assert_eq!(selection_index, 0);
                assert!(std::ptr::eq(target, &incoming[0]));
                target
            }
            row => panic!("expected incoming call row, got {row:?}"),
        };
        let mut cache = CallHierarchyRowDisplayCache::default();
        let label = cache.call_label(target);
        assert!(label.chars().count() <= LSP_POPUP_LABEL_MAX_CHARS);
        assert!(!label.chars().any(char::is_control));
        assert!(!label.contains('\u{202e}'));
    }

    #[test]
    fn prepared_call_hierarchy_caps_display_rows_and_selection_targets() {
        let root = item("Root", 3, 5);
        let incoming = (0..=CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION)
            .map(|index| call(&format!("incoming-{index}")))
            .collect::<Vec<_>>();
        let outgoing = (0..=CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION)
            .map(|index| call(&format!("outgoing-{index}")))
            .collect::<Vec<_>>();

        let prepared =
            PreparedCallHierarchyPopup::build(Some(&root), None, 0, 0, &incoming, &outgoing);

        assert_eq!(
            prepared.row_count(),
            CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION * 2
        );
        assert_eq!(
            prepared.visible_row_count(),
            CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION * 2 + 2
        );
        assert_eq!(
            prepared
                .open_target_for_selection(CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION * 2 - 1)
                .expect("last displayed outgoing call")
                .item
                .name,
            format!(
                "outgoing-{}",
                CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION - 1
            )
        );
        assert!(
            prepared
                .open_target_for_selection(CALL_HIERARCHY_MAX_DISPLAY_CALLS_PER_DIRECTION * 2)
                .is_none()
        );
    }

    #[test]
    fn call_hierarchy_visible_rows_keep_selection_indices_on_calls_only() {
        assert_eq!(call_hierarchy_visible_row_count(2, 1), 5);
        assert_eq!(
            call_hierarchy_visible_row(2, 1, 0),
            Some(CallHierarchyVisibleRow::IncomingHeader)
        );
        assert_eq!(
            call_hierarchy_visible_row(2, 1, 1),
            Some(CallHierarchyVisibleRow::IncomingCall {
                call_index: 0,
                selection_index: 0,
            })
        );
        assert_eq!(
            call_hierarchy_visible_row(2, 1, 3),
            Some(CallHierarchyVisibleRow::OutgoingHeader)
        );
        assert_eq!(
            call_hierarchy_visible_row(2, 1, 4),
            Some(CallHierarchyVisibleRow::OutgoingCall {
                call_index: 0,
                selection_index: 2,
            })
        );
    }

    #[test]
    fn call_hierarchy_visible_rows_include_empty_group_placeholders() {
        assert_eq!(call_hierarchy_visible_row_count(0, 3), 6);
        assert_eq!(
            call_hierarchy_visible_row(0, 3, 1),
            Some(CallHierarchyVisibleRow::IncomingEmpty)
        );
        assert_eq!(
            call_hierarchy_visible_row(0, 3, 3),
            Some(CallHierarchyVisibleRow::OutgoingCall {
                call_index: 0,
                selection_index: 0,
            })
        );
    }

    #[test]
    fn call_hierarchy_selection_maps_to_visible_rows_with_headers() {
        assert_eq!(call_hierarchy_visible_row_for_selection(2, 1, 0), 1);
        assert_eq!(call_hierarchy_visible_row_for_selection(2, 1, 2), 4);
        assert_eq!(call_hierarchy_visible_row_for_selection(0, 3, 0), 3);
        assert_eq!(call_hierarchy_visible_row_for_selection(0, 0, 9), 0);
    }

    #[test]
    fn call_hierarchy_visible_row_count_saturates() {
        assert_eq!(
            call_hierarchy_visible_row_count(usize::MAX, usize::MAX),
            usize::MAX
        );
    }
}
