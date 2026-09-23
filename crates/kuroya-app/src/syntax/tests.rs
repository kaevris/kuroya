use super::*;
use crate::syntax_cache::MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE;
use kuroya_core::{
    MAX_PLUGIN_SYNTAX_BYTES, PLUGIN_API_VERSION, PluginCapabilities, PluginContributions,
    PluginLanguageContribution, PluginManifest, PluginSyntaxContribution, TextEdit,
};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "kuroya-app-syntax-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn toy_plugin(root: std::path::PathBuf, syntax_text: &str) -> PluginDescriptor {
    let syntax_dir = root.join("syntax");
    std::fs::create_dir_all(&syntax_dir).unwrap();
    let syntax_path = syntax_dir.join("toy.sublime-syntax");
    std::fs::write(&syntax_path, syntax_text).unwrap();

    PluginDescriptor {
        root,
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "toy.plugin".to_owned(),
            name: "Toy".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                languages: true,
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "toy-lang".to_owned(),
                    extensions: vec!["toy".to_owned()],
                    aliases: vec!["Toy".to_owned()],
                }],
                syntaxes: vec![PluginSyntaxContribution {
                    language: "toy-lang".to_owned(),
                    path: syntax_path,
                }],
                ..PluginContributions::default()
            },
        },
    }
}

#[test]
fn visible_highlighting_uses_stateful_checkpoints() {
    let text = (0..140)
        .map(|line| {
            if line == 5 {
                "/* comment".to_owned()
            } else if line == 120 {
                "comment end */".to_owned()
            } else {
                format!("let value_{line} = {line};")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let mut highlighter = SyntaxHighlighter::new();

    let first =
        highlighter.layout_visible(&buffer, 13.0, 4, 0..130, true, egui::Color32::WHITE, -1);
    let second =
        highlighter.layout_visible(&buffer, 13.0, 4, 100..105, true, egui::Color32::WHITE, -1);

    assert_eq!(first.len(), 130);
    assert_eq!(second.len(), 5);
    assert!(
        highlighter
            .caches
            .values()
            .any(|cache| cache.checkpoint_count() > 1)
    );
}

#[test]
fn visible_highlighting_reuses_visible_layout_cache_for_same_viewport() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\nlet next = 2;\n".to_owned(),
    );
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    let first = highlighter.layout_visible(&buffer, 13.0, 4, 0..2, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout should populate the highlight cache");
    assert_eq!(cache.visible_layout_count(), 1);
    assert_eq!(cache.visible_layout_hits(), 0);

    highlighter.syntaxes = syntect::parsing::SyntaxSet::new();
    let second = highlighter.layout_visible(&buffer, 13.0, 4, 0..2, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available");

    assert_eq!(first.len(), second.len());
    assert_eq!(first[0].text, second[0].text);
    assert_eq!(first[1].text, second[1].text);
    assert_eq!(cache.visible_layout_count(), 1);
    assert_eq!(cache.visible_layout_hits(), 1);
}

#[test]
fn visible_highlighting_prunes_stale_versions_for_same_buffer() {
    let mut buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\n".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    let old_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    assert!(highlighter.caches.contains_key(&old_key));

    buffer.apply_edit(TextEdit {
        range: 0..0,
        inserted: "// updated\n".to_owned(),
    });
    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    let new_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);

    assert_ne!(old_key, new_key);
    assert!(!highlighter.caches.contains_key(&old_key));
    assert!(highlighter.caches.contains_key(&new_key));
    assert_eq!(
        highlighter
            .cache_order
            .iter()
            .filter(|cached| cached.is_for_same_buffer(&new_key))
            .count(),
        1
    );
}

#[test]
fn visible_highlighting_prunes_stale_syntax_identity_for_same_buffer() {
    let mut buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("notes.rs")),
        "print('value')\n".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    let old_key = HighlightCacheKey::for_buffer_with_extension(
        &buffer,
        13.0,
        4,
        "rs",
        None,
        egui::Color32::WHITE,
    );
    assert!(highlighter.caches.contains_key(&old_key));

    buffer.set_path(PathBuf::from("script.py"));
    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    let new_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);

    assert_ne!(old_key, new_key);
    assert!(!highlighter.caches.contains_key(&old_key));
    assert!(highlighter.caches.contains_key(&new_key));
    assert_eq!(
        highlighter
            .cache_order
            .iter()
            .filter(|cached| cached.is_for_same_buffer(&new_key))
            .count(),
        1
    );
}

#[test]
fn visible_highlighting_skips_cache_for_empty_or_out_of_bounds_rows() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\n".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();
    let reversed_start = 10;
    let reversed_end = 5;

    assert!(
        highlighter
            .layout_visible(
                &buffer,
                13.0,
                4,
                reversed_start..reversed_end,
                true,
                egui::Color32::WHITE,
                -1
            )
            .is_empty()
    );
    assert!(highlighter.caches.is_empty());
    assert!(highlighter.cache_order.is_empty());

    assert!(
        highlighter
            .layout_visible(&buffer, 13.0, 4, 99..100, true, egui::Color32::WHITE, -1)
            .is_empty()
    );
    assert!(highlighter.caches.is_empty());
    assert!(highlighter.cache_order.is_empty());
}

#[test]
fn visible_highlighting_reuses_cached_subrange_of_previous_viewport() {
    let text = (0..8)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    let first = highlighter.layout_visible(&buffer, 13.0, 4, 0..8, true, egui::Color32::WHITE, -1);
    let subrange =
        highlighter.layout_visible(&buffer, 13.0, 4, 2..5, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available");

    assert_eq!(subrange.len(), 3);
    assert_eq!(subrange, first[2..5]);
    assert_eq!(cache.visible_layout_count(), 1);
    assert_eq!(cache.visible_layout_hits(), 1);
}

#[test]
fn visible_highlighting_does_not_reuse_partial_overlap() {
    let text = (0..8)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 2..6, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 1..5, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available");

    assert_eq!(cache.visible_layout_count(), 2);
    assert_eq!(cache.visible_layout_hits(), 0);
}

#[test]
fn visible_layout_cache_hit_refreshes_lru_order() {
    let text = (0..100)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    for start in (0..80).step_by(10) {
        highlighter.layout_visible(
            &buffer,
            13.0,
            4,
            start..start + 10,
            true,
            egui::Color32::WHITE,
            -1,
        );
    }
    highlighter.layout_visible(&buffer, 13.0, 4, 0..5, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 80..90, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 0..5, true, egui::Color32::WHITE, -1);
    let hits_after_refreshed_subrange = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available")
        .visible_layout_hits();

    highlighter.layout_visible(&buffer, 13.0, 4, 10..15, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available");

    assert_eq!(hits_after_refreshed_subrange, 2);
    assert_eq!(cache.visible_layout_hits(), 2);
    assert_eq!(
        cache.visible_layout_count(),
        MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE
    );
}

#[test]
fn visible_layout_subrange_prefers_most_recent_containing_range() {
    let text = (0..100)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..10, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 0..20, true, egui::Color32::WHITE, -1);
    for start in (20..80).step_by(10) {
        highlighter.layout_visible(
            &buffer,
            13.0,
            4,
            start..start + 10,
            true,
            egui::Color32::WHITE,
            -1,
        );
    }
    highlighter.layout_visible(&buffer, 13.0, 4, 2..5, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 80..90, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&buffer, 13.0, 4, 0..15, true, egui::Color32::WHITE, -1);
    let cache = highlighter
        .caches
        .get(&key)
        .expect("visible layout cache should still be available");

    assert_eq!(cache.visible_layout_hits(), 2);
    assert_eq!(
        cache.visible_layout_count(),
        MAX_VISIBLE_LAYOUT_RANGES_PER_CACHE
    );
}

#[test]
fn visible_highlighting_cache_distinguishes_line_render_limits() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        format!("{}tail\nshort", "x".repeat(64)),
    );
    let limited_key = HighlightCacheKey::for_buffer_with_extension(
        &buffer,
        13.0,
        4,
        "rs",
        Some(12),
        egui::Color32::WHITE,
    );
    let full_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    let limited =
        highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, 12);
    let full = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);

    assert_eq!(limited[0].text, "x".repeat(12));
    assert_eq!(full[0].text, format!("{}tail", "x".repeat(64)));
    assert!(highlighter.caches.contains_key(&limited_key));
    assert!(highlighter.caches.contains_key(&full_key));
}

#[test]
fn disabled_syntax_highlighting_returns_plain_jobs_without_cache() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let\tvalue = 1;\nnext".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();

    let jobs = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, false, egui::Color32::WHITE, -1);

    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].text, "let value = 1;");
    assert!(highlighter.caches.is_empty());
    assert!(highlighter.cache_order.is_empty());
}

#[test]
fn disabled_syntax_highlighting_clears_existing_cache() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\n".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    assert!(!highlighter.caches.is_empty());

    let jobs = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, false, egui::Color32::WHITE, -1);

    assert_eq!(jobs.len(), 1);
    assert!(highlighter.caches.is_empty());
    assert!(highlighter.cache_order.is_empty());
}

#[test]
fn disabled_syntax_highlighting_clears_only_target_buffer_cache() {
    let first = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let first = 1;\n".to_owned(),
    );
    let second = TextBuffer::from_text(
        2,
        Some(PathBuf::from("src/main.rs")),
        "let second = 2;\n".to_owned(),
    );
    let first_key = HighlightCacheKey::for_buffer(&first, 13.0, 4, egui::Color32::WHITE);
    let second_key = HighlightCacheKey::for_buffer(&second, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&first, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(&second, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    assert!(highlighter.caches.contains_key(&first_key));
    assert!(highlighter.caches.contains_key(&second_key));

    let jobs = highlighter.layout_visible(&second, 13.0, 4, 0..1, false, egui::Color32::WHITE, -1);

    assert_eq!(jobs.len(), 1);
    assert!(highlighter.caches.contains_key(&first_key));
    assert!(!highlighter.caches.contains_key(&second_key));
    assert_eq!(
        highlighter.cache_order.iter().collect::<Vec<_>>(),
        vec![&first_key]
    );
}

#[test]
fn disabled_syntax_highlighting_ignores_impossible_rows_without_cache_churn() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\n".to_owned(),
    );
    let key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    assert!(highlighter.caches.contains_key(&key));
    assert_eq!(
        highlighter.cache_order.iter().collect::<Vec<_>>(),
        vec![&key]
    );

    let jobs = highlighter.layout_visible(
        &buffer,
        13.0,
        4,
        usize::MAX..usize::MAX,
        false,
        egui::Color32::WHITE,
        -1,
    );

    assert!(jobs.is_empty());
    assert!(highlighter.caches.contains_key(&key));
    assert_eq!(
        highlighter.cache_order.iter().collect::<Vec<_>>(),
        vec![&key]
    );
}

#[test]
fn visible_layout_caps_long_lines_before_building_jobs() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        format!("{}tail\nshort", "x".repeat(64)),
    );
    let mut highlighter = SyntaxHighlighter::new();

    let plain = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, false, egui::Color32::WHITE, 12);
    let highlighted =
        highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, 12);

    assert_eq!(plain[0].text, "x".repeat(12));
    assert_eq!(highlighted[0].text, "x".repeat(12));
}

#[test]
fn deep_visible_highlighting_falls_back_to_plain_and_warms_bounded_checkpoints() {
    let start = MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT + CHECKPOINT_INTERVAL + 10;
    let text = (0..start + 3)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer_with_extension(
        &buffer,
        13.0,
        4,
        "rs",
        None,
        egui::Color32::WHITE,
    );
    let mut highlighter = SyntaxHighlighter::new();

    let jobs = highlighter.layout_visible(
        &buffer,
        13.0,
        4,
        start..start + 3,
        true,
        egui::Color32::WHITE,
        -1,
    );

    assert_eq!(jobs.len(), 3);
    assert_eq!(jobs[0].text, format!("let value_{start} = {start};"));
    let cache = highlighter
        .caches
        .get(&key)
        .expect("deep fallback should still warm true checkpoints");
    assert_eq!(
        cache.max_checkpoint_line(),
        Some(MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT)
    );
    assert_eq!(cache.visible_layout_count(), 0);
}

#[test]
fn repeated_deep_visible_highlighting_progresses_true_warmup() {
    let start = MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT * 3 + 10;
    let text = (0..start + 3)
        .map(|line| format!("let value_{line} = {line};"))
        .collect::<Vec<_>>()
        .join("\n");
    let buffer = TextBuffer::from_text(1, Some(PathBuf::from("src/main.rs")), text);
    let key = HighlightCacheKey::for_buffer_with_extension(
        &buffer,
        13.0,
        4,
        "rs",
        None,
        egui::Color32::WHITE,
    );
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(
        &buffer,
        13.0,
        4,
        start..start + 1,
        true,
        egui::Color32::WHITE,
        -1,
    );
    let first_max = highlighter
        .caches
        .get(&key)
        .and_then(|cache| cache.max_checkpoint_line());
    highlighter.layout_visible(
        &buffer,
        13.0,
        4,
        start..start + 1,
        true,
        egui::Color32::WHITE,
        -1,
    );
    let second_max = highlighter
        .caches
        .get(&key)
        .and_then(|cache| cache.max_checkpoint_line());

    assert_eq!(first_max, Some(MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT));
    assert_eq!(second_max, Some(MAX_HIGHLIGHT_REPLAY_LINES_PER_LAYOUT * 2));
}

#[test]
fn highlight_cache_evicts_oldest_key_without_clearing_warm_caches() {
    let buffers = (1..=(MAX_HIGHLIGHT_CACHES as u64 + 1))
        .map(|id| {
            TextBuffer::from_text(
                id,
                Some(PathBuf::from("src/main.rs")),
                format!("let value_{id} = {id};\n"),
            )
        })
        .collect::<Vec<_>>();
    let first_key = HighlightCacheKey::for_buffer(&buffers[0], 13.0, 4, egui::Color32::WHITE);
    let second_key = HighlightCacheKey::for_buffer(&buffers[1], 13.0, 4, egui::Color32::WHITE);
    let newest_key =
        HighlightCacheKey::for_buffer(buffers.last().unwrap(), 13.0, 4, egui::Color32::WHITE);
    let mut highlighter = SyntaxHighlighter::new();

    for buffer in buffers.iter().take(MAX_HIGHLIGHT_CACHES) {
        highlighter.layout_visible(buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    }
    assert_eq!(highlighter.caches.len(), MAX_HIGHLIGHT_CACHES);

    highlighter.layout_visible(&buffers[0], 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    highlighter.layout_visible(
        buffers.last().unwrap(),
        13.0,
        4,
        0..1,
        true,
        egui::Color32::WHITE,
        -1,
    );

    assert_eq!(highlighter.caches.len(), MAX_HIGHLIGHT_CACHES);
    assert!(highlighter.caches.contains_key(&first_key));
    assert!(!highlighter.caches.contains_key(&second_key));
    assert!(highlighter.caches.contains_key(&newest_key));
}

#[test]
fn plugin_syntax_load_registers_language_extension_for_highlighting() {
    let root = temp_root("plugin-load");
    let plugin = toy_plugin(
        root.clone(),
        r#"%YAML 1.2
---
name: Toy
file_extensions: []
scope: source.toy
contexts:
  main:
    - match: '\b(keyword)\b'
      scope: keyword.control.toy
"#,
    );
    let syntax_load = PluginSyntaxLoad::from_plugins(&[plugin]);

    assert!(syntax_load.errors.is_empty());
    assert!(
        syntax_load
            .registry
            .syntax_for_language("toy-lang")
            .is_some()
    );

    let mut highlighter = SyntaxHighlighter::new();
    highlighter.install_plugin_syntaxes(syntax_load);
    let buffer = TextBuffer::from_text(
        99,
        Some(root.join("src").join("main.toy")),
        "keyword value".to_owned(),
    );

    assert_eq!(buffer.language(), LanguageId::PlainText);
    assert_eq!(highlighter.syntax_name_for_buffer(&buffer), "Toy");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_syntax_load_keeps_defaults_when_one_syntax_fails() {
    let root = temp_root("plugin-load-error");
    let plugin = toy_plugin(root.clone(), "not: [valid");

    let syntax_load = PluginSyntaxLoad::from_plugins(&[plugin]);

    assert_eq!(syntax_load.errors.len(), 1);
    assert!(syntax_load.registry.is_empty());

    let mut highlighter = SyntaxHighlighter::new();
    highlighter.install_plugin_syntaxes(syntax_load);
    let rust = TextBuffer::from_text(
        100,
        Some(std::path::PathBuf::from("src/main.rs")),
        "fn main() {}".to_owned(),
    );

    assert_eq!(highlighter.syntax_name_for_buffer(&rust), "Rust");

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn plugin_syntax_load_skips_oversized_syntax_file() {
    let root = temp_root("plugin-load-oversized");
    let syntax_dir = root.join("syntax");
    std::fs::create_dir_all(&syntax_dir).unwrap();
    let syntax_path = syntax_dir.join("huge.sublime-syntax");
    std::fs::write(
        &syntax_path,
        vec![b'a'; usize::try_from(MAX_PLUGIN_SYNTAX_BYTES + 1).unwrap()],
    )
    .unwrap();
    let plugin = PluginDescriptor {
        root: root.clone(),
        manifest: PluginManifest {
            api_version: PLUGIN_API_VERSION.to_owned(),
            id: "toy.plugin".to_owned(),
            name: "Toy".to_owned(),
            version: "0.1.0".to_owned(),
            entry: None,
            activation_events: Vec::new(),
            capabilities: PluginCapabilities {
                languages: true,
                syntax: true,
                ..PluginCapabilities::default()
            },
            contributes: PluginContributions {
                languages: vec![PluginLanguageContribution {
                    id: "toy-lang".to_owned(),
                    extensions: vec!["toy".to_owned()],
                    aliases: vec!["Toy".to_owned()],
                }],
                syntaxes: vec![PluginSyntaxContribution {
                    language: "toy-lang".to_owned(),
                    path: syntax_path,
                }],
                ..PluginContributions::default()
            },
        },
    };

    let syntax_load = PluginSyntaxLoad::from_plugins(&[plugin]);

    assert_eq!(syntax_load.errors.len(), 1);
    assert!(syntax_load.errors[0].error.contains("plugin file limit"));
    assert!(syntax_load.registry.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn visible_highlighting_rebuilds_theme_colors_on_theme_identity_change() {
    let buffer = TextBuffer::from_text(
        1,
        Some(PathBuf::from("src/main.rs")),
        "let value = 1;\n".to_owned(),
    );
    let mut highlighter = SyntaxHighlighter::new();

    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, egui::Color32::WHITE, -1);
    let dark_theme_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, egui::Color32::WHITE);
    assert!(highlighter.caches.contains_key(&dark_theme_key));

    let light_theme_text = egui::Color32::from_rgb(36, 41, 49);
    highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, light_theme_text, -1);
    let light_theme_key = HighlightCacheKey::for_buffer(&buffer, 13.0, 4, light_theme_text);

    assert_ne!(dark_theme_key, light_theme_key);
    assert!(highlighter.caches.contains_key(&light_theme_key));

    // The rebuild must not serve stale colors from the old theme's cache.
    let jobs = highlighter.layout_visible(&buffer, 13.0, 4, 0..1, true, light_theme_text, -1);
    assert_eq!(jobs[0].sections[0].format.color, light_theme_text);
}
