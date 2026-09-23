use crate::{
    KuroyaApp,
    lsp_code_actions::code_action_display_label,
    path_display::display_path_label_cow,
    popup_buttons::{PopupButtonKind, popup_button},
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Align, Context, Id, Key, RichText, ScrollArea};
use kuroya_core::{
    LspCodeAction, LspTextEdit, LspWorkspaceDocumentChange, LspWorkspaceResourceOperation,
};
use std::{
    collections::hash_map::DefaultHasher,
    fmt::Write as _,
    hash::{Hash, Hasher},
    path::Path,
    sync::Arc,
};

const CODE_ACTION_POPUP_DISPLAY_CACHE_ID: &str = "kuroya.code_action_popup.display_cache";
const MAX_CODE_ACTION_POPUP_ITEMS: usize = 250;

impl KuroyaApp {
    pub(crate) fn render_code_actions_popup(&mut self, ctx: &Context) {
        let mut close = false;
        let mut apply_item = None;

        egui::Window::new("Code Actions")
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 156.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 132.0])
            .default_size([560.0, 260.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let target = code_actions_target_label(
                        self.code_actions_path.as_deref(),
                        self.code_actions_line,
                        self.code_actions_column,
                    );
                    ui.label(
                        RichText::new(target)
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
                let row_height = ui.spacing().interact_size.y;
                let visible_action_count = visible_code_action_count(self.code_actions.len());
                let navigation_page_step = selection_page_step(row_height, ui.available_height());
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.code_actions_selected,
                        visible_action_count,
                        navigation_page_step,
                    )
                });
                let enter_pressed = ui.input(|input| input.key_pressed(Key::Enter));

                ui.separator();
                if self.code_actions.is_empty() {
                    ui.add_space(24.0);
                    ui.centered_and_justified(|ui| {
                        ui.label("No code actions");
                    });
                } else {
                    let items = cached_code_action_popup_items(ctx, &self.code_actions);
                    let action_count = items.len();
                    clamp_selection(&mut self.code_actions_selected, action_count);
                    if enter_pressed {
                        if items.get(self.code_actions_selected).is_some() {
                            apply_item = Some((Arc::clone(&items), self.code_actions_selected));
                        }
                    }
                    let mut scroll_area = ScrollArea::vertical();
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                self.code_actions_selected,
                                action_count,
                                row_height,
                                ui.available_height(),
                            ));
                    }
                    scroll_area.show_rows(ui, row_height, action_count, |ui, rows| {
                        let mut selected = self.code_actions_selected;
                        for idx in rows {
                            let Some(item) = items.get(idx) else {
                                continue;
                            };
                            let row = CodeActionPopupRowDisplay::new(
                                item.target.index,
                                idx == selected,
                                item.label.as_str(),
                            );
                            let response = row.show(ui);
                            if response.clicked() {
                                selected = row.index;
                                apply_item = Some((Arc::clone(&items), idx));
                            }
                        }
                        self.code_actions_selected = selected;
                    });
                    if self.code_actions.len() > action_count {
                        ui.label(
                            RichText::new(code_action_limit_label(
                                action_count,
                                self.code_actions.len(),
                            ))
                            .small()
                            .color(ui.visuals().weak_text_color()),
                        );
                    }
                }
            });

        if close {
            self.clear_lsp_code_action_state();
            clear_cached_code_action_popup_items(ctx);
            self.status = "Closed code actions".to_owned();
        } else if let Some((items, item_index)) = apply_item {
            clear_cached_code_action_popup_items(ctx);
            if let Some(action) =
                take_cached_code_action_popup_item(&items, item_index, &mut self.code_actions)
            {
                self.apply_code_action(action);
            } else {
                self.status = "Code action selection changed before apply; choose the action again"
                    .to_owned();
            }
        }
    }
}

fn take_cached_code_action_popup_item(
    items: &[CodeActionPopupDisplayItem],
    item_index: usize,
    actions: &mut Vec<LspCodeAction>,
) -> Option<LspCodeAction> {
    items
        .get(item_index)
        .and_then(|item| item.target.take_matching_action(actions))
}

#[derive(Clone, Debug)]
struct CodeActionPopupDisplayItem {
    target: CodeActionPopupApplyTarget,
    label: String,
}

impl CodeActionPopupDisplayItem {
    fn new(index: usize, action: &LspCodeAction) -> Self {
        Self {
            target: CodeActionPopupApplyTarget::new(index, action),
            label: code_action_display_label(action),
        }
    }
}

#[derive(Clone, Debug)]
struct CodeActionPopupApplyTarget {
    index: usize,
    source: CodeActionPopupDisplaySource,
}

impl CodeActionPopupApplyTarget {
    fn new(index: usize, action: &LspCodeAction) -> Self {
        Self {
            index,
            source: CodeActionPopupDisplaySource::new(action),
        }
    }

    fn take_matching_action(&self, actions: &mut Vec<LspCodeAction>) -> Option<LspCodeAction> {
        if !actions
            .get(self.index)
            .is_some_and(|action| self.matches_action(action))
        {
            return None;
        }

        Some(actions.swap_remove(self.index))
    }

    fn matches_action(&self, action: &LspCodeAction) -> bool {
        self.source.matches(action)
    }
}

struct CodeActionPopupRowDisplay<'a> {
    index: usize,
    selected: bool,
    label: &'a str,
}

impl<'a> CodeActionPopupRowDisplay<'a> {
    fn new(index: usize, selected: bool, label: &'a str) -> Self {
        Self {
            index,
            selected,
            label,
        }
    }

    fn show(&self, ui: &mut egui::Ui) -> egui::Response {
        ui.selectable_label(self.selected, self.label)
    }
}

fn cached_code_action_popup_items(
    ctx: &Context,
    actions: &[LspCodeAction],
) -> Arc<Vec<CodeActionPopupDisplayItem>> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<CodeActionPopupDisplayCache>(Id::new(
            CODE_ACTION_POPUP_DISPLAY_CACHE_ID,
        ))
        .items_for(actions)
    })
}

fn clear_cached_code_action_popup_items(ctx: &Context) {
    ctx.data_mut(|data| {
        data.remove::<CodeActionPopupDisplayCache>(Id::new(CODE_ACTION_POPUP_DISPLAY_CACHE_ID));
    });
}

#[derive(Clone, Default)]
struct CodeActionPopupDisplayCache {
    items: Arc<Vec<CodeActionPopupDisplayItem>>,
}

impl CodeActionPopupDisplayCache {
    fn items_for(&mut self, actions: &[LspCodeAction]) -> Arc<Vec<CodeActionPopupDisplayItem>> {
        if self.matches(actions) {
            return Arc::clone(&self.items);
        }

        self.items = Arc::new(
            actions
                .iter()
                .take(visible_code_action_count(actions.len()))
                .enumerate()
                .map(|(index, action)| CodeActionPopupDisplayItem::new(index, action))
                .collect(),
        );
        Arc::clone(&self.items)
    }

    fn matches(&self, actions: &[LspCodeAction]) -> bool {
        self.items.len() == visible_code_action_count(actions.len())
            && self
                .items
                .iter()
                .zip(actions.iter().take(self.items.len()))
                .all(|(item, action)| item.target.matches_action(action))
    }
}

fn visible_code_action_count(total: usize) -> usize {
    total.min(MAX_CODE_ACTION_POPUP_ITEMS)
}

#[derive(Clone, Debug)]
struct CodeActionPopupDisplaySource {
    title: String,
    kind: Option<String>,
    edit_count: usize,
    document_change_count: usize,
    payload_fingerprint: CodeActionPayloadFingerprint,
    needs_resolve: bool,
    resolve_payload: Option<Arc<serde_json::Value>>,
}

impl CodeActionPopupDisplaySource {
    fn new(action: &LspCodeAction) -> Self {
        Self {
            title: action.title.clone(),
            kind: action.kind.clone(),
            edit_count: action.edits.len(),
            document_change_count: action.document_changes.len(),
            payload_fingerprint: code_action_payload_fingerprint(action),
            needs_resolve: action.needs_resolve(),
            resolve_payload: action.resolve_payload.clone(),
        }
    }

    fn matches(&self, action: &LspCodeAction) -> bool {
        self.title == action.title
            && self.kind.as_deref() == action.kind.as_deref()
            && self.edit_count == action.edits.len()
            && self.document_change_count == action.document_changes.len()
            && self.payload_fingerprint == code_action_payload_fingerprint(action)
            && self.needs_resolve == action.needs_resolve()
            && resolve_payload_matches(&self.resolve_payload, &action.resolve_payload)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CodeActionPayloadFingerprint {
    hash: u64,
    text_bytes: usize,
}

fn code_action_payload_fingerprint(action: &LspCodeAction) -> CodeActionPayloadFingerprint {
    let mut hasher = DefaultHasher::new();
    hash_text_edits(&action.edits, &mut hasher);
    hash_document_changes(&action.document_changes, &mut hasher);
    CodeActionPayloadFingerprint {
        hash: hasher.finish(),
        text_bytes: code_action_payload_text_bytes(action),
    }
}

fn hash_text_edits(edits: &[LspTextEdit], state: &mut impl Hasher) {
    edits.len().hash(state);
    for edit in edits {
        edit.path.hash(state);
        edit.start_line.hash(state);
        edit.start_column.hash(state);
        edit.end_line.hash(state);
        edit.end_column.hash(state);
        edit.new_text.hash(state);
    }
}

fn hash_document_changes(changes: &[LspWorkspaceDocumentChange], state: &mut impl Hasher) {
    changes.len().hash(state);
    for change in changes {
        match change {
            LspWorkspaceDocumentChange::TextEdit {
                path,
                version,
                edits,
            } => {
                0u8.hash(state);
                path.hash(state);
                version.hash(state);
                hash_text_edits(edits, state);
            }
            LspWorkspaceDocumentChange::Resource(operation) => {
                1u8.hash(state);
                hash_resource_operation(operation, state);
            }
        }
    }
}

fn hash_resource_operation(operation: &LspWorkspaceResourceOperation, state: &mut impl Hasher) {
    match operation {
        LspWorkspaceResourceOperation::CreateFile {
            path,
            overwrite,
            ignore_if_exists,
        } => {
            0u8.hash(state);
            path.hash(state);
            overwrite.hash(state);
            ignore_if_exists.hash(state);
        }
        LspWorkspaceResourceOperation::RenameFile {
            old_path,
            new_path,
            overwrite,
            ignore_if_exists,
        } => {
            1u8.hash(state);
            old_path.hash(state);
            new_path.hash(state);
            overwrite.hash(state);
            ignore_if_exists.hash(state);
        }
        LspWorkspaceResourceOperation::DeleteFile {
            path,
            recursive,
            ignore_if_not_exists,
        } => {
            2u8.hash(state);
            path.hash(state);
            recursive.hash(state);
            ignore_if_not_exists.hash(state);
        }
    }
}

fn code_action_payload_text_bytes(action: &LspCodeAction) -> usize {
    let mut bytes = text_edit_bytes(&action.edits);
    for change in &action.document_changes {
        bytes += document_change_text_bytes(change);
    }
    bytes
}

fn text_edit_bytes(edits: &[LspTextEdit]) -> usize {
    let mut bytes = 0;
    for edit in edits {
        bytes += edit.new_text.len();
    }
    bytes
}

fn document_change_text_bytes(change: &LspWorkspaceDocumentChange) -> usize {
    match change {
        LspWorkspaceDocumentChange::TextEdit { edits, .. } => text_edit_bytes(edits),
        LspWorkspaceDocumentChange::Resource(_) => 0,
    }
}

fn resolve_payload_matches(
    source: &Option<Arc<serde_json::Value>>,
    action: &Option<Arc<serde_json::Value>>,
) -> bool {
    match (source, action) {
        (Some(source), Some(action)) => Arc::ptr_eq(source, action),
        (None, None) => true,
        _ => false,
    }
}

fn code_actions_target_label(path: Option<&Path>, line: usize, column: usize) -> String {
    let Some(path) = path else {
        return "No target".to_owned();
    };

    let path = display_path_label_cow(path);
    let mut label = String::with_capacity(path.len() + 24);
    label.push_str(&path);
    let _ = write!(label, ":{line}:{column}");
    label
}

fn code_action_limit_label(action_count: usize, total_count: usize) -> String {
    let mut label = String::with_capacity("Showing first  of  code actions".len() + 40);
    let _ = write!(
        label,
        "Showing first {action_count} of {total_count} code actions"
    );
    label
}

#[cfg(test)]
mod tests {
    use super::{
        CodeActionPopupDisplayCache, CodeActionPopupRowDisplay, MAX_CODE_ACTION_POPUP_ITEMS,
        code_action_limit_label, code_actions_target_label, take_cached_code_action_popup_item,
    };
    use crate::{
        lsp_code_actions::code_action_display_label, path_display::DISPLAY_PATH_LABEL_MAX_CHARS,
    };
    use kuroya_core::{LspCodeAction, LspTextEdit};
    use serde_json::json;
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };

    fn action(title: &str, kind: Option<&str>) -> LspCodeAction {
        LspCodeAction {
            title: title.to_owned(),
            kind: kind.map(str::to_owned),
            edits: Vec::new(),
            document_changes: Vec::new(),
            resolve_payload: None,
        }
    }

    fn action_with_payload(
        title: &str,
        kind: Option<&str>,
        payload: Arc<serde_json::Value>,
    ) -> LspCodeAction {
        LspCodeAction {
            title: title.to_owned(),
            kind: kind.map(str::to_owned),
            edits: Vec::new(),
            document_changes: Vec::new(),
            resolve_payload: Some(payload),
        }
    }

    fn edit() -> LspTextEdit {
        edit_with_text("use std::collections::HashMap;\n")
    }

    fn edit_with_text(new_text: &str) -> LspTextEdit {
        LspTextEdit {
            path: PathBuf::from("workspace/src/main.rs"),
            start_line: 1,
            start_column: 1,
            end_line: 1,
            end_column: 1,
            new_text: new_text.to_owned(),
        }
    }

    #[test]
    fn code_actions_target_label_sanitizes_display_path_and_preserves_position() {
        let path = Path::new("workspace/src")
            .join(format!("bad\nname\u{202e}{}.rs", "very-long-".repeat(24)));

        let label = code_actions_target_label(Some(&path), 12, 34);

        assert!(label.ends_with(":12:34"));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(label.contains("..."));
        assert!(label.chars().count() <= DISPLAY_PATH_LABEL_MAX_CHARS + ":12:34".chars().count());
    }

    #[test]
    fn code_actions_target_label_keeps_no_target_fallback() {
        assert_eq!(code_actions_target_label(None, 12, 34), "No target");
    }

    #[test]
    fn code_action_limit_label_preserves_capped_list_text() {
        assert_eq!(
            code_action_limit_label(250, 317),
            "Showing first 250 of 317 code actions"
        );
    }

    #[test]
    fn code_action_popup_row_display_sanitizes_label_without_mutating_action() {
        let action = action(
            &format!("Fix\n{}\u{202e}tail", "unsafe-title-".repeat(32)),
            Some(&format!(
                "quickfix\t{}\u{2066}kind",
                "unsafe-kind-".repeat(16)
            )),
        );
        let raw_title = action.title.clone();
        let raw_kind = action.kind.clone();
        let mut cache = CodeActionPopupDisplayCache::default();

        let items = cache.items_for(std::slice::from_ref(&action));
        let row = CodeActionPopupRowDisplay::new(3, true, items[0].label.as_str());

        assert_eq!(row.index, 3);
        assert!(row.selected);
        assert_eq!(row.label, items[0].label.as_str());
        assert_eq!(items[0].label, code_action_display_label(&action));
        assert!(!row.label.contains('\n'), "{:?}", row.label);
        assert!(!row.label.contains('\t'), "{:?}", row.label);
        assert!(!row.label.contains('\u{202e}'), "{:?}", row.label);
        assert!(!row.label.contains('\u{2066}'), "{:?}", row.label);
        assert!(row.label.contains("..."), "{:?}", row.label);

        assert_eq!(action.title, raw_title);
        assert_eq!(raw_kind, action.kind);
        assert!(action.title.contains('\n'));
        assert!(action.title.contains('\u{202e}'));
        assert!(
            action
                .kind
                .as_deref()
                .is_some_and(|kind| { kind.contains('\t') && kind.contains('\u{2066}') })
        );
    }

    #[test]
    fn code_action_popup_display_cache_reuses_prepared_items_for_same_actions() {
        let actions = vec![
            action("Add missing import", Some("quickfix")),
            action("Extract function", Some("refactor.extract")),
        ];
        let mut cache = CodeActionPopupDisplayCache::default();

        let first = cache.items_for(&actions);
        let second = cache.items_for(&actions);

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.len(), actions.len());
        assert_eq!(first[0].target.index, 0);
        assert_eq!(first[1].target.index, 1);
        assert_eq!(first[0].label, code_action_display_label(&actions[0]));
        assert_eq!(first[1].label, code_action_display_label(&actions[1]));
    }

    #[test]
    fn code_action_popup_display_cache_invalidates_when_display_source_changes() {
        let mut actions = vec![action("Fix import", Some("quickfix"))];
        let mut cache = CodeActionPopupDisplayCache::default();
        let first = cache.items_for(&actions);

        actions[0].title = "Extract constant".to_owned();
        actions[0].kind = Some("refactor.extract".to_owned());
        let second = cache.items_for(&actions);

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(second[0].label, code_action_display_label(&actions[0]));
        assert_ne!(first[0].label, second[0].label);
    }

    #[test]
    fn code_action_popup_display_cache_invalidates_when_action_shape_changes() {
        let mut actions = vec![action("Fix import", Some("quickfix"))];
        let mut cache = CodeActionPopupDisplayCache::default();
        let first = cache.items_for(&actions);

        actions[0].edits.push(edit());
        let second = cache.items_for(&actions);

        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(first[0].label, second[0].label);
        assert_eq!(second[0].target.index, 0);
    }

    #[test]
    fn code_action_popup_display_cache_caps_prepared_items_for_large_action_lists() {
        let actions = (0..MAX_CODE_ACTION_POPUP_ITEMS + 17)
            .map(|index| action(&format!("Fix import {index}"), Some("quickfix")))
            .collect::<Vec<_>>();
        let mut cache = CodeActionPopupDisplayCache::default();

        let items = cache.items_for(&actions);
        let second = cache.items_for(&actions);

        assert_eq!(items.len(), MAX_CODE_ACTION_POPUP_ITEMS);
        assert_eq!(
            items.last().expect("last capped item").target.index,
            MAX_CODE_ACTION_POPUP_ITEMS - 1
        );
        assert!(Arc::ptr_eq(&items, &second));
    }

    #[test]
    fn code_action_popup_cached_item_apply_takes_selected_row_without_target_clone() {
        let mut actions = vec![
            action("Fix import", Some("quickfix")),
            action("Extract function", Some("refactor.extract")),
        ];
        let mut cache = CodeActionPopupDisplayCache::default();

        let items = cache.items_for(&actions);
        let applied = take_cached_code_action_popup_item(&items, 1, &mut actions)
            .expect("selected cached row should still match");

        assert_eq!(applied.title, "Extract function");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Fix import");
    }

    #[test]
    fn code_action_popup_apply_target_takes_matching_raw_action_payload() {
        let payload = Arc::new(json!({
            "title": "Fix import\nunsafe",
            "data": {
                "id": 7,
                "raw": "\u{202e}payload"
            }
        }));
        let action = action_with_payload(
            "Fix import\nunsafe\u{202e}",
            Some("quickfix\tunsafe\u{2066}"),
            Arc::clone(&payload),
        );
        let mut actions = vec![action];
        let raw_title = actions[0].title.clone();
        let raw_kind = actions[0].kind.clone();
        let mut cache = CodeActionPopupDisplayCache::default();

        let items = cache.items_for(&actions);
        let target = items[0].target.clone();
        assert!(!items[0].label.contains('\n'), "{:?}", items[0].label);
        assert!(!items[0].label.contains('\t'), "{:?}", items[0].label);
        assert!(!items[0].label.contains('\u{202e}'), "{:?}", items[0].label);
        let applied = target
            .take_matching_action(&mut actions)
            .expect("matching popup target");

        assert!(actions.is_empty());
        assert_eq!(applied.title, raw_title);
        assert_eq!(applied.kind, raw_kind);
        assert!(
            applied
                .resolve_payload
                .as_ref()
                .is_some_and(|applied_payload| Arc::ptr_eq(applied_payload, &payload))
        );
    }

    #[test]
    fn code_action_popup_apply_target_rejects_changed_action_shape() {
        let mut actions = vec![action("Fix import", Some("quickfix"))];
        let mut cache = CodeActionPopupDisplayCache::default();
        let target = cache.items_for(&actions)[0].target.clone();

        actions[0].edits.push(edit());

        assert!(target.take_matching_action(&mut actions).is_none());
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Fix import");
        assert_eq!(actions[0].edits.len(), 1);
    }

    #[test]
    fn code_action_popup_apply_target_rejects_changed_edit_payload_with_same_shape() {
        let mut actions = vec![action("Fix import", Some("quickfix"))];
        actions[0]
            .edits
            .push(edit_with_text("use std::collections::HashMap;\n"));
        let mut cache = CodeActionPopupDisplayCache::default();
        let target = cache.items_for(&actions)[0].target.clone();

        actions[0].edits[0].new_text = "use std::collections::BTreeMap;\n".to_owned();

        assert!(target.take_matching_action(&mut actions).is_none());
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].edits.len(), 1);
        assert_eq!(
            actions[0].edits[0].new_text,
            "use std::collections::BTreeMap;\n"
        );
    }

    #[test]
    fn code_action_popup_apply_target_rejects_changed_resolve_payload() {
        let first_payload = Arc::new(json!({ "data": { "id": 1 } }));
        let second_payload = Arc::new(json!({ "data": { "id": 2 } }));
        let mut actions = vec![action_with_payload(
            "Fix import",
            Some("quickfix"),
            Arc::clone(&first_payload),
        )];
        let mut cache = CodeActionPopupDisplayCache::default();
        let target = cache.items_for(&actions)[0].target.clone();

        actions[0].resolve_payload = Some(second_payload);

        assert!(target.take_matching_action(&mut actions).is_none());
        assert_eq!(actions.len(), 1);
        assert!(
            actions[0]
                .resolve_payload
                .as_ref()
                .is_some_and(|payload| !Arc::ptr_eq(payload, &first_payload))
        );
    }
}
