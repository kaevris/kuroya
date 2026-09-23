use crate::text_match::AsciiCaseInsensitiveMatcher;
use std::{
    borrow::Cow,
    cmp::Ordering,
    collections::{BinaryHeap, HashSet},
    path::Path,
    sync::Arc,
};

use super::{
    MAX_PROJECT_SYMBOL_QUERY_CHARS, MAX_PROJECT_SYMBOL_QUERY_TERMS, ProjectSymbol,
    ProjectSymbolKind,
};

#[derive(Debug, Clone, Copy)]
struct ProjectSymbolMatch<'a> {
    score: i32,
    symbol: &'a ProjectSymbol,
}

#[derive(Debug, Clone)]
pub(super) struct ProjectSymbolQueryTerm<'a> {
    pub(super) text: &'a str,
    pub(super) path_text: Cow<'a, str>,
    matcher: AsciiCaseInsensitiveMatcher<'a>,
}

#[derive(Debug, Clone)]
pub(super) enum ProjectSymbolQuery<'a> {
    Single(ProjectSymbolQueryTerm<'a>),
    Pair([ProjectSymbolQueryTerm<'a>; 2]),
    Many(Vec<ProjectSymbolQueryTerm<'a>>),
}

impl PartialEq for ProjectSymbolMatch<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl Eq for ProjectSymbolMatch<'_> {}

impl PartialOrd for ProjectSymbolMatch<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ProjectSymbolMatch<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        let left = (self.score, self.symbol);
        let right = (other.score, other.symbol);
        project_symbol_match_cmp(&left, &right)
    }
}

impl<'a> ProjectSymbolQueryTerm<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            text,
            path_text: project_symbol_search_text(Cow::Borrowed(text)),
            matcher: AsciiCaseInsensitiveMatcher::new(text),
        }
    }
}

impl<'a> ProjectSymbolQuery<'a> {
    pub(super) fn new(query: &'a str) -> Option<Self> {
        let query = query.trim();
        if query.is_empty()
            || query
                .chars()
                .take(MAX_PROJECT_SYMBOL_QUERY_CHARS + 1)
                .count()
                > MAX_PROJECT_SYMBOL_QUERY_CHARS
        {
            return None;
        }

        let mut terms = query.split_ascii_whitespace();
        let first = terms.next()?;
        let Some(second) = terms.next() else {
            return Some(Self::Single(ProjectSymbolQueryTerm::new(first)));
        };
        let Some(third) = terms.next() else {
            return Some(Self::Pair([
                ProjectSymbolQueryTerm::new(first),
                ProjectSymbolQueryTerm::new(second),
            ]));
        };

        let mut prepared_terms = Vec::with_capacity(4);
        prepared_terms.push(ProjectSymbolQueryTerm::new(first));
        prepared_terms.push(ProjectSymbolQueryTerm::new(second));
        prepared_terms.push(ProjectSymbolQueryTerm::new(third));
        for term in terms {
            if prepared_terms.len() >= MAX_PROJECT_SYMBOL_QUERY_TERMS {
                return None;
            }
            prepared_terms.push(ProjectSymbolQueryTerm::new(term));
        }
        Some(Self::Many(prepared_terms))
    }

    fn score(&self, symbol: &ProjectSymbol, path_text: Option<&str>) -> Option<i32> {
        match self {
            Self::Single(term) => project_symbol_match_score_single(symbol, path_text, term),
            Self::Pair(terms) => project_symbol_match_score(symbol, path_text, terms),
            Self::Many(terms) => project_symbol_match_score(symbol, path_text, terms),
        }
    }
}

pub(super) fn workspace_symbols(
    symbols: &[ProjectSymbol],
    symbol_search_paths: &[Arc<str>],
    query: &str,
    limit: usize,
) -> Vec<ProjectSymbol> {
    let query = query.trim();
    if query.is_empty() || limit == 0 {
        return Vec::new();
    }

    let Some(query) = ProjectSymbolQuery::new(query) else {
        return Vec::new();
    };

    let symbol_search_paths = if symbol_search_paths.len() == symbols.len() {
        Some(symbol_search_paths)
    } else {
        None
    };
    let mut matches = BinaryHeap::with_capacity(limit.min(symbols.len()));
    for (index, symbol) in symbols.iter().enumerate() {
        let path_text = symbol_search_paths
            .and_then(|paths| paths.get(index))
            .map(|path| path.as_ref());
        let Some(score) = query.score(symbol, path_text) else {
            continue;
        };
        let candidate = ProjectSymbolMatch { score, symbol };
        if matches.len() < limit {
            matches.push(candidate);
            continue;
        }

        if matches
            .peek()
            .is_some_and(|worst| candidate.cmp(worst).is_lt())
        {
            matches.pop();
            matches.push(candidate);
        }
    }

    let mut matches = matches.into_vec();
    matches.sort_unstable();
    let mut ranked_symbols = Vec::with_capacity(matches.len());
    ranked_symbols.extend(
        matches
            .into_iter()
            .map(|symbol_match| symbol_match.symbol.clone()),
    );
    ranked_symbols
}

fn project_symbol_match_cmp(
    left: &(i32, &ProjectSymbol),
    right: &(i32, &ProjectSymbol),
) -> Ordering {
    let (left_score, left_symbol) = left;
    let (right_score, right_symbol) = right;
    right_score
        .cmp(left_score)
        .then(left_symbol.name.cmp(&right_symbol.name))
        .then(left_symbol.relative_path.cmp(&right_symbol.relative_path))
        .then(left_symbol.line.cmp(&right_symbol.line))
        .then(left_symbol.column.cmp(&right_symbol.column))
        .then(
            left_symbol
                .kind
                .lsp_kind()
                .cmp(&right_symbol.kind.lsp_kind()),
        )
        .then(left_symbol.path.cmp(&right_symbol.path))
}

fn project_symbol_match_score(
    symbol: &ProjectSymbol,
    path_text: Option<&str>,
    terms: &[ProjectSymbolQueryTerm<'_>],
) -> Option<i32> {
    let mut score = 0;
    let mut computed_path_text = None::<Cow<'_, str>>;
    for term in terms {
        score += project_symbol_term_score(symbol, path_text, term, &mut computed_path_text)?;
    }
    Some(score + project_symbol_kind_score(symbol.kind))
}

fn project_symbol_match_score_single(
    symbol: &ProjectSymbol,
    path_text: Option<&str>,
    term: &ProjectSymbolQueryTerm<'_>,
) -> Option<i32> {
    let mut computed_path_text = None::<Cow<'_, str>>;
    let score = project_symbol_term_score(symbol, path_text, term, &mut computed_path_text)?;
    Some(score + project_symbol_kind_score(symbol.kind))
}

fn project_symbol_term_score<'a>(
    symbol: &'a ProjectSymbol,
    path_text: Option<&'a str>,
    term: &ProjectSymbolQueryTerm<'_>,
    computed_path_text: &mut Option<Cow<'a, str>>,
) -> Option<i32> {
    if symbol.name.eq_ignore_ascii_case(term.text) {
        Some(100)
    } else if let Some(start) = term.matcher.find_from(&symbol.name, 0) {
        Some(if start == 0 { 80 } else { 50 })
    } else if let Some(subsequence_score) =
        project_symbol_subsequence_score(&symbol.name, term.text)
    {
        Some(subsequence_score)
    } else {
        let path_text = match path_text {
            Some(path_text) => path_text,
            None => {
                if computed_path_text.is_none() {
                    *computed_path_text =
                        Some(project_symbol_search_path_text(&symbol.relative_path));
                }
                computed_path_text.as_deref().unwrap_or_default()
            }
        };
        path_text.contains(term.path_text.as_ref()).then_some(20)
    }
}

fn project_symbol_subsequence_score(name: &str, query: &str) -> Option<i32> {
    let mut query_chars = query.chars().peekable();
    let mut all_on_word_starts = true;
    let mut previous_was_separator = true;
    for (index, name_char) in name.chars().enumerate() {
        let Some(query_char) = query_chars.peek() else {
            break;
        };
        let matches_query = name_char.eq_ignore_ascii_case(query_char);
        if matches_query {
            if index > 0 && !previous_was_separator && !name_char.is_ascii_uppercase() {
                all_on_word_starts = false;
            }
            query_chars.next();
        }
        previous_was_separator = matches!(name_char, '_' | '-' | ' ' | '.');
    }
    if query_chars.peek().is_some() {
        return None;
    }
    Some(if all_on_word_starts { 45 } else { 30 })
}

pub(super) fn project_symbol_search_paths(symbols: &[ProjectSymbol]) -> Vec<Arc<str>> {
    let mut paths = Vec::with_capacity(symbols.len());
    let mut cached_paths = HashSet::<Arc<str>>::with_capacity(symbols.len().min(1024));
    let mut last_relative_path = None::<&Path>;
    let mut last_search_path = None::<Arc<str>>;
    for symbol in symbols {
        let relative_path = symbol.relative_path.as_path();
        if let (Some(last_relative_path), Some(last_search_path)) =
            (last_relative_path, last_search_path.as_ref())
            && last_relative_path == relative_path
        {
            paths.push(Arc::clone(last_search_path));
            continue;
        }

        let search_path = project_symbol_search_path(relative_path);
        let search_path = match cached_paths.get(search_path.as_ref()) {
            Some(cached_path) => Arc::clone(cached_path),
            None => {
                cached_paths.insert(Arc::clone(&search_path));
                search_path
            }
        };
        last_relative_path = Some(relative_path);
        last_search_path = Some(Arc::clone(&search_path));
        paths.push(search_path);
    }
    paths
}

pub(super) fn project_symbol_search_path(relative_path: &Path) -> Arc<str> {
    match project_symbol_search_path_text(relative_path) {
        Cow::Borrowed(path) => Arc::from(path),
        Cow::Owned(path) => Arc::from(path.into_boxed_str()),
    }
}

fn project_symbol_search_path_text(relative_path: &Path) -> Cow<'_, str> {
    project_symbol_search_text(relative_path.to_string_lossy())
}

fn project_symbol_search_text(text: Cow<'_, str>) -> Cow<'_, str> {
    let mut has_uppercase = false;
    let mut has_windows_separator = false;
    for byte in text.as_ref().bytes() {
        has_uppercase |= byte.is_ascii_uppercase();
        has_windows_separator |= byte == b'\\';
        if has_uppercase && has_windows_separator {
            break;
        }
    }
    if !has_uppercase && !has_windows_separator {
        return text;
    }

    match text {
        Cow::Borrowed(text) => {
            let mut text = if has_windows_separator {
                text.replace('\\', "/")
            } else {
                text.to_owned()
            };
            if has_uppercase {
                text.make_ascii_lowercase();
            }
            Cow::Owned(text)
        }
        Cow::Owned(mut text) => {
            if has_windows_separator {
                text = text.replace('\\', "/");
            }
            if has_uppercase {
                text.make_ascii_lowercase();
            }
            Cow::Owned(text)
        }
    }
}

fn project_symbol_kind_score(kind: ProjectSymbolKind) -> i32 {
    match kind {
        ProjectSymbolKind::Function | ProjectSymbolKind::Class | ProjectSymbolKind::Struct => 8,
        ProjectSymbolKind::Enum | ProjectSymbolKind::Interface | ProjectSymbolKind::Type => 6,
        ProjectSymbolKind::Module => 4,
        ProjectSymbolKind::Constant | ProjectSymbolKind::Variable => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::workspace_symbols;
    use crate::{ProjectSymbol, ProjectSymbolKind};
    use std::{path::PathBuf, sync::Arc};

    fn symbol(name: &str, relative_path: &str) -> ProjectSymbol {
        ProjectSymbol {
            name: name.to_owned(),
            kind: ProjectSymbolKind::Function,
            path: PathBuf::from(relative_path),
            relative_path: PathBuf::from(relative_path),
            line: 1,
            column: 1,
        }
    }

    fn names(results: &[ProjectSymbol]) -> Vec<&str> {
        results.iter().map(|symbol| symbol.name.as_str()).collect()
    }

    #[test]
    fn fuzzy_query_matches_across_word_boundaries() {
        let symbols = vec![
            symbol("LiveIndexProbe", "src/live.rs"),
            symbol("unrelated", "src/other.rs"),
        ];
        let paths = vec![Arc::from("src/live.rs"), Arc::from("src/other.rs")];

        let results = workspace_symbols(&symbols, &paths, "liveindex", 10);

        assert_eq!(names(&results), vec!["LiveIndexProbe"]);
    }

    #[test]
    fn word_start_fuzzy_ranks_above_plain_subsequence() {
        let symbols = vec![
            symbol("sLoopImp", "src/a.rs"),
            symbol("LiveIndexProbe", "src/b.rs"),
        ];
        let paths = vec![Arc::from("src/a.rs"), Arc::from("src/b.rs")];

        let results = workspace_symbols(&symbols, &paths, "lip", 10);

        assert_eq!(names(&results), vec!["LiveIndexProbe", "sLoopImp"]);
    }

    #[test]
    fn substring_matches_still_rank_above_fuzzy() {
        let symbols = vec![
            symbol("LiveIndexProbe", "src/a.rs"),
            symbol("IndexBar", "src/b.rs"),
        ];
        let paths = vec![Arc::from("src/a.rs"), Arc::from("src/b.rs")];

        let results = workspace_symbols(&symbols, &paths, "index", 10);

        assert_eq!(names(&results), vec!["IndexBar", "LiveIndexProbe"]);
    }

    #[test]
    fn impossible_queries_still_return_nothing() {
        let symbols = vec![symbol("LiveIndexProbe", "src/a.rs")];
        let paths = vec![Arc::from("src/a.rs")];

        assert!(workspace_symbols(&symbols, &paths, "zzz", 10).is_empty());

        assert!(workspace_symbols(&symbols, &paths, "xedit", 10).is_empty());
    }
}
