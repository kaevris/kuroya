use crate::{
    KuroyaApp,
    path_display::{sanitized_display_label_cow, sanitized_owned_display_label},
    source_control_git_panel_ui::{
        SOURCE_CONTROL_GIT_ROW_HEIGHT, SOURCE_CONTROL_GIT_STASH_PANEL_DEFAULT_SIZE,
        apply_git_panel_spacing, git_panel_list_max_height, render_git_panel_row,
    },
    ui_state::{
        clamp_selection, handle_list_navigation_keys, selected_row_scroll_offset,
        selection_page_step,
    },
};
use eframe::egui::{self, Context, InputState, Key, RichText, ScrollArea, TextEdit};
use kuroya_core::{Command, GitStashEntry, text_match::ascii_case_insensitive_contains};
use std::{borrow::Cow, ops::Range};

const SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_STASH_PANEL_FRAGMENT_ELLIPSIS: &str = "...";

impl KuroyaApp {
    pub(crate) fn render_git_stashes_panel(&mut self, ctx: &Context) {
        let mut close = false;
        let mut pending_stash_action: Option<(
            SourceControlStashActionTarget,
            SourceControlStashFooterActionKind,
        )> = None;
        let mut pending_selected_stash_action: Option<SourceControlStashFooterActionKind> = None;

        egui::Window::new("Git Stashes")
            .id(egui::Id::new((
                "git_stashes_panel",
                self.git_panel_open_generation,
            )))
            .max_size(crate::layout::popup_window_max_size_with_top_margin(
                ctx, 120.0,
            ))
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_TOP, [0.0, 96.0])
            .default_size(SOURCE_CONTROL_GIT_STASH_PANEL_DEFAULT_SIZE)
            .show(ctx, |ui| {
                apply_git_panel_spacing(ui);
                ui.horizontal(|ui| {
                    ui.add(
                        TextEdit::singleline(&mut self.source_control_stash_message)
                            .hint_text("Stash message")
                            .desired_width(420.0),
                    );
                    if ui.button("Save").clicked() {
                        self.command_bus.push(Command::SaveGitStash);
                    }
                    if ui.button("Refresh").clicked() {
                        self.begin_git_stashes_panel();
                    }
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });

                let filter_response = ui.add(
                    TextEdit::singleline(&mut self.source_control_stash_query)
                        .hint_text("Filter stashes")
                        .desired_width(f32::INFINITY),
                );
                let stash_query_changed = filter_response.changed();
                self.source_control_stash_selected =
                    source_control_stash_selected_after_query_change(
                        stash_query_changed,
                        self.source_control_stash_selected,
                    );
                let filtered_stash_rows = source_control_filtered_stash_rows(
                    &self.source_control_stashes,
                    &self.source_control_stash_query,
                );
                let stash_row_count = filtered_stash_rows.len();

                if ui.input(|input| input.key_pressed(Key::Escape)) {
                    close = true;
                }
                clamp_selection(&mut self.source_control_stash_selected, stash_row_count);
                let viewport_height = ui.available_height();
                let selection_changed = ui.input(|input| {
                    handle_list_navigation_keys(
                        input,
                        &mut self.source_control_stash_selected,
                        stash_row_count,
                        selection_page_step(SOURCE_CONTROL_GIT_ROW_HEIGHT, viewport_height),
                    )
                }) || stash_query_changed;
                if ui.input(|input| input.key_pressed(Key::Enter)) {
                    pending_selected_stash_action =
                        Some(SourceControlStashFooterActionKind::OpenChanges);
                }
                if let Some(action) = ui.input(source_control_stash_keyboard_action) {
                    pending_selected_stash_action =
                        Some(source_control_stash_keyboard_footer_action(action));
                }

                ui.separator();
                let mut visible_selected_row: Option<usize> = None;
                if self.source_control_stashes.is_empty() {
                    ui.label(RichText::new("No git stashes").small());
                } else if filtered_stash_rows.is_empty() {
                    ui.label(RichText::new("No matching stashes").small());
                } else {
                    let mut scroll_area = ScrollArea::vertical()
                        .auto_shrink([false, true])
                        .max_height(git_panel_list_max_height(ctx, 96.0));
                    if selection_changed {
                        scroll_area =
                            scroll_area.vertical_scroll_offset(selected_row_scroll_offset(
                                self.source_control_stash_selected,
                                stash_row_count,
                                SOURCE_CONTROL_GIT_ROW_HEIGHT,
                                viewport_height,
                            ));
                    }
                    scroll_area.show_rows(
                        ui,
                        SOURCE_CONTROL_GIT_ROW_HEIGHT,
                        stash_row_count,
                        |ui, rows| {
                            let visible_rows =
                                source_control_stash_visible_rows(&filtered_stash_rows, rows);
                            visible_selected_row =
                                visible_rows.selected_row(self.source_control_stash_selected);
                            for row_display in visible_rows.row_displays() {
                                let row = row_display.row();
                                let selected = row == self.source_control_stash_selected;
                                let response = render_git_panel_row(
                                    ui,
                                    selected,
                                    row_display.stash_ref(),
                                    row_display.message(),
                                    row_display.short_oid(),
                                )
                                .on_hover_text(row_display.label());
                                if response.clicked() {
                                    self.source_control_stash_selected = row;
                                    visible_selected_row = Some(row);
                                }
                                if response.double_clicked() {
                                    pending_stash_action = Some((
                                        row_display.target(),
                                        SourceControlStashFooterActionKind::OpenChanges,
                                    ));
                                }
                                response.context_menu(|ui| {
                                    for action in source_control_stash_menu_actions() {
                                        if ui.button(action.label).clicked() {
                                            pending_stash_action =
                                                Some((row_display.target(), action.kind));
                                            ui.close();
                                        }
                                    }
                                });
                            }
                        },
                    );
                }

                if let Some(action) = pending_selected_stash_action
                    && pending_stash_action.is_none()
                {
                    if let Some(target) = visible_selected_row.and_then(|row| {
                        source_control_stash_action_target_for_visible_row(
                            &filtered_stash_rows,
                            row,
                        )
                    }) {
                        pending_stash_action = Some((target, action));
                    } else if !self.source_control_stashes.is_empty() {
                        self.status = source_control_hidden_stash_selection_status();
                    }
                }

                ui.horizontal(|ui| {
                    let selected_row = visible_selected_row;
                    let selected_available = selected_row.is_some();
                    for action in source_control_stash_footer_actions() {
                        if ui
                            .add_enabled(selected_available, egui::Button::new(action.label))
                            .on_hover_text(action.tooltip)
                            .clicked()
                            && let Some(row) = selected_row
                            && let Some(target) =
                                source_control_stash_action_target_for_visible_row(
                                    &filtered_stash_rows,
                                    row,
                                )
                        {
                            pending_stash_action = Some((target, action.kind));
                        }
                    }
                    ui.label(
                        RichText::new(source_control_stash_count_label(
                            stash_row_count,
                            self.source_control_stashes.len(),
                        ))
                        .small(),
                    );
                });
            });

        if close {
            self.source_control_stashes_open = false;
            self.status = "Closed git stashes".to_owned();
        }
        if let Some(resolved_action) = pending_stash_action.and_then(|(target, action)| {
            source_control_resolve_stash_action(ctx, &self.source_control_stashes, target, action)
        }) {
            match resolved_action {
                SourceControlResolvedStashAction::OpenChanges(stash) => {
                    self.open_stash_changes(stash);
                }
                SourceControlResolvedStashAction::CopyPatch(stash) => {
                    self.copy_stash_patch(ctx, &stash);
                }
                SourceControlResolvedStashAction::Apply(index) => {
                    self.apply_git_stash(index);
                }
                SourceControlResolvedStashAction::Pop(index) => {
                    self.pop_git_stash(index);
                }
                SourceControlResolvedStashAction::Drop(index) => {
                    self.drop_git_stash(index);
                }
                SourceControlResolvedStashAction::Status(status) => {
                    self.status = status;
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn source_control_stash_label(stash: &GitStashEntry) -> String {
    source_control_stash_row_label(stash)
}

#[derive(Debug)]
struct SourceControlStashRowDisplay<'a> {
    row: usize,
    stash_row: usize,
    stash: &'a GitStashEntry,
    stash_ref: String,
    short_oid: Cow<'a, str>,
    message: Cow<'a, str>,
    label: String,
}

impl<'a> SourceControlStashRowDisplay<'a> {
    fn new(row: usize, stash_row: usize, stash: &'a GitStashEntry) -> Self {
        let stash_ref = source_control_stash_ref(stash);
        let short_oid = source_control_stash_panel_display_label(&stash.short_oid, "unknown");
        let message = source_control_stash_panel_display_label(&stash.message, "No message");
        let label = source_control_stash_row_label_from_display(
            &stash_ref,
            short_oid.as_ref(),
            message.as_ref(),
        );
        Self {
            row,
            stash_row,
            stash,
            stash_ref,
            short_oid,
            message,
            label,
        }
    }

    fn row(&self) -> usize {
        self.row
    }

    fn target(&self) -> SourceControlStashActionTarget {
        SourceControlStashActionTarget::new(self.stash_row, self.stash)
    }

    fn label(&self) -> &str {
        &self.label
    }

    fn stash_ref(&self) -> &str {
        &self.stash_ref
    }

    fn short_oid(&self) -> &str {
        self.short_oid.as_ref()
    }

    fn message(&self) -> &str {
        self.message.as_ref()
    }
}

#[derive(Debug, Clone, Copy)]
struct SourceControlStashVisibleRows<'a> {
    first_row: usize,
    rows: &'a [(usize, &'a GitStashEntry)],
}

impl<'a> SourceControlStashVisibleRows<'a> {
    fn selected_row(&self, selected: usize) -> Option<usize> {
        let visible_end = self.first_row.saturating_add(self.rows.len());
        (selected >= self.first_row && selected < visible_end).then_some(selected)
    }

    fn row_displays(self) -> impl Iterator<Item = SourceControlStashRowDisplay<'a>> + 'a {
        self.rows
            .iter()
            .enumerate()
            .map(move |(offset, (stash_row, stash))| {
                SourceControlStashRowDisplay::new(self.first_row + offset, *stash_row, stash)
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SourceControlStashActionTarget {
    row: usize,
    stash_index: usize,
    short_oid: String,
    message: String,
}

impl SourceControlStashActionTarget {
    fn new(row: usize, stash: &GitStashEntry) -> Self {
        Self {
            row,
            stash_index: stash.index,
            short_oid: stash.short_oid.clone(),
            message: stash.message.clone(),
        }
    }
}

fn source_control_stash_action_target_for_visible_row(
    filtered_rows: &[(usize, &GitStashEntry)],
    row: usize,
) -> Option<SourceControlStashActionTarget> {
    filtered_rows
        .get(row)
        .map(|(stash_row, stash)| SourceControlStashActionTarget::new(*stash_row, stash))
}

#[cfg(test)]
fn source_control_stash_action_target_at(
    stashes: &[GitStashEntry],
    row: usize,
) -> Option<(SourceControlStashActionTarget, &GitStashEntry)> {
    stashes
        .get(row)
        .map(|stash| (SourceControlStashActionTarget::new(row, stash), stash))
}

#[cfg(test)]
fn source_control_stash_for_action_target<'a>(
    stashes: &'a [GitStashEntry],
    target: &SourceControlStashActionTarget,
) -> Option<&'a GitStashEntry> {
    if source_control_stash_action_target_identity_is_ambiguous(stashes, target) {
        return None;
    }
    source_control_stash_for_unambiguous_action_target(stashes, target)
}

fn source_control_stash_for_unambiguous_action_target<'a>(
    stashes: &'a [GitStashEntry],
    target: &SourceControlStashActionTarget,
) -> Option<&'a GitStashEntry> {
    if let Some(stash) = stashes
        .get(target.row)
        .filter(|stash| source_control_stash_matches_exact_action_target(stash, target))
    {
        return Some(stash);
    }
    stashes
        .iter()
        .find(|stash| source_control_stash_matches_action_target_identity(stash, target))
}

fn source_control_stash_matches_exact_action_target(
    stash: &GitStashEntry,
    target: &SourceControlStashActionTarget,
) -> bool {
    stash.index == target.stash_index
        && source_control_stash_matches_action_target_identity(stash, target)
}

fn source_control_stash_matches_action_target_identity(
    stash: &GitStashEntry,
    target: &SourceControlStashActionTarget,
) -> bool {
    stash.short_oid == target.short_oid && stash.message == target.message
}

fn source_control_stash_action_target_identity_is_ambiguous(
    stashes: &[GitStashEntry],
    target: &SourceControlStashActionTarget,
) -> bool {
    let mut matches = stashes
        .iter()
        .filter(|stash| source_control_stash_matches_action_target_identity(stash, target));
    matches.next().is_some() && matches.next().is_some()
}

enum SourceControlResolvedStashAction {
    OpenChanges(GitStashEntry),
    CopyPatch(GitStashEntry),
    Apply(usize),
    Pop(usize),
    Drop(usize),
    Status(String),
}

fn source_control_resolve_stash_action(
    ctx: &Context,
    stashes: &[GitStashEntry],
    target: SourceControlStashActionTarget,
    action: SourceControlStashFooterActionKind,
) -> Option<SourceControlResolvedStashAction> {
    if source_control_stash_action_target_identity_is_ambiguous(stashes, &target) {
        return Some(SourceControlResolvedStashAction::Status(
            source_control_ambiguous_stash_identity_action_status(target.stash_index),
        ));
    }
    let Some(stash) = source_control_stash_for_unambiguous_action_target(stashes, &target) else {
        return Some(SourceControlResolvedStashAction::Status(
            source_control_stale_stash_action_status(target.stash_index),
        ));
    };
    if !source_control_stash_index_is_unique(stashes, stash.index) {
        return Some(SourceControlResolvedStashAction::Status(
            source_control_ambiguous_stash_action_status(stash.index),
        ));
    }
    Some(match action {
        SourceControlStashFooterActionKind::OpenChanges => {
            SourceControlResolvedStashAction::OpenChanges(stash.clone())
        }
        SourceControlStashFooterActionKind::CopyPatch => {
            SourceControlResolvedStashAction::CopyPatch(stash.clone())
        }
        SourceControlStashFooterActionKind::Apply => {
            SourceControlResolvedStashAction::Apply(stash.index)
        }
        SourceControlStashFooterActionKind::Pop => {
            SourceControlResolvedStashAction::Pop(stash.index)
        }
        SourceControlStashFooterActionKind::Drop => {
            SourceControlResolvedStashAction::Drop(stash.index)
        }
        SourceControlStashFooterActionKind::CopyRef => SourceControlResolvedStashAction::Status(
            copy_stash_to_clipboard(ctx, stash, SourceControlStashCopyKind::Ref),
        ),
        SourceControlStashFooterActionKind::CopyMessage => {
            SourceControlResolvedStashAction::Status(copy_stash_to_clipboard(
                ctx,
                stash,
                SourceControlStashCopyKind::Message,
            ))
        }
    })
}

fn source_control_stale_stash_action_status(index: usize) -> String {
    use std::fmt::Write as _;

    let mut status = String::with_capacity(
        "Git stash list changed; refresh before acting on stash ".len()
            + source_control_usize_decimal_len(index),
    );
    status.push_str("Git stash list changed; refresh before acting on stash ");
    let _ = write!(status, "{index}");
    status
}

fn source_control_ambiguous_stash_action_status(index: usize) -> String {
    let mut status = String::with_capacity(
        "Git stash list has duplicate ; refresh before acting".len()
            + source_control_stash_ref_len(index),
    );
    status.push_str("Git stash list has duplicate ");
    push_source_control_stash_ref(&mut status, index);
    status.push_str("; refresh before acting");
    status
}

fn source_control_ambiguous_stash_identity_action_status(index: usize) -> String {
    let mut status = String::with_capacity(
        "Git stash list has duplicate identity for ; refresh before acting".len()
            + source_control_stash_ref_len(index),
    );
    status.push_str("Git stash list has duplicate identity for ");
    push_source_control_stash_ref(&mut status, index);
    status.push_str("; refresh before acting");
    status
}

fn source_control_stash_index_is_unique(stashes: &[GitStashEntry], index: usize) -> bool {
    let mut found = false;
    for stash in stashes {
        if stash.index != index {
            continue;
        }
        if found {
            return false;
        }
        found = true;
    }
    found
}

fn source_control_hidden_stash_selection_status() -> String {
    "Selected git stash is not visible; scroll to it before acting".to_owned()
}

#[cfg(test)]
fn source_control_stash_row_label(stash: &GitStashEntry) -> String {
    let stash_ref = source_control_stash_ref(stash);
    let oid = source_control_stash_panel_display_label(&stash.short_oid, "unknown");
    let message = source_control_stash_panel_display_label(&stash.message, "No message");
    source_control_stash_row_label_from_display(&stash_ref, oid.as_ref(), message.as_ref())
}

fn source_control_stash_row_label_from_display(
    stash_ref: &str,
    oid: &str,
    message: &str,
) -> String {
    let mut label = String::with_capacity(stash_ref.len() + 4 + oid.len() + message.len());
    label.push_str(stash_ref);
    label.push_str("  ");
    label.push_str(oid);
    label.push_str("  ");
    label.push_str(message);
    label
}

fn source_control_stash_panel_display_label<'a>(value: &'a str, fallback: &str) -> Cow<'a, str> {
    match source_control_stash_display_input(value) {
        Cow::Borrowed(input) => sanitized_display_label_cow(
            input,
            SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS,
            fallback,
        ),
        Cow::Owned(input) => Cow::Owned(sanitized_owned_display_label(
            input,
            SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS,
            fallback,
        )),
    }
}

fn source_control_stash_display_input(value: &str) -> Cow<'_, str> {
    if value.len()
        <= source_control_stash_display_input_byte_limit(
            SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS,
        )
        && value
            .chars()
            .nth(SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS)
            .is_none()
    {
        return Cow::Borrowed(value);
    }

    Cow::Owned(source_control_stash_sample_display_input(
        value,
        SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS,
    ))
}

fn source_control_stash_display_input_byte_limit(max_chars: usize) -> usize {
    max_chars.saturating_mul(8).max(512)
}

fn source_control_stash_sample_display_input(value: &str, max_chars: usize) -> String {
    if max_chars <= SOURCE_CONTROL_STASH_PANEL_FRAGMENT_ELLIPSIS.len() {
        return ".".repeat(max_chars);
    }

    let keep = max_chars - SOURCE_CONTROL_STASH_PANEL_FRAGMENT_ELLIPSIS.len();
    let head = keep / 2;
    let tail = keep - head;
    let head_end = source_control_stash_sample_head_end(value, head);
    let tail_start = source_control_stash_sample_tail_start(value, tail).max(head_end);
    let mut output = String::with_capacity(head_end + 3 + value.len().saturating_sub(tail_start));
    output.push_str(&value[..head_end]);
    output.push_str(SOURCE_CONTROL_STASH_PANEL_FRAGMENT_ELLIPSIS);
    output.push_str(&value[tail_start..]);
    output
}

fn source_control_stash_sample_head_end(value: &str, chars: usize) -> usize {
    if chars == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(chars)
        .map_or(value.len(), |(index, _)| index)
}

fn source_control_stash_sample_tail_start(value: &str, chars: usize) -> usize {
    if chars == 0 {
        return value.len();
    }
    value
        .char_indices()
        .rev()
        .nth(chars - 1)
        .map_or(0, |(index, _)| index)
}

fn source_control_stash_visible_rows<'a>(
    filtered_rows: &'a [(usize, &'a GitStashEntry)],
    rows: Range<usize>,
) -> SourceControlStashVisibleRows<'a> {
    let start = rows.start.min(filtered_rows.len());
    let end = rows.end.min(filtered_rows.len()).max(start);
    SourceControlStashVisibleRows {
        first_row: start,
        rows: &filtered_rows[start..end],
    }
}

fn source_control_filtered_stash_rows<'a>(
    stashes: &'a [GitStashEntry],
    query: &str,
) -> Vec<(usize, &'a GitStashEntry)> {
    let terms = source_control_stash_filter_terms(query);
    stashes
        .iter()
        .enumerate()
        .filter(|(_, stash)| source_control_stash_matches_filter_terms(stash, &terms))
        .collect()
}

fn source_control_stash_filter_terms(query: &str) -> Vec<&str> {
    query.split_whitespace().collect()
}

fn source_control_stash_matches_filter_terms(stash: &GitStashEntry, terms: &[&str]) -> bool {
    terms.iter().all(|term| {
        ascii_case_insensitive_contains(&stash.short_oid, term)
            || ascii_case_insensitive_contains(&stash.message, term)
    })
}

fn source_control_stash_selected_after_query_change(
    query_changed: bool,
    previous_selected: usize,
) -> usize {
    if query_changed { 0 } else { previous_selected }
}

fn source_control_stash_count_label(filtered_count: usize, total_count: usize) -> String {
    if filtered_count == total_count {
        format!("{total_count} stashes")
    } else {
        format!("{filtered_count} of {total_count} stashes")
    }
}

#[cfg(test)]
fn source_control_stash_unfiltered_rows(stashes: &[GitStashEntry]) -> Vec<(usize, &GitStashEntry)> {
    source_control_filtered_stash_rows(stashes, "")
}

#[cfg(test)]
fn source_control_stash_visible_row_displays<'a>(
    filtered_rows: &'a [(usize, &'a GitStashEntry)],
    rows: Range<usize>,
) -> impl Iterator<Item = SourceControlStashRowDisplay<'a>> + 'a {
    source_control_stash_visible_rows(filtered_rows, rows).row_displays()
}

#[cfg(test)]
fn source_control_visible_selected_stash_row(
    filtered_rows: &[(usize, &GitStashEntry)],
    selected: usize,
    rows: Range<usize>,
) -> Option<usize> {
    source_control_stash_visible_rows(filtered_rows, rows).selected_row(selected)
}

#[cfg(test)]
fn source_control_visible_selected_stash_action_target_at<'a>(
    filtered_rows: &'a [(usize, &'a GitStashEntry)],
    selected: usize,
    rows: Range<usize>,
) -> Option<(SourceControlStashActionTarget, &'a GitStashEntry)> {
    let selected = source_control_visible_selected_stash_row(filtered_rows, selected, rows)?;
    let (stash_row, stash) = filtered_rows.get(selected)?;
    Some((
        SourceControlStashActionTarget::new(*stash_row, stash),
        stash,
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlStashFooterActionKind {
    OpenChanges,
    CopyPatch,
    Apply,
    Pop,
    Drop,
    CopyRef,
    CopyMessage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceControlStashMenuAction {
    kind: SourceControlStashFooterActionKind,
    label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SourceControlStashFooterAction {
    kind: SourceControlStashFooterActionKind,
    label: &'static str,
    tooltip: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceControlStashKeyboardActionKind {
    Patch,
    Ref,
    Message,
    Apply,
    Pop,
    Drop,
}

fn source_control_stash_keyboard_action(
    input: &InputState,
) -> Option<SourceControlStashKeyboardActionKind> {
    let mut pressed = source_control_stash_keyboard_action_kinds()
        .iter()
        .copied()
        .filter(|action| source_control_stash_keyboard_action_pressed(input, *action));
    let action = pressed.next()?;
    pressed.next().is_none().then_some(action)
}

fn source_control_stash_keyboard_action_pressed(
    input: &InputState,
    action: SourceControlStashKeyboardActionKind,
) -> bool {
    let only_alt = input.modifiers.alt
        && !input.modifiers.ctrl
        && !input.modifiers.command
        && !input.modifiers.mac_cmd
        && !input.modifiers.shift;
    only_alt
        && input.key_pressed(match action {
            SourceControlStashKeyboardActionKind::Patch => Key::P,
            SourceControlStashKeyboardActionKind::Ref => Key::R,
            SourceControlStashKeyboardActionKind::Message => Key::M,
            SourceControlStashKeyboardActionKind::Apply => Key::A,
            SourceControlStashKeyboardActionKind::Pop => Key::O,
            SourceControlStashKeyboardActionKind::Drop => Key::D,
        })
}

fn source_control_stash_keyboard_action_kinds() -> &'static [SourceControlStashKeyboardActionKind] {
    &[
        SourceControlStashKeyboardActionKind::Patch,
        SourceControlStashKeyboardActionKind::Ref,
        SourceControlStashKeyboardActionKind::Message,
        SourceControlStashKeyboardActionKind::Apply,
        SourceControlStashKeyboardActionKind::Pop,
        SourceControlStashKeyboardActionKind::Drop,
    ]
}

fn source_control_stash_keyboard_footer_action(
    action: SourceControlStashKeyboardActionKind,
) -> SourceControlStashFooterActionKind {
    match action {
        SourceControlStashKeyboardActionKind::Patch => {
            SourceControlStashFooterActionKind::CopyPatch
        }
        SourceControlStashKeyboardActionKind::Ref => SourceControlStashFooterActionKind::CopyRef,
        SourceControlStashKeyboardActionKind::Message => {
            SourceControlStashFooterActionKind::CopyMessage
        }
        SourceControlStashKeyboardActionKind::Apply => SourceControlStashFooterActionKind::Apply,
        SourceControlStashKeyboardActionKind::Pop => SourceControlStashFooterActionKind::Pop,
        SourceControlStashKeyboardActionKind::Drop => SourceControlStashFooterActionKind::Drop,
    }
}

#[cfg(test)]
pub(crate) fn source_control_stash_keyboard_action_labels() -> Vec<&'static str> {
    let actions = source_control_stash_keyboard_action_kinds();
    let mut labels = Vec::with_capacity(actions.len());
    labels.extend(
        actions
            .iter()
            .copied()
            .map(source_control_stash_keyboard_action_label),
    );
    labels
}

#[cfg(test)]
fn source_control_stash_keyboard_action_label(
    action: SourceControlStashKeyboardActionKind,
) -> &'static str {
    match action {
        SourceControlStashKeyboardActionKind::Patch => "Alt+P Copy Patch",
        SourceControlStashKeyboardActionKind::Ref => "Alt+R Copy Stash Ref",
        SourceControlStashKeyboardActionKind::Message => "Alt+M Copy Stash Message",
        SourceControlStashKeyboardActionKind::Apply => "Alt+A Apply Stash",
        SourceControlStashKeyboardActionKind::Pop => "Alt+O Pop Stash",
        SourceControlStashKeyboardActionKind::Drop => "Alt+D Drop Stash",
    }
}

fn source_control_stash_menu_actions() -> &'static [SourceControlStashMenuAction] {
    &[
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::OpenChanges,
            label: "Open Changes",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::CopyPatch,
            label: "Copy Patch",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::CopyRef,
            label: "Copy Stash Ref",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::CopyMessage,
            label: "Copy Stash Message",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::Apply,
            label: "Apply Stash",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::Pop,
            label: "Pop Stash",
        },
        SourceControlStashMenuAction {
            kind: SourceControlStashFooterActionKind::Drop,
            label: "Drop Stash",
        },
    ]
}

fn source_control_stash_footer_actions() -> &'static [SourceControlStashFooterAction] {
    &[
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::OpenChanges,
            label: "Open Changes",
            tooltip: "Open Stash Changes",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::CopyPatch,
            label: "Copy Patch",
            tooltip: "Copy Stash Patch (Alt+P)",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::Apply,
            label: "Apply",
            tooltip: "Apply Stash (Alt+A)",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::Pop,
            label: "Pop",
            tooltip: "Pop Stash (Alt+O)",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::Drop,
            label: "Drop",
            tooltip: "Drop Stash (Alt+D)",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::CopyRef,
            label: "Copy Ref",
            tooltip: "Copy Stash Ref (Alt+R)",
        },
        SourceControlStashFooterAction {
            kind: SourceControlStashFooterActionKind::CopyMessage,
            label: "Copy Message",
            tooltip: "Copy Stash Message (Alt+M)",
        },
    ]
}

#[cfg(test)]
pub(crate) fn source_control_stash_footer_action_labels() -> Vec<&'static str> {
    let actions = source_control_stash_footer_actions();
    let mut labels = Vec::with_capacity(actions.len());
    labels.extend(actions.iter().map(|action| action.label));
    labels
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceControlStashCopyKind {
    Ref,
    Message,
}

fn copy_stash_to_clipboard(
    ctx: &Context,
    stash: &GitStashEntry,
    kind: SourceControlStashCopyKind,
) -> String {
    let text = source_control_stash_copy_text(stash, kind);
    ctx.copy_text(text);
    source_control_stash_copy_status(stash, kind)
}

pub(crate) fn source_control_stash_ref(stash: &GitStashEntry) -> String {
    let mut stash_ref = String::with_capacity(source_control_stash_ref_len(stash.index));
    push_source_control_stash_ref(&mut stash_ref, stash.index);
    stash_ref
}

fn push_source_control_stash_ref(output: &mut String, index: usize) {
    use std::fmt::Write as _;

    output.push_str("stash@{");
    let _ = write!(output, "{index}");
    output.push('}');
}

fn source_control_stash_ref_len(index: usize) -> usize {
    "stash@{}".len() + source_control_usize_decimal_len(index)
}

fn source_control_usize_decimal_len(mut value: usize) -> usize {
    let mut len = 1;
    while value >= 10 {
        value /= 10;
        len += 1;
    }
    len
}

pub(crate) fn source_control_stash_copy_text(
    stash: &GitStashEntry,
    kind: SourceControlStashCopyKind,
) -> String {
    match kind {
        SourceControlStashCopyKind::Ref => source_control_stash_ref(stash),
        SourceControlStashCopyKind::Message => stash.message.clone(),
    }
}

pub(crate) fn source_control_stash_copy_status(
    stash: &GitStashEntry,
    kind: SourceControlStashCopyKind,
) -> String {
    match kind {
        SourceControlStashCopyKind::Ref => {
            let mut status = String::with_capacity(
                "Copied stash ref ".len() + source_control_stash_ref_len(stash.index),
            );
            status.push_str("Copied stash ref ");
            push_source_control_stash_ref(&mut status, stash.index);
            status
        }
        SourceControlStashCopyKind::Message => {
            let mut status = String::with_capacity(
                "Copied stash message for ".len() + source_control_stash_ref_len(stash.index),
            );
            status.push_str("Copied stash message for ");
            push_source_control_stash_ref(&mut status, stash.index);
            status
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS, SourceControlResolvedStashAction,
        SourceControlStashActionTarget, SourceControlStashCopyKind,
        SourceControlStashFooterActionKind, SourceControlStashKeyboardActionKind,
        source_control_ambiguous_stash_action_status,
        source_control_ambiguous_stash_identity_action_status, source_control_filtered_stash_rows,
        source_control_resolve_stash_action, source_control_stale_stash_action_status,
        source_control_stash_action_target_at, source_control_stash_copy_status,
        source_control_stash_copy_text, source_control_stash_count_label,
        source_control_stash_display_input, source_control_stash_for_action_target,
        source_control_stash_keyboard_action, source_control_stash_label,
        source_control_stash_menu_actions, source_control_stash_ref, source_control_stash_ref_len,
        source_control_stash_selected_after_query_change, source_control_stash_unfiltered_rows,
        source_control_stash_visible_row_displays, source_control_stash_visible_rows,
        source_control_visible_selected_stash_action_target_at,
        source_control_visible_selected_stash_row,
    };
    use eframe::egui::{Context, Event, Key, Modifiers, RawInput};
    use kuroya_core::GitStashEntry;

    fn stash(index: usize, short_oid: &str, message: &str) -> GitStashEntry {
        GitStashEntry {
            index,
            short_oid: short_oid.to_owned(),
            message: message.to_owned(),
        }
    }

    fn stash_filter_rows(stashes: &[GitStashEntry], query: &str) -> Vec<usize> {
        source_control_filtered_stash_rows(stashes, query)
            .into_iter()
            .map(|(stash_row, _)| stash_row)
            .collect()
    }

    fn stash_keyboard_action_for_keys(
        keys: &[Key],
    ) -> Option<SourceControlStashKeyboardActionKind> {
        let ctx = Context::default();
        let modifiers = Modifiers::ALT;
        let input = RawInput {
            modifiers,
            events: keys
                .iter()
                .copied()
                .map(|key| Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                })
                .collect(),
            ..RawInput::default()
        };
        let mut action = None;

        let _ = ctx.run(input, |ctx| {
            action = ctx.input(source_control_stash_keyboard_action);
        });

        action
    }

    #[test]
    fn stash_ref_capacity_matches_formatted_ref_width() {
        for index in [0, 9, 10, 99, 100, usize::MAX] {
            let stash = stash(index, "abcd1234", "work in progress");
            let stash_ref = source_control_stash_ref(&stash);

            assert_eq!(stash_ref, format!("stash@{{{index}}}"));
            assert_eq!(source_control_stash_ref_len(index), stash_ref.len());
        }
    }

    #[test]
    fn stash_label_preserves_display_text_for_multi_digit_indices() {
        let stash = stash(123, "abcd1234", "work in progress");

        assert_eq!(
            source_control_stash_label(&stash),
            "stash@{123}  abcd1234  work in progress"
        );
    }

    #[test]
    fn stash_label_samples_overlong_fragments_before_display_sanitize() {
        let hostile = format!("{}\u{202e}\n{}", "left".repeat(800), "right".repeat(800));
        let sampled = source_control_stash_display_input(&hostile);

        assert!(matches!(&sampled, std::borrow::Cow::Owned(_)));
        assert!(sampled.starts_with("left"));
        assert!(sampled.ends_with("right"));
        assert!(sampled.chars().count() <= SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS);

        let label = source_control_stash_label(&stash(7, &hostile, &hostile));

        assert!(label.starts_with("stash@{7}  left"));
        assert!(label.contains("..."));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(
            label.chars().count()
                <= source_control_stash_ref_len(7)
                    + 4
                    + SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS * 2
        );
    }

    #[test]
    fn stash_label_samples_huge_ascii_fragments_from_head_and_tail() {
        let huge = format!(
            "{}{}{}",
            "left-".repeat(1024),
            "middle-".repeat(4096),
            "right-".repeat(1024)
        );

        let sampled = source_control_stash_display_input(&huge);
        let label = source_control_stash_label(&stash(7, &huge, &huge));

        assert!(matches!(sampled, std::borrow::Cow::Owned(_)));
        assert!(sampled.len() < huge.len() / 8);
        assert!(sampled.starts_with("left-"));
        assert!(sampled.ends_with("right-"));
        assert!(label.starts_with("stash@{7}  left-"));
        assert!(label.contains("right-"));
        assert!(label.contains("..."));
        assert!(
            label.chars().count()
                <= source_control_stash_ref_len(7)
                    + 4
                    + SOURCE_CONTROL_STASH_PANEL_FRAGMENT_MAX_CHARS * 2
        );
    }

    #[test]
    fn stash_visible_row_display_is_safe_without_changing_raw_copy_fields() {
        let raw_oid = format!("12\u{202e}\n34{}", "a".repeat(200));
        let raw_message = format!("On main:\u{2066}\nwork{}", "b".repeat(200));
        let stashes = vec![
            stash(0, "aaaa0000", "first"),
            stash(1, &raw_oid, &raw_message),
        ];
        let filtered_rows = source_control_stash_unfiltered_rows(&stashes);

        let row_displays =
            source_control_stash_visible_row_displays(&filtered_rows, 1..2).collect::<Vec<_>>();

        assert_eq!(row_displays.len(), 1);
        assert_eq!(row_displays[0].row(), 1);
        assert_eq!(
            row_displays[0].target(),
            SourceControlStashActionTarget::new(1, &stashes[1])
        );
        let label = row_displays[0].label();
        assert!(label.starts_with("stash@{1}  12 34"));
        assert!(label.contains("On main: work"));
        assert!(label.contains("..."));
        assert!(!label.contains('\n'));
        assert!(!label.contains('\u{202e}'));
        assert!(!label.contains('\u{2066}'));
        assert_eq!(stashes[1].short_oid, raw_oid);
        assert_eq!(
            source_control_stash_copy_text(&stashes[1], SourceControlStashCopyKind::Message),
            raw_message
        );
    }

    #[test]
    fn stash_visible_rows_clamps_out_of_range_requests() {
        let stashes = vec![
            stash(0, "aaaa0000", "first"),
            stash(1, "bbbb1111", "second"),
            stash(2, "cccc2222", "third"),
        ];
        let filtered_rows = source_control_stash_unfiltered_rows(&stashes);

        let visible_rows = source_control_stash_visible_rows(&filtered_rows, 1..10);
        assert_eq!(visible_rows.first_row, 1);
        assert_eq!(
            visible_rows
                .row_displays()
                .map(|row_display| row_display.target().stash_index)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );

        let visible_rows = source_control_stash_visible_rows(&filtered_rows, 10..12);
        assert_eq!(visible_rows.first_row, 3);
        assert!(visible_rows.row_displays().next().is_none());

        let visible_rows =
            source_control_stash_visible_rows(&filtered_rows, usize::MAX - 1..usize::MAX);
        assert_eq!(visible_rows.first_row, 3);
        assert!(visible_rows.row_displays().next().is_none());

        let reversed_start = filtered_rows.len() - 1;
        let reversed_end = reversed_start.saturating_sub(1);
        let visible_rows =
            source_control_stash_visible_rows(&filtered_rows, reversed_start..reversed_end);
        assert_eq!(visible_rows.first_row, 2);
        assert!(visible_rows.row_displays().next().is_none());
    }

    #[test]
    fn stash_action_target_resolves_same_raw_identity_after_row_or_index_shift() {
        let stashes = vec![
            stash(10, "aaaa0000", "first"),
            stash(4, "bbbb1111", "second"),
        ];

        let (target, selected) = source_control_stash_action_target_at(&stashes, 1).unwrap();

        assert_eq!(target, SourceControlStashActionTarget::new(1, &stashes[1]));
        assert_eq!(selected.message, "second");
        assert_eq!(
            source_control_stash_for_action_target(&stashes, &target)
                .unwrap()
                .message,
            "second"
        );

        let same_row_different_index = vec![
            stash(10, "aaaa0000", "first"),
            stash(5, "bbbb1111", "second"),
        ];
        assert_eq!(
            source_control_stash_for_action_target(&same_row_different_index, &target)
                .unwrap()
                .index,
            5
        );

        let same_row_and_index_different_oid = vec![
            stash(10, "aaaa0000", "first"),
            stash(4, "cccc2222", "different stash"),
        ];
        assert!(
            source_control_stash_for_action_target(&same_row_and_index_different_oid, &target)
                .is_none()
        );

        let same_row_index_and_oid_different_message = vec![
            stash(10, "aaaa0000", "first"),
            stash(4, "bbbb1111", "different stash"),
        ];
        assert!(
            source_control_stash_for_action_target(
                &same_row_index_and_oid_different_message,
                &target
            )
            .is_none()
        );

        let same_index_different_row = vec![
            stash(4, "bbbb1111", "second"),
            stash(10, "aaaa0000", "first"),
        ];
        assert_eq!(
            source_control_stash_for_action_target(&same_index_different_row, &target)
                .unwrap()
                .message,
            "second"
        );
        assert!(source_control_stash_action_target_at(&stashes, 3).is_none());
    }

    #[test]
    fn visible_selected_stash_action_target_requires_rendered_row() {
        let stashes = vec![
            stash(0, "aaaa0000", "first"),
            stash(1, "bbbb1111", "second"),
            stash(2, "cccc2222", "third"),
        ];
        let filtered_rows = source_control_stash_unfiltered_rows(&stashes);

        assert_eq!(
            source_control_visible_selected_stash_action_target_at(&filtered_rows, 1, 1..2)
                .map(|(target, _)| target),
            Some(SourceControlStashActionTarget::new(1, &stashes[1]))
        );
        assert!(
            source_control_visible_selected_stash_action_target_at(&filtered_rows, 1, 0..1)
                .is_none()
        );
        assert!(
            source_control_visible_selected_stash_action_target_at(&filtered_rows, 1, 2..3)
                .is_none()
        );
        assert!(
            source_control_visible_selected_stash_action_target_at(&filtered_rows, 1, 10..12)
                .is_none()
        );
        let start = 2;
        let end = 1;
        assert!(
            source_control_visible_selected_stash_action_target_at(&filtered_rows, 1, start..end)
                .is_none()
        );
    }

    #[test]
    fn shifted_stash_action_target_uses_current_unique_index() {
        let ctx = Context::default();
        let original = vec![
            stash(0, "aaaa0000", "first"),
            stash(4, "bbbb1111", "second"),
        ];
        let (target, _) = source_control_stash_action_target_at(&original, 1).unwrap();
        let shifted = vec![
            stash(0, "aaaa0000", "first"),
            stash(1, "cccc2222", "new"),
            stash(5, "bbbb1111", "second"),
        ];

        let resolved = source_control_resolve_stash_action(
            &ctx,
            &shifted,
            target,
            SourceControlStashFooterActionKind::Apply,
        );

        match resolved {
            Some(SourceControlResolvedStashAction::Apply(index)) => assert_eq!(index, 5),
            _ => panic!("expected shifted stash apply target"),
        }
    }

    #[test]
    fn stale_stash_action_target_returns_retry_status() {
        let ctx = Context::default();
        let original = vec![
            stash(10, "aaaa0000", "first"),
            stash(4, "bbbb1111", "second"),
        ];
        let (target, _) = source_control_stash_action_target_at(&original, 1).unwrap();
        let changed = vec![
            stash(10, "aaaa0000", "first"),
            stash(4, "cccc2222", "different stash"),
        ];

        let resolved = source_control_resolve_stash_action(
            &ctx,
            &changed,
            target,
            SourceControlStashFooterActionKind::Apply,
        );

        match resolved {
            Some(SourceControlResolvedStashAction::Status(status)) => {
                assert_eq!(status, source_control_stale_stash_action_status(4));
            }
            _ => panic!("expected stale status"),
        }
    }

    #[test]
    fn duplicate_stash_index_action_returns_ambiguous_status() {
        let ctx = Context::default();
        let stashes = vec![
            stash(4, "aaaa0000", "first"),
            stash(4, "bbbb1111", "second"),
        ];
        let (target, _) = source_control_stash_action_target_at(&stashes, 1).unwrap();

        let resolved = source_control_resolve_stash_action(
            &ctx,
            &stashes,
            target,
            SourceControlStashFooterActionKind::Drop,
        );

        match resolved {
            Some(SourceControlResolvedStashAction::Status(status)) => {
                assert_eq!(status, source_control_ambiguous_stash_action_status(4));
            }
            _ => panic!("expected ambiguous status"),
        }
    }

    #[test]
    fn duplicate_stash_identity_action_returns_ambiguous_status() {
        let ctx = Context::default();
        let stashes = vec![
            stash(4, "bbbb1111", "same identity"),
            stash(5, "bbbb1111", "same identity"),
        ];
        let (target, _) = source_control_stash_action_target_at(&stashes, 0).unwrap();

        let resolved = source_control_resolve_stash_action(
            &ctx,
            &stashes,
            target,
            SourceControlStashFooterActionKind::Drop,
        );

        match resolved {
            Some(SourceControlResolvedStashAction::Status(status)) => {
                assert_eq!(
                    status,
                    source_control_ambiguous_stash_identity_action_status(4)
                );
            }
            _ => panic!("expected ambiguous identity status"),
        }
    }

    #[test]
    fn stash_keyboard_action_ignores_ambiguous_shortcut_frames() {
        assert_eq!(
            stash_keyboard_action_for_keys(&[Key::P]),
            Some(SourceControlStashKeyboardActionKind::Patch)
        );
        assert_eq!(stash_keyboard_action_for_keys(&[Key::P, Key::D]), None);
    }

    #[test]
    fn stash_menu_actions_preserve_context_menu_order_and_labels() {
        let actions = source_control_stash_menu_actions();

        assert_eq!(
            actions
                .iter()
                .map(|action| (action.kind, action.label))
                .collect::<Vec<_>>(),
            vec![
                (
                    SourceControlStashFooterActionKind::OpenChanges,
                    "Open Changes"
                ),
                (SourceControlStashFooterActionKind::CopyPatch, "Copy Patch"),
                (
                    SourceControlStashFooterActionKind::CopyRef,
                    "Copy Stash Ref"
                ),
                (
                    SourceControlStashFooterActionKind::CopyMessage,
                    "Copy Stash Message"
                ),
                (SourceControlStashFooterActionKind::Apply, "Apply Stash"),
                (SourceControlStashFooterActionKind::Pop, "Pop Stash"),
                (SourceControlStashFooterActionKind::Drop, "Drop Stash"),
            ]
        );
    }

    #[test]
    fn stash_copy_status_uses_ref_without_intermediate_ref_formatting() {
        let stash = stash(42, "abcd1234", "work in progress");

        assert_eq!(
            source_control_stash_copy_status(&stash, SourceControlStashCopyKind::Ref),
            "Copied stash ref stash@{42}"
        );
        assert_eq!(
            source_control_stash_copy_status(&stash, SourceControlStashCopyKind::Message),
            "Copied stash message for stash@{42}"
        );
    }

    #[test]
    fn stash_filter_requires_all_terms_and_folds_ascii_case() {
        let stashes = vec![
            stash(0, "aaaa0000", "On main: work in progress"),
            stash(1, "bbbb1111", "On feature: WIP router"),
            stash(2, "cccc2222", "On dev: experiment"),
        ];

        assert_eq!(stash_filter_rows(&stashes, ""), vec![0, 1, 2]);
        assert_eq!(stash_filter_rows(&stashes, "   "), vec![0, 1, 2]);

        assert_eq!(stash_filter_rows(&stashes, "bbbb ROUTER"), vec![1]);
        assert_eq!(stash_filter_rows(&stashes, "MAIN progress"), vec![0]);
        assert_eq!(stash_filter_rows(&stashes, "aaaa main"), vec![0]);
        assert!(stash_filter_rows(&stashes, "missing").is_empty());
        assert!(stash_filter_rows(&stashes, "aaaa missing").is_empty());
    }

    #[test]
    fn stash_filter_keeps_original_stash_rows_and_action_targets() {
        let stashes = vec![
            stash(0, "aaaa0000", "latest work"),
            stash(4, "bbbb1111", "middle experiment"),
            stash(9, "cccc2222", "oldest work"),
        ];

        let filtered = source_control_filtered_stash_rows(&stashes, "work");

        assert_eq!(
            filtered
                .iter()
                .map(|(stash_row, _)| *stash_row)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(
            filtered
                .iter()
                .map(|(_, filtered_stash)| filtered_stash.index)
                .collect::<Vec<_>>(),
            vec![0, 9]
        );

        let displays = source_control_stash_visible_rows(&filtered, 0..10)
            .row_displays()
            .collect::<Vec<_>>();
        assert_eq!(
            displays.iter().map(|row| row.row()).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(
            displays[0].target(),
            SourceControlStashActionTarget::new(0, &stashes[0])
        );
        assert_eq!(
            displays[1].target(),
            SourceControlStashActionTarget::new(2, &stashes[2])
        );
        assert_eq!(displays[1].stash_ref(), "stash@{9}");
    }

    #[test]
    fn stash_visible_selected_row_tracks_filtered_positions() {
        let stashes = vec![
            stash(0, "aaaa0000", "latest work"),
            stash(4, "bbbb1111", "middle experiment"),
            stash(9, "cccc2222", "oldest work"),
        ];
        let filtered = source_control_filtered_stash_rows(&stashes, "experiment");

        assert_eq!(
            source_control_visible_selected_stash_row(&filtered, 0, 0..1),
            Some(0)
        );
        assert_eq!(
            source_control_visible_selected_stash_action_target_at(&filtered, 0, 0..1)
                .map(|(target, _)| target),
            Some(SourceControlStashActionTarget::new(1, &stashes[1]))
        );

        assert!(
            source_control_visible_selected_stash_action_target_at(&filtered, 1, 0..1).is_none()
        );
    }

    #[test]
    fn stash_selection_resets_to_first_row_when_query_changes() {
        assert_eq!(source_control_stash_selected_after_query_change(true, 7), 0);
        assert_eq!(
            source_control_stash_selected_after_query_change(false, 7),
            7
        );
    }

    #[test]
    fn stash_count_label_shows_n_of_m_only_while_filtering() {
        assert_eq!(source_control_stash_count_label(3, 3), "3 stashes");
        assert_eq!(source_control_stash_count_label(1, 3), "1 of 3 stashes");
        assert_eq!(source_control_stash_count_label(0, 3), "0 of 3 stashes");
    }
}
