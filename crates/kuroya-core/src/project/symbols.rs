use crate::LanguageId;

use super::{
    MAX_SYMBOL_FILE_BYTES, MAX_SYMBOL_LINE_BYTES, MAX_SYMBOLS_PER_FILE, ProjectSymbol,
    ProjectSymbolKind,
};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

mod ast_symbols;
#[cfg(test)]
mod ast_tests;

const AST_MAX_FILE_BYTES: usize = 256 * 1024;

pub(super) const RUST_AST_PARSE_BUDGET_BYTES: u64 = 8 * 1024 * 1024;

pub(super) fn extract_project_symbols(
    path: &Path,
    relative_path: &Path,
    file_len: Option<u64>,
    remaining: usize,
    ast_budget: &mut u64,
) -> Vec<ProjectSymbol> {
    if remaining == 0 {
        return Vec::new();
    }
    let Some(language) = symbol_language_from_source_extension(path) else {
        return Vec::new();
    };
    if file_len.is_some_and(|len| len > MAX_SYMBOL_FILE_BYTES) {
        return Vec::new();
    }
    let Some(text) = read_symbol_text_with_limit(path, MAX_SYMBOL_FILE_BYTES) else {
        return Vec::new();
    };
    let per_file_limit = remaining.min(MAX_SYMBOLS_PER_FILE);

    if *ast_budget > 0
        && text.len() <= AST_MAX_FILE_BYTES
        && let Some(extracted) = ast_symbols::extract_ast_symbols(language, &text, per_file_limit)
        && !extracted.is_empty()
    {
        *ast_budget = ast_budget.saturating_sub(text.len() as u64);
        let paths = (path.to_path_buf(), relative_path.to_path_buf());
        return extracted
            .into_iter()
            .map(|(name, kind, line, column)| ProjectSymbol {
                name,
                kind,
                path: paths.0.clone(),
                relative_path: paths.1.clone(),
                line,
                column,
            })
            .collect();
    }

    let mut symbol_paths = None::<(PathBuf, PathBuf)>;
    let mut symbols = Vec::with_capacity(per_file_limit.min(16));
    for (line_idx, line) in text.lines().enumerate() {
        if symbols.len() >= per_file_limit {
            break;
        }
        if line.len() > MAX_SYMBOL_LINE_BYTES {
            continue;
        }
        let Some((name, kind, column)) = symbol_from_line(language, line) else {
            continue;
        };
        let paths =
            symbol_paths.get_or_insert_with(|| (path.to_path_buf(), relative_path.to_path_buf()));
        symbols.push(ProjectSymbol {
            name,
            kind,
            path: paths.0.clone(),
            relative_path: paths.1.clone(),
            line: line_idx.saturating_add(1),
            column: column.saturating_add(1),
        });
    }
    symbols
}

pub(super) fn read_symbol_text_with_limit(path: &Path, max_bytes: u64) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let max_read = max_bytes.saturating_add(1);
    let capacity = usize::try_from(max_read)
        .unwrap_or(usize::MAX)
        .min(64 * 1024);
    let mut bytes = Vec::with_capacity(capacity);
    file.take(max_read).read_to_end(&mut bytes).ok()?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_bytes {
        return None;
    }
    String::from_utf8(bytes).ok()
}

fn symbol_language_from_source_extension(path: &Path) -> Option<LanguageId> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("rs") {
        Some(LanguageId::Rust)
    } else if extension.eq_ignore_ascii_case("py") {
        Some(LanguageId::Python)
    } else if matches_ignore_ascii_case(extension, &["ts", "tsx"]) {
        Some(LanguageId::TypeScript)
    } else if matches_ignore_ascii_case(extension, &["js", "jsx", "mjs", "cjs"]) {
        Some(LanguageId::JavaScript)
    } else if extension.eq_ignore_ascii_case("go") {
        Some(LanguageId::Go)
    } else if extension.eq_ignore_ascii_case("java") {
        Some(LanguageId::Java)
    } else if matches_ignore_ascii_case(extension, &["c", "h"]) {
        Some(LanguageId::C)
    } else if matches_ignore_ascii_case(extension, &["cc", "cpp", "cxx", "hh", "hpp", "hxx"]) {
        Some(LanguageId::Cpp)
    } else if extension.eq_ignore_ascii_case("cs") {
        Some(LanguageId::CSharp)
    } else {
        None
    }
}

fn matches_ignore_ascii_case(value: &str, candidates: &[&str]) -> bool {
    candidates
        .iter()
        .any(|candidate| value.eq_ignore_ascii_case(candidate))
}

fn symbol_from_line(
    language: LanguageId,
    line: &str,
) -> Option<(String, ProjectSymbolKind, usize)> {
    match language {
        LanguageId::Rust => rust_symbol_from_line(line),
        LanguageId::Python => python_symbol_from_line(line),
        LanguageId::TypeScript | LanguageId::JavaScript => script_symbol_from_line(line),
        LanguageId::Go => go_symbol_from_line(line),
        LanguageId::Java | LanguageId::CSharp => java_family_symbol_from_line(line),
        LanguageId::C | LanguageId::Cpp => c_family_symbol_from_line(line),
        _ => None,
    }
}

fn rust_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with("#[") {
        return None;
    }
    let mut rest = strip_prefix_words(trimmed, &["pub", "async", "unsafe", "extern"]);
    if rest.starts_with("pub(") {
        rest = rest.split_once(')')?.1.trim_start();
        rest = strip_prefix_words(rest, &["async", "unsafe", "extern"]);
    }
    let candidates = [
        ("fn ", ProjectSymbolKind::Function),
        ("struct ", ProjectSymbolKind::Struct),
        ("enum ", ProjectSymbolKind::Enum),
        ("trait ", ProjectSymbolKind::Interface),
        ("mod ", ProjectSymbolKind::Module),
        ("const ", ProjectSymbolKind::Constant),
        ("static ", ProjectSymbolKind::Constant),
        ("type ", ProjectSymbolKind::Type),
    ];
    candidates.iter().find_map(|(prefix, kind)| {
        rest.strip_prefix(prefix).and_then(|after| {
            let name = parse_identifier(after)?;
            let column = line.find(name)?;
            Some((name.to_owned(), *kind, column))
        })
    })
}

fn python_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (after, kind) = trimmed
        .strip_prefix("async def ")
        .map(|after| (after, ProjectSymbolKind::Function))
        .or_else(|| {
            trimmed
                .strip_prefix("def ")
                .map(|after| (after, ProjectSymbolKind::Function))
        })
        .or_else(|| {
            trimmed
                .strip_prefix("class ")
                .map(|after| (after, ProjectSymbolKind::Class))
        })?;
    let name = parse_identifier(after)?;
    let column = line.find(name)?;
    Some((name.to_owned(), kind, column))
}

fn script_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }
    let rest = strip_prefix_words(
        trimmed,
        &["export", "default", "async", "declare", "public", "private"],
    );
    let candidates = [
        ("function ", ProjectSymbolKind::Function),
        ("class ", ProjectSymbolKind::Class),
        ("interface ", ProjectSymbolKind::Interface),
        ("type ", ProjectSymbolKind::Type),
        ("enum ", ProjectSymbolKind::Enum),
    ];
    if let Some(symbol) = candidates.iter().find_map(|(prefix, kind)| {
        rest.strip_prefix(prefix).and_then(|after| {
            let name = parse_identifier(after)?;
            let column = line.find(name)?;
            Some((name.to_owned(), *kind, column))
        })
    }) {
        return Some(symbol);
    }

    script_binding_symbol_from_line(line, rest)
}

fn script_binding_symbol_from_line(
    line: &str,
    rest: &str,
) -> Option<(String, ProjectSymbolKind, usize)> {
    let bindings = [
        ("const ", ProjectSymbolKind::Constant),
        ("let ", ProjectSymbolKind::Variable),
        ("var ", ProjectSymbolKind::Variable),
    ];
    bindings.iter().find_map(|(prefix, fallback_kind)| {
        rest.strip_prefix(prefix).and_then(|after| {
            let name = parse_identifier(after)?;
            let after_name = after.get(name.len()..)?.trim_start();
            let kind = after_name
                .split_once('=')
                .map(|(_, assigned)| assigned.trim_start())
                .filter(|assigned| script_assignment_is_function_like(assigned))
                .map(|_| ProjectSymbolKind::Function)
                .unwrap_or(*fallback_kind);
            let column = line.find(name)?;
            Some((name.to_owned(), kind, column))
        })
    })
}

fn script_assignment_is_function_like(assigned: &str) -> bool {
    if script_starts_with_word(assigned, "function") {
        return true;
    }
    if let Some(after_async) = assigned.strip_prefix("async").and_then(|rest| {
        rest.chars()
            .next()
            .filter(|ch| ch.is_whitespace())
            .map(|_| rest.trim_start())
    }) {
        return script_starts_with_word(after_async, "function")
            || script_assignment_is_arrow_function(after_async);
    }
    script_assignment_is_arrow_function(assigned)
}

fn script_assignment_is_arrow_function(assigned: &str) -> bool {
    let Some(arrow) = assigned.find("=>") else {
        return false;
    };
    let params = assigned[..arrow].trim();
    params.starts_with('(')
        || params.starts_with('<')
        || parse_identifier(params).is_some_and(|name| name.len() == params.len())
}

fn script_starts_with_word(text: &str, word: &str) -> bool {
    text.strip_prefix(word).is_some_and(|rest| {
        rest.chars()
            .next()
            .is_none_or(|ch| !is_identifier_continue(ch))
    })
}

fn go_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") {
        return None;
    }

    if let Some(after) = trimmed.strip_prefix("func ") {
        let after = skip_go_receiver(after).unwrap_or(after);
        let name = parse_identifier(after)?;
        let column = line.rfind(name)?;
        return Some((name.to_owned(), ProjectSymbolKind::Function, column));
    }

    if let Some(after) = trimmed.strip_prefix("type ") {
        let name = parse_identifier(after)?;
        let rest = after[name.len()..].trim_start();
        let kind = if rest.starts_with("struct") {
            ProjectSymbolKind::Struct
        } else if rest.starts_with("interface") {
            ProjectSymbolKind::Interface
        } else {
            ProjectSymbolKind::Type
        };
        let column = line.find(name)?;
        return Some((name.to_owned(), kind, column));
    }

    for (prefix, kind) in [
        ("const ", ProjectSymbolKind::Constant),
        ("var ", ProjectSymbolKind::Variable),
    ] {
        if let Some(after) = trimmed.strip_prefix(prefix) {
            let name = parse_identifier(after)?;
            let column = line.find(name)?;
            return Some((name.to_owned(), kind, column));
        }
    }

    None
}

fn skip_go_receiver(text: &str) -> Option<&str> {
    let text = text.trim_start();
    if !text.starts_with('(') {
        return None;
    }

    let receiver_end = text.find(')')?;
    Some(text[receiver_end + 1..].trim_start())
}

fn java_family_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('@') {
        return None;
    }
    let rest = strip_prefix_words(
        trimmed,
        &[
            "public",
            "private",
            "protected",
            "internal",
            "static",
            "final",
            "abstract",
            "async",
            "unsafe",
            "sealed",
            "partial",
            "extern",
            "virtual",
            "override",
            "readonly",
        ],
    );
    let candidates = [
        ("class ", ProjectSymbolKind::Class),
        ("interface ", ProjectSymbolKind::Interface),
        ("enum ", ProjectSymbolKind::Enum),
        ("struct ", ProjectSymbolKind::Struct),
        ("record ", ProjectSymbolKind::Class),
    ];
    if let Some(symbol) = candidates.iter().find_map(|(prefix, kind)| {
        rest.strip_prefix(prefix).and_then(|after| {
            let name = parse_identifier(after)?;
            let column = line.find(name)?;
            Some((name.to_owned(), *kind, column))
        })
    }) {
        return Some(symbol);
    }

    function_symbol_before_paren(line, rest)
}

fn c_family_symbol_from_line(line: &str) -> Option<(String, ProjectSymbolKind, usize)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with("//") || trimmed.starts_with('#') {
        return None;
    }
    let rest = strip_prefix_words(
        trimmed,
        &[
            "static",
            "inline",
            "extern",
            "constexpr",
            "virtual",
            "template",
        ],
    );
    let candidates = [
        ("class ", ProjectSymbolKind::Class),
        ("struct ", ProjectSymbolKind::Struct),
        ("enum ", ProjectSymbolKind::Enum),
        ("typedef ", ProjectSymbolKind::Type),
    ];
    if let Some(symbol) = candidates.iter().find_map(|(prefix, kind)| {
        rest.strip_prefix(prefix).and_then(|after| {
            let name = parse_identifier(after)?;
            let column = line.find(name)?;
            Some((name.to_owned(), *kind, column))
        })
    }) {
        return Some(symbol);
    }

    function_symbol_before_paren(line, rest)
}

fn function_symbol_before_paren(
    line: &str,
    signature: &str,
) -> Option<(String, ProjectSymbolKind, usize)> {
    let paren = signature.find('(')?;
    let before = signature[..paren].trim_end();
    let name = before
        .rsplit(|ch: char| !(ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()))
        .find(|part| !part.is_empty())?;
    if matches!(
        name,
        "if" | "for" | "while" | "switch" | "catch" | "return" | "sizeof"
    ) {
        return None;
    }
    let column = line.rfind(name)?;
    Some((name.to_owned(), ProjectSymbolKind::Function, column))
}

fn strip_prefix_words<'a>(mut text: &'a str, words: &[&str]) -> &'a str {
    loop {
        let original = text;
        for word in words {
            if let Some(rest) = text.strip_prefix(word).and_then(|rest| {
                rest.chars()
                    .next()
                    .filter(|ch| ch.is_whitespace())
                    .map(|_| rest.trim_start())
            }) {
                text = rest;
                break;
            }
        }
        if text == original {
            return text;
        }
    }
}

fn parse_identifier(text: &str) -> Option<&str> {
    let text = text.trim_start();
    let mut chars = text.char_indices();
    let (_, first) = chars.next()?;
    if !is_identifier_start(first) {
        return None;
    }
    let mut end = first.len_utf8();
    for (idx, ch) in chars {
        if is_identifier_continue(ch) {
            end = idx + ch.len_utf8();
        } else {
            break;
        }
    }
    Some(&text[..end])
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}
