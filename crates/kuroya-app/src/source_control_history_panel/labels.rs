#[cfg(test)]
use super::current_unix_seconds;
use super::source_control_commit_age_label_at;
use crate::path_display::sanitized_display_label_cow;
use kuroya_core::{GitCommitSummary, GitRemoteDivergence};
use std::borrow::Cow;

pub(super) const SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS: usize = 64;
pub(super) const SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS: usize = 160;
const SOURCE_CONTROL_HISTORY_COMMIT_AUTHOR_DISPLAY_MAX_CHARS: usize = 120;
const SOURCE_CONTROL_HISTORY_DISPLAY_SAMPLE_EXTRA_CHARS: usize = 32;
#[cfg(test)]
const SOURCE_CONTROL_HISTORY_AGE_DISPLAY_MAX_CHARS: usize = 24;
#[cfg(test)]
pub(super) const SOURCE_CONTROL_HISTORY_ROW_LABEL_MAX_CHARS: usize =
    SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS
        + SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS
        + SOURCE_CONTROL_HISTORY_COMMIT_AUTHOR_DISPLAY_MAX_CHARS
        + SOURCE_CONTROL_HISTORY_AGE_DISPLAY_MAX_CHARS
        + 6;

#[cfg(test)]
pub(crate) fn source_control_commit_label(
    commit: &GitCommitSummary,
    now_seconds: i64,
    show_author: bool,
) -> String {
    SourceControlHistoryRowDisplay::new(commit, now_seconds, show_author)
        .label()
        .to_owned()
}

#[derive(Debug, Clone)]
pub(super) struct SourceControlHistoryRowDisplay<'a> {
    pub(super) short_oid: Cow<'a, str>,
    pub(super) summary: Cow<'a, str>,
    detail: String,
    label: String,
    pub(super) label_age_start: Option<usize>,
    author_visible: bool,
    pub(super) tooltip: Option<String>,
}

impl<'a> SourceControlHistoryRowDisplay<'a> {
    pub(super) fn new(commit: &'a GitCommitSummary, now_seconds: i64, show_author: bool) -> Self {
        let short_oid = source_control_commit_id_display_text(&commit.short_oid);
        let summary = source_control_commit_summary_display_text(&commit.summary);
        let age = source_control_commit_age_label_at(commit, now_seconds);
        let author = show_author.then(|| source_control_commit_author_display_text(&commit.author));
        let detail = source_control_commit_detail_from_display(
            author.as_ref().map(|author| author.as_ref()),
            &age,
        );
        let label_age_start = (!show_author)
            .then(|| source_control_commit_label_age_start(short_oid.as_ref(), summary.as_ref()));
        let label = source_control_commit_label_from_display(
            short_oid.as_ref(),
            summary.as_ref(),
            author.as_ref().map(|author| author.as_ref()),
            &age,
        );
        Self {
            short_oid,
            summary,
            detail,
            label,
            label_age_start,
            author_visible: show_author,
            tooltip: None,
        }
    }

    #[cfg(test)]
    pub(super) fn label(&self) -> &str {
        &self.label
    }

    pub(super) fn short_oid(&self) -> &str {
        self.short_oid.as_ref()
    }

    pub(super) fn summary(&self) -> &str {
        self.summary.as_ref()
    }

    pub(super) fn detail(&self) -> &str {
        &self.detail
    }

    pub(super) fn tooltip(&mut self, commit: &GitCommitSummary) -> &str {
        if self.author_visible {
            return &self.label;
        }
        if self.tooltip.is_none() {
            let author = source_control_commit_author_display_text(&commit.author);
            self.tooltip = Some(source_control_commit_label_from_display(
                self.short_oid.as_ref(),
                self.summary.as_ref(),
                Some(author.as_ref()),
                self.label_age(),
            ));
        }
        self.tooltip.as_deref().unwrap_or(&self.label)
    }

    pub(super) fn label_age(&self) -> &str {
        self.label_age_start
            .and_then(|start| self.label.get(start..))
            .unwrap_or_default()
    }

    pub(super) fn copy_status(&self, kind: SourceControlCommitCopyKind) -> String {
        source_control_commit_copy_status_for_id(self.short_oid.as_ref(), kind)
    }
}

fn source_control_commit_label_age_start(short_oid: &str, summary: &str) -> usize {
    short_oid.len() + 2 + summary.len() + 2
}

fn source_control_commit_detail_from_display(author: Option<&str>, age: &str) -> String {
    let Some(author) = author else {
        return age.to_owned();
    };
    let mut detail =
        String::with_capacity(author.len().saturating_add(age.len()).saturating_add(2));
    detail.push_str(author);
    detail.push_str("  ");
    detail.push_str(age);
    detail
}

fn source_control_commit_label_from_display(
    short_oid: &str,
    summary: &str,
    author: Option<&str>,
    age: &str,
) -> String {
    let mut label = String::with_capacity(
        short_oid
            .len()
            .saturating_add(summary.len())
            .saturating_add(age.len())
            .saturating_add(if let Some(author) = author {
                author.len().saturating_add(6)
            } else {
                4
            }),
    );
    label.push_str(short_oid);
    label.push_str("  ");
    label.push_str(summary);
    label.push_str("  ");
    if let Some(author) = author {
        label.push_str(author);
        label.push_str("  ");
    }
    label.push_str(age);
    label
}

#[cfg(test)]
pub(super) fn source_control_commit_id_display(value: &str) -> String {
    source_control_commit_id_display_text(value).into_owned()
}

pub(super) fn source_control_commit_id_display_text(value: &str) -> Cow<'_, str> {
    source_control_commit_display_field_text(
        value,
        SOURCE_CONTROL_HISTORY_COMMIT_ID_DISPLAY_MAX_CHARS,
        "unknown",
    )
}

#[cfg(test)]
pub(super) fn source_control_commit_summary_display(value: &str) -> String {
    source_control_commit_summary_display_text(value).into_owned()
}

fn source_control_commit_summary_display_text(value: &str) -> Cow<'_, str> {
    source_control_commit_display_field_text(
        value,
        SOURCE_CONTROL_HISTORY_COMMIT_SUMMARY_DISPLAY_MAX_CHARS,
        "No message",
    )
}

fn source_control_commit_author_display_text(value: &str) -> Cow<'_, str> {
    source_control_commit_display_field_text(
        value,
        SOURCE_CONTROL_HISTORY_COMMIT_AUTHOR_DISPLAY_MAX_CHARS,
        "Unknown author",
    )
}

#[cfg(test)]
pub(super) fn source_control_commit_display_field(
    value: &str,
    max_chars: usize,
    fallback: &str,
) -> String {
    source_control_commit_display_field_text(value, max_chars, fallback).into_owned()
}

pub(super) fn source_control_commit_display_field_text<'a>(
    value: &'a str,
    max_chars: usize,
    fallback: &str,
) -> Cow<'a, str> {
    match source_control_history_display_sample(value, max_chars) {
        Cow::Borrowed(sample) => sanitized_display_label_cow(sample, max_chars, fallback),
        Cow::Owned(sample) => Cow::Owned(
            sanitized_display_label_cow(sample.as_ref(), max_chars, fallback).into_owned(),
        ),
    }
}

pub(super) fn source_control_history_display_sample(value: &str, max_chars: usize) -> Cow<'_, str> {
    if value.len() <= source_control_history_display_sample_byte_limit(max_chars) {
        return Cow::Borrowed(value);
    }

    let keep_chars = max_chars
        .saturating_add(SOURCE_CONTROL_HISTORY_DISPLAY_SAMPLE_EXTRA_CHARS)
        .max(1);
    let head_chars = keep_chars / 2;
    let tail_chars = keep_chars.saturating_sub(head_chars);

    let head_end = source_control_history_sample_head_end(value, head_chars);
    let tail_start = source_control_history_sample_tail_start(value, tail_chars);
    if tail_start <= head_end {
        return Cow::Borrowed(value);
    }

    Cow::Owned(source_control_history_join_display_sample(
        &value[..head_end],
        &value[tail_start..],
    ))
}

fn source_control_history_sample_head_end(value: &str, chars: usize) -> usize {
    if chars == 0 {
        return 0;
    }
    value
        .char_indices()
        .nth(chars)
        .map_or(value.len(), |(index, _)| index)
}

fn source_control_history_sample_tail_start(value: &str, chars: usize) -> usize {
    if chars == 0 {
        return value.len();
    }
    value
        .char_indices()
        .rev()
        .nth(chars - 1)
        .map_or(0, |(index, _)| index)
}

fn source_control_history_display_sample_byte_limit(max_chars: usize) -> usize {
    max_chars
        .saturating_add(SOURCE_CONTROL_HISTORY_DISPLAY_SAMPLE_EXTRA_CHARS)
        .saturating_mul(8)
        .max(512)
}

fn source_control_history_join_display_sample(head: &str, tail: &str) -> String {
    let mut sample = String::with_capacity(head.len().saturating_add(tail.len()).saturating_add(3));
    sample.push_str(head);
    sample.push_str("...");
    sample.push_str(tail);
    sample
}

pub(crate) fn source_control_graph_divergence_label(
    divergence: Option<GitRemoteDivergence>,
    show_incoming: bool,
    show_outgoing: bool,
) -> Option<String> {
    let divergence = divergence?;
    let incoming_visible = show_incoming && divergence.incoming > 0;
    let outgoing_visible = show_outgoing && divergence.outgoing > 0;
    match (incoming_visible, outgoing_visible) {
        (true, true) => Some(format!(
            "Incoming {} Outgoing {}",
            divergence.incoming, divergence.outgoing
        )),
        (true, false) => Some(format!("Incoming {}", divergence.incoming)),
        (false, true) => Some(format!("Outgoing {}", divergence.outgoing)),
        (false, false) => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceControlCommitCopyKind {
    Oid,
    ShortOid,
    Summary,
    Author,
    Age,
}

#[cfg(test)]
pub(crate) fn source_control_commit_copy_text(
    commit: &GitCommitSummary,
    kind: SourceControlCommitCopyKind,
) -> String {
    source_control_commit_copy_text_at(commit, kind, current_unix_seconds())
}

pub(crate) fn source_control_commit_copy_text_at(
    commit: &GitCommitSummary,
    kind: SourceControlCommitCopyKind,
    now_seconds: i64,
) -> String {
    match kind {
        SourceControlCommitCopyKind::Oid => commit.oid.clone(),
        SourceControlCommitCopyKind::ShortOid => commit.short_oid.clone(),
        SourceControlCommitCopyKind::Summary => commit.summary.clone(),
        SourceControlCommitCopyKind::Author => commit.author.clone(),
        SourceControlCommitCopyKind::Age => source_control_commit_age_label_at(commit, now_seconds),
    }
}

pub(crate) fn source_control_commit_copy_status(
    commit: &GitCommitSummary,
    kind: SourceControlCommitCopyKind,
) -> String {
    let short_oid = source_control_commit_id_display_text(&commit.short_oid);
    source_control_commit_copy_status_for_id(short_oid.as_ref(), kind)
}

fn source_control_commit_copy_status_for_id(
    short_oid: &str,
    kind: SourceControlCommitCopyKind,
) -> String {
    match kind {
        SourceControlCommitCopyKind::Oid => {
            format!("Copied commit ID {short_oid}")
        }
        SourceControlCommitCopyKind::ShortOid => {
            format!("Copied short commit ID {short_oid}")
        }
        SourceControlCommitCopyKind::Summary => {
            format!("Copied commit message for {short_oid}")
        }
        SourceControlCommitCopyKind::Author => {
            format!("Copied commit author for {short_oid}")
        }
        SourceControlCommitCopyKind::Age => {
            format!("Copied commit age for {short_oid}")
        }
    }
}
