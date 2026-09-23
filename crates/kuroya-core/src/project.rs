#[cfg(test)]
use crate::text_match::ascii_case_insensitive_contains as contains_ascii_case_insensitive;
#[cfg(test)]
use crate::text_match::ascii_case_insensitive_starts_with as starts_with_ascii_case_insensitive;
use crate::workspace_paths::{normalize_child_path, path_starts_with_lexically};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    borrow::Cow,
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

mod symbol_search;
mod symbols;
use symbols::RUST_AST_PARSE_BUDGET_BYTES;

#[cfg(test)]
use symbol_search::{ProjectSymbolQuery, project_symbol_search_path};
use symbol_search::{project_symbol_search_paths, workspace_symbols};
use symbols::extract_project_symbols;
#[cfg(test)]
use symbols::read_symbol_text_with_limit;

const MAX_PROJECT_SYMBOLS: usize = 20_000;
const MAX_SYMBOLS_PER_FILE: usize = 128;
const MAX_SYMBOL_FILE_BYTES: u64 = 512 * 1024;
const MAX_SYMBOL_LINE_BYTES: usize = 8 * 1024;
const MAX_PROJECT_SYMBOL_QUERY_CHARS: usize = 512;
const MAX_PROJECT_SYMBOL_QUERY_TERMS: usize = 32;

pub const PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT: usize = 5_000;

pub const PROJECT_INDEX_SYMBOL_POLICY_VERSION: u32 = 2;

pub const PROJECT_INDEX_SYMBOL_SCAN_BUDGET_BYTES: u64 = 32 * 1024 * 1024;

pub const DEFAULT_PROJECT_INDEX_MAX_FILES: usize = 150_000;
pub const MIN_PROJECT_INDEX_MAX_FILES: usize = 1_000;
pub const MAX_PROJECT_INDEX_MAX_FILES: usize = 1_000_000;
const PROJECT_INDEX_MAX_GLOB_PATTERNS: usize = 1024;
const PROJECT_INDEX_MAX_GLOB_PATTERN_BYTES: usize = 4096;
const PROJECT_INDEX_PROTECTED_WORKSPACE_DIRS: &[&str] = &[".git", ".kuroya"];
const PROJECT_INDEX_INDEXED_HIDDEN_DIRS: &[&str] = &[".github", ".gitlab", ".config", ".vscode"];
const DEFAULT_PROJECT_INDEX_EXCLUDE_GLOBS: &[&str] = &[
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    ".next",
    "out",
    ".cache",
    ".turbo",
    ".parcel-cache",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".gradle",
    ".terraform",
    ".venv",
    "venv",
    "__pycache__",
    ".pnpm-store",
];

pub fn default_project_index_exclude_globs() -> Vec<String> {
    DEFAULT_PROJECT_INDEX_EXCLUDE_GLOBS
        .iter()
        .map(|glob| (*glob).to_owned())
        .collect()
}

pub fn merged_exclude_globs(user_globs: &[String]) -> Vec<String> {
    let mut merged =
        Vec::with_capacity(DEFAULT_PROJECT_INDEX_EXCLUDE_GLOBS.len() + user_globs.len());
    merged.extend(default_project_index_exclude_globs());
    for glob in user_globs {
        let glob = glob.trim();
        if glob.is_empty() || merged.iter().any(|existing| existing == glob) {
            continue;
        }
        merged.push(glob.to_owned());
    }
    merged
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub root: PathBuf,
    pub opened_at: SystemTime,
}

impl Workspace {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            opened_at: SystemTime::now(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,

    #[serde(default)]
    pub len: u64,

    #[serde(default)]
    pub modified_millis: u64,

    #[serde(default)]
    pub created_millis: u64,
}

impl ProjectEntry {
    pub fn from_metadata_parts(
        path: PathBuf,
        relative_path: PathBuf,
        is_dir: bool,
        depth: usize,
        metadata: Option<&fs::Metadata>,
    ) -> Self {
        let (len, modified_millis, created_millis) = if is_dir {
            (0, 0, 0)
        } else {
            match metadata {
                Some(metadata) => (
                    metadata.len(),
                    metadata_modified_millis(metadata),
                    metadata_created_millis(metadata),
                ),
                None => (0, 0, 0),
            }
        };
        Self {
            path,
            relative_path,
            is_dir,
            depth,
            len,
            modified_millis,
            created_millis,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectSymbolKind {
    Module,
    Class,
    Function,
    Variable,
    Constant,
    Enum,
    Interface,
    Struct,
    Type,
}

impl ProjectSymbolKind {
    pub fn lsp_kind(self) -> u8 {
        match self {
            Self::Module => 2,
            Self::Class => 5,
            Self::Function => 12,
            Self::Variable => 13,
            Self::Constant => 14,
            Self::Enum => 10,
            Self::Interface => 11,
            Self::Struct => 23,
            Self::Type => 26,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSymbol {
    pub name: String,
    pub kind: ProjectSymbolKind,
    pub path: PathBuf,
    pub relative_path: PathBuf,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIndexSignature {
    pub max_files: usize,
    #[serde(default)]
    pub options_fingerprint: u64,
    pub file_count: usize,
    pub entry_count: usize,
    pub truncated: bool,
    #[serde(default)]
    pub symbol_policy_version: u32,
    pub fingerprint: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIndexOptions {
    pub max_files: usize,
    pub exclude_globs: Vec<String>,
    pub excluded_paths: Vec<PathBuf>,

    pub include_hidden_dirs: bool,
}

impl ProjectIndexOptions {
    pub fn new(max_files: usize) -> Self {
        Self {
            max_files,
            exclude_globs: default_project_index_exclude_globs(),
            excluded_paths: Vec::new(),
            include_hidden_dirs: false,
        }
    }

    pub fn with_exclude_globs(max_files: usize, exclude_globs: Vec<String>) -> Self {
        Self {
            max_files,
            exclude_globs,
            excluded_paths: Vec::new(),
            include_hidden_dirs: false,
        }
    }

    pub fn with_excluded_path(mut self, path: PathBuf) -> Self {
        self.excluded_paths.push(path);
        self
    }

    pub fn with_include_hidden_dirs(mut self, include_hidden_dirs: bool) -> Self {
        self.include_hidden_dirs = include_hidden_dirs;
        self
    }

    pub fn options_fingerprint(&self) -> u64 {
        project_index_options_fingerprint(
            &self.exclude_globs,
            &self.excluded_paths,
            self.include_hidden_dirs,
        )
    }

    pub fn path_filter(&self, root: &Path) -> ProjectIndexPathFilter {
        ProjectIndexPathFilter::new(root, &self.exclude_globs, &self.excluded_paths)
    }
}

impl Default for ProjectIndexOptions {
    fn default() -> Self {
        Self::new(DEFAULT_PROJECT_INDEX_MAX_FILES)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProjectIndex {
    data: Arc<ProjectIndexData>,
}

#[derive(Debug, Default, Clone, Serialize)]
struct ProjectIndexData {
    root: PathBuf,
    files: Vec<PathBuf>,
    entries: Vec<ProjectEntry>,
    symbols: Vec<ProjectSymbol>,
    #[serde(skip)]
    symbol_search_paths: Vec<Arc<str>>,
    truncated: bool,

    #[serde(default)]
    max_files: usize,

    #[serde(default)]
    include_hidden_dirs: bool,

    #[serde(default)]
    symbol_budget_used: u64,

    #[serde(skip)]
    path_filter: Option<ProjectIndexPathFilter>,
}

#[derive(Deserialize)]
struct ProjectIndexSerde {
    root: PathBuf,
    files: Vec<PathBuf>,
    entries: Vec<ProjectEntry>,
    symbols: Vec<ProjectSymbol>,
    truncated: bool,
    #[serde(default)]
    max_files: usize,
    #[serde(default)]
    include_hidden_dirs: bool,
    #[serde(default)]
    symbol_budget_used: u64,
}

impl<'de> Deserialize<'de> for ProjectIndex {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ProjectIndexSerde {
            root,
            files,
            entries,
            symbols,
            truncated,
            max_files,
            include_hidden_dirs,
            symbol_budget_used,
        } = ProjectIndexSerde::deserialize(deserializer)?;
        let symbol_search_paths = project_symbol_search_paths(&symbols);
        Ok(Self {
            data: Arc::new(ProjectIndexData {
                root,
                files,
                entries,
                symbols,
                symbol_search_paths,
                truncated,
                max_files,
                include_hidden_dirs,
                symbol_budget_used,
                path_filter: None,
            }),
        })
    }
}

impl Serialize for ProjectIndex {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.data.serialize(serializer)
    }
}

impl ProjectIndex {
    fn from_data(data: ProjectIndexData) -> Self {
        Self {
            data: Arc::new(data),
        }
    }

    #[cfg(test)]
    fn from_parts_for_test(
        root: PathBuf,
        files: Vec<PathBuf>,
        entries: Vec<ProjectEntry>,
        symbols: Vec<ProjectSymbol>,
        symbol_search_paths: Vec<Arc<str>>,
        truncated: bool,
    ) -> Self {
        Self::from_data(ProjectIndexData {
            root,
            files,
            entries,
            symbol_search_paths,
            symbols,
            truncated,
            max_files: 0,
            include_hidden_dirs: false,
            symbol_budget_used: 0,
            path_filter: None,
        })
    }

    pub fn rebuild(root: &Path, max_files: usize) -> Self {
        Self::rebuild_with_options(root, &ProjectIndexOptions::new(max_files))
    }

    pub fn rebuild_with_signature(root: &Path, max_files: usize) -> (Self, ProjectIndexSignature) {
        Self::rebuild_with_signature_options(root, &ProjectIndexOptions::new(max_files))
    }

    pub fn rebuild_with_options(root: &Path, options: &ProjectIndexOptions) -> Self {
        Self::rebuild_with_signature_options(root, options).0
    }

    pub fn rebuild_with_signature_options(
        root: &Path,
        options: &ProjectIndexOptions,
    ) -> (Self, ProjectIndexSignature) {
        Self::rebuild_with_signature_inner(root, options, true)
    }

    fn rebuild_with_signature_inner(
        root: &Path,
        options: &ProjectIndexOptions,
        extract_symbols_requested: bool,
    ) -> (Self, ProjectIndexSignature) {
        let max_files = options.max_files;
        let mut entries = Vec::with_capacity(project_index_initial_entry_capacity(max_files));
        let mut file_count = 0usize;
        let mut symbols = Vec::with_capacity(
            max_files
                .min(PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT)
                .saturating_mul(MAX_SYMBOLS_PER_FILE)
                .min(MAX_PROJECT_SYMBOLS),
        );
        let mut truncated = false;
        let path_filter = options.path_filter(root);
        let include_hidden_dirs = options.include_hidden_dirs;
        let walker_filter = path_filter.clone();
        let walker = ignore::WalkBuilder::new(root)
            .hidden(false)
            .git_ignore(true)
            .git_exclude(true)
            .parents(true)
            .filter_entry(move |entry| {
                project_index_entry_is_not_pruned(entry, &walker_filter, include_hidden_dirs)
            })
            .build();

        for entry in walker.flatten() {
            let path = entry.path();
            if path == root {
                continue;
            }

            let Some(file_type) = entry.file_type() else {
                continue;
            };
            let is_dir = file_type.is_dir();
            let is_file = file_type.is_file();

            if is_file && file_count >= max_files {
                truncated = true;
                break;
            }
            if !(is_dir || is_file) {
                continue;
            }

            let path = path.to_path_buf();
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let metadata = if is_file { entry.metadata().ok() } else { None };

            if is_file {
                file_count = file_count.saturating_add(1);
            }

            let depth = relative_path.components().count().saturating_sub(1);
            entries.push(ProjectEntry::from_metadata_parts(
                path,
                relative_path,
                is_dir,
                depth,
                metadata.as_ref(),
            ));
        }

        entries.sort_unstable_by(|a, b| {
            project_index_entry_sort_cmp(&a.relative_path, a.is_dir, &b.relative_path, b.is_dir)
        });

        let mut symbol_budget_used = 0u64;
        if extract_symbols_requested {
            let (extracted, used) = collect_project_symbols(
                &entries,
                file_count,
                MAX_PROJECT_SYMBOLS,
                PROJECT_INDEX_SYMBOL_SCAN_BUDGET_BYTES,
            );
            symbols = extracted;
            symbol_budget_used = used;
        }
        let signature = ProjectIndexSignature::from_stored_entries(options, truncated, &entries);
        let files = entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.clone())
            .collect();
        let index = Self::from_data(ProjectIndexData {
            root: root.to_path_buf(),
            files,
            entries,
            symbol_search_paths: project_symbol_search_paths(&symbols),
            symbols,
            truncated,
            max_files,
            include_hidden_dirs,
            symbol_budget_used,
            path_filter: Some(path_filter),
        });
        (index, signature)
    }

    pub fn root(&self) -> &Path {
        &self.data.root
    }

    pub fn is_warm(&self) -> bool {
        !self.data.root.as_os_str().is_empty()
    }

    pub fn max_files(&self) -> usize {
        self.data.max_files
    }

    pub fn clone_for_update(&self) -> Self {
        Self::from_data((*self.data).clone())
    }

    pub fn signature_from_entries(&self, options: &ProjectIndexOptions) -> ProjectIndexSignature {
        ProjectIndexSignature::from_stored_entries(options, self.data.truncated, &self.data.entries)
    }

    pub fn apply_path_changes(&mut self, root: &Path, changed: &[PathBuf]) -> bool {
        if !self.is_warm() || root.as_os_str() != self.data.root.as_os_str() {
            return false;
        }
        let data = Arc::make_mut(&mut self.data);
        let include_hidden_dirs = data.include_hidden_dirs;
        let mut file_count = data.files.len();
        let mut symbols_changed = false;
        let mut needs_sort = false;
        let mut truncated = data.truncated;
        let mut changed_any = false;
        let mut budget_remaining = if file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT {
            PROJECT_INDEX_SYMBOL_SCAN_BUDGET_BYTES.saturating_sub(data.symbol_budget_used)
        } else {
            0
        };

        for raw_path in changed {
            let Some(path) = normalize_child_path(&data.root, raw_path) else {
                continue;
            };

            if path.as_os_str() == data.root.as_os_str() {
                continue;
            }
            let Ok(relative_path) = path.strip_prefix(&data.root) else {
                continue;
            };

            let metadata = fs::metadata(&path).ok();
            let is_dir = metadata.as_ref().is_some_and(fs::Metadata::is_dir);
            let is_file = metadata.as_ref().is_some_and(fs::Metadata::is_file);
            if project_index_path_is_ignored_by_policy(
                relative_path,
                is_dir,
                include_hidden_dirs,
                data.path_filter.as_ref(),
                &path,
            ) {
                continue;
            }

            if !is_dir && !is_file {
                let removed = project_index_remove_subtree(&mut data.entries, &path);
                if !removed.is_empty() {
                    file_count = file_count
                        .saturating_sub(removed.iter().filter(|entry| !entry.is_dir).count());
                    symbols_changed |=
                        project_index_retain_symbols_outside(&mut data.symbols, &path);
                    changed_any = true;
                }
                continue;
            }

            if is_dir {
                let removed = project_index_remove_subtree(&mut data.entries, &path);
                let removed_files = removed.iter().filter(|entry| !entry.is_dir).count();
                file_count = file_count.saturating_sub(removed_files);
                symbols_changed |= project_index_retain_symbols_outside(&mut data.symbols, &path);
                changed_any |= !removed.is_empty();
                let (added, hit_cap) = project_index_scan_subtree(
                    &data.root,
                    &path,
                    include_hidden_dirs,
                    data.path_filter.as_ref(),
                    max_files_remaining(file_count, data.max_files),
                    &mut file_count,
                );
                if !added.is_empty() {
                    symbols_changed |= project_index_extract_symbols_for_new_entries(
                        &mut data.symbols,
                        &added,
                        file_count,
                        &mut budget_remaining,
                    );
                    data.entries.extend(added);
                    needs_sort = true;
                    changed_any = true;
                }
                truncated |= hit_cap;
                continue;
            }

            if is_file {
                let depth = relative_path.components().count().saturating_sub(1);
                let fresh = ProjectEntry::from_metadata_parts(
                    path.clone(),
                    relative_path.to_path_buf(),
                    false,
                    depth,
                    metadata.as_ref(),
                );
                if let Some(existing) = project_index_find_file_entry(&data.entries, relative_path)
                    && existing == &fresh
                {
                    continue;
                }
                let removed = project_index_remove_subtree(&mut data.entries, &path);
                file_count =
                    file_count.saturating_sub(removed.iter().filter(|entry| !entry.is_dir).count());
                symbols_changed |= project_index_retain_symbols_outside(&mut data.symbols, &path);

                if data.max_files > 0 && file_count >= data.max_files {
                    truncated = true;
                    changed_any = true;
                    continue;
                }
                let position = data.entries.binary_search_by(|entry| {
                    project_index_entry_sort_cmp(
                        &entry.relative_path,
                        entry.is_dir,
                        &fresh.relative_path,
                        fresh.is_dir,
                    )
                });
                let Err(insert_at) = position else {
                    continue;
                };
                data.entries.insert(insert_at, fresh);
                file_count = file_count.saturating_add(1);
                changed_any = true;
                symbols_changed |= project_index_extract_symbols_for_new_entries(
                    &mut data.symbols,
                    &data.entries[insert_at..insert_at + 1],
                    file_count,
                    &mut budget_remaining,
                );
            }
        }

        if !changed_any {
            return false;
        }
        if needs_sort {
            data.entries.sort_unstable_by(|a, b| {
                project_index_entry_sort_cmp(&a.relative_path, a.is_dir, &b.relative_path, b.is_dir)
            });
        }
        data.files = data
            .entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.path.clone())
            .collect();

        data.truncated = truncated && file_count >= data.max_files;
        if file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT {
            data.symbol_budget_used =
                PROJECT_INDEX_SYMBOL_SCAN_BUDGET_BYTES.saturating_sub(budget_remaining);
        }
        if symbols_changed {
            data.symbol_search_paths = project_symbol_search_paths(&data.symbols);
        }
        true
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.data.files
    }

    pub fn symbols(&self) -> &[ProjectSymbol] {
        &self.data.symbols
    }

    pub fn all_entries(&self) -> &[ProjectEntry] {
        &self.data.entries
    }

    pub fn truncated(&self) -> bool {
        self.data.truncated
    }

    pub fn workspace_symbols(&self, query: &str, limit: usize) -> Vec<ProjectSymbol> {
        workspace_symbols(
            &self.data.symbols,
            &self.data.symbol_search_paths,
            query,
            limit,
        )
    }

    pub fn entries(&self, root: &Path, limit: usize) -> Vec<ProjectEntry> {
        let capacity = limit.min(self.data.entries.len());
        if root == self.data.root.as_path() {
            let mut entries = Vec::with_capacity(capacity);
            entries.extend(self.data.entries.iter().take(limit).cloned());
            return entries;
        }

        let mut entries = Vec::with_capacity(capacity);
        for entry in &self.data.entries {
            if entries.len() >= limit {
                break;
            }
            let Ok(relative_path) = entry.path.strip_prefix(root) else {
                continue;
            };
            if relative_path.as_os_str().is_empty() {
                continue;
            }
            let mut entry = entry.clone();
            entry.relative_path = relative_path.to_path_buf();
            entry.depth = entry.relative_path.components().count().saturating_sub(1);
            entries.push(entry);
        }
        entries
    }
}

fn project_index_entry_is_not_pruned(
    entry: &ignore::DirEntry,
    path_filter: &ProjectIndexPathFilter,
    include_hidden_dirs: bool,
) -> bool {
    if entry.depth() == 0 {
        return true;
    }

    if !include_hidden_dirs
        && entry
            .file_type()
            .is_some_and(|file_type| file_type.is_dir())
        && project_index_dir_is_hidden(entry.path())
    {
        return false;
    }

    !path_filter.is_excluded(entry.path())
}

fn project_index_dir_is_hidden(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name.starts_with('.')
        && !PROJECT_INDEX_INDEXED_HIDDEN_DIRS
            .iter()
            .any(|indexed| name.eq_ignore_ascii_case(indexed))
}

#[derive(Debug, Clone)]
pub struct ProjectIndexPathFilter {
    root: PathBuf,
    globs: Option<GlobSet>,
    excluded_paths: Vec<PathBuf>,
}

impl ProjectIndexPathFilter {
    fn new(root: &Path, patterns: &[String], excluded_paths: &[PathBuf]) -> Self {
        Self {
            root: root.to_path_buf(),
            globs: build_project_index_glob_set(patterns),
            excluded_paths: excluded_paths
                .iter()
                .filter_map(|path| normalize_child_path(root, path))
                .collect(),
        }
    }

    pub fn is_excluded(&self, path: &Path) -> bool {
        if self.has_protected_component(path) {
            return true;
        }
        if self
            .excluded_paths
            .iter()
            .any(|excluded| path_starts_with_lexically(path, excluded))
        {
            return true;
        }
        let Some(globs) = &self.globs else {
            return false;
        };
        let relative = self.relative_path(path);
        let file_name = path.file_name().map(Path::new);
        globs.is_match(relative.as_ref()) || file_name.is_some_and(|name| globs.is_match(name))
    }

    fn has_protected_component(&self, path: &Path) -> bool {
        self.relative_path(path).components().any(|component| {
            let std::path::Component::Normal(name) = component else {
                return false;
            };
            name.to_str().is_some_and(|name| {
                PROJECT_INDEX_PROTECTED_WORKSPACE_DIRS
                    .iter()
                    .any(|protected| name.eq_ignore_ascii_case(protected))
            })
        })
    }

    fn relative_path<'a>(&self, path: &'a Path) -> Cow<'a, Path> {
        if let Ok(relative) = path.strip_prefix(&self.root) {
            return Cow::Borrowed(relative);
        }
        if path_starts_with_lexically(path, &self.root) {
            let relative = path
                .components()
                .skip(self.root.components().count())
                .collect::<PathBuf>();
            return Cow::Owned(relative);
        }
        Cow::Borrowed(path)
    }
}

fn build_project_index_glob_set(patterns: &[String]) -> Option<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    let mut added = HashSet::with_capacity(
        patterns
            .len()
            .min(PROJECT_INDEX_MAX_GLOB_PATTERNS)
            .saturating_mul(3),
    );
    let mut has_patterns = false;
    let mut pattern_count = 0usize;

    for pattern in patterns {
        let pattern = pattern.trim();
        if pattern.is_empty() {
            continue;
        }
        pattern_count = pattern_count.saturating_add(1);
        if pattern_count > PROJECT_INDEX_MAX_GLOB_PATTERNS
            || pattern.len() > PROJECT_INDEX_MAX_GLOB_PATTERN_BYTES
        {
            continue;
        }
        if add_project_index_glob_pattern(&mut builder, &mut added, pattern).is_err() {
            continue;
        }
        has_patterns = true;
        if project_index_glob_pattern_is_bare(pattern) {
            let _ = add_project_index_bare_glob_variants(&mut builder, &mut added, pattern);
        }
    }

    if has_patterns {
        builder.build().ok()
    } else {
        None
    }
}

fn project_index_glob_pattern_is_bare(pattern: &str) -> bool {
    !pattern.contains(['/', '\\']) && !pattern.starts_with("**")
}

fn add_project_index_glob_pattern(
    builder: &mut GlobSetBuilder,
    added: &mut HashSet<String>,
    pattern: &str,
) -> Result<(), globset::Error> {
    if added.insert(pattern.to_owned()) {
        let mut glob = GlobBuilder::new(pattern);
        glob.case_insensitive(cfg!(windows));
        builder.add(glob.build()?);
    }
    Ok(())
}

fn add_project_index_bare_glob_variants(
    builder: &mut GlobSetBuilder,
    added: &mut HashSet<String>,
    pattern: &str,
) -> Result<(), globset::Error> {
    let mut descendant_pattern = String::with_capacity(pattern.len() + 6);
    descendant_pattern.push_str("**/");
    descendant_pattern.push_str(pattern);
    add_project_index_glob_pattern(builder, added, &descendant_pattern)?;

    descendant_pattern.push_str("/**");
    add_project_index_glob_pattern(builder, added, &descendant_pattern)
}

fn project_index_options_fingerprint(
    exclude_globs: &[String],
    excluded_paths: &[PathBuf],
    include_hidden_dirs: bool,
) -> u64 {
    let mut hash = FNV_OFFSET;
    for glob in exclude_globs {
        let glob = glob.trim();
        if glob.is_empty() {
            continue;
        }
        for byte in glob.as_bytes() {
            fnv_hash_u8(&mut hash, *byte);
        }
        fnv_hash_u8(&mut hash, 0);
    }
    fnv_hash_u8(&mut hash, 0xff);
    for path in excluded_paths {
        for byte in path.to_string_lossy().as_bytes() {
            fnv_hash_u8(&mut hash, *byte);
        }
        fnv_hash_u8(&mut hash, 0);
    }
    fnv_hash_u8(&mut hash, 0xfe);
    fnv_hash_u8(&mut hash, u8::from(include_hidden_dirs));
    hash
}

fn project_index_entry_sort_cmp(
    left_path: &Path,
    left_is_dir: bool,
    right_path: &Path,
    right_is_dir: bool,
) -> std::cmp::Ordering {
    left_path
        .cmp(right_path)
        .then(left_is_dir.cmp(&right_is_dir).reverse())
}

fn project_index_initial_entry_capacity(max_files: usize) -> usize {
    max_files.min(DEFAULT_PROJECT_INDEX_MAX_FILES)
}

impl ProjectIndexSignature {
    fn from_stored_entries(
        options: &ProjectIndexOptions,
        truncated: bool,
        entries: &[ProjectEntry],
    ) -> Self {
        let options_fingerprint = options.options_fingerprint();
        let file_count = entries.iter().filter(|entry| !entry.is_dir).count();
        Self {
            max_files: options.max_files,
            options_fingerprint,
            file_count,
            entry_count: entries.len(),
            truncated,
            symbol_policy_version: PROJECT_INDEX_SYMBOL_POLICY_VERSION,
            fingerprint: project_index_signature_fingerprint(
                options.max_files,
                options_fingerprint,
                truncated,
                entries,
            ),
        }
    }

    pub fn matches_options(&self, options: &ProjectIndexOptions) -> bool {
        self.max_files == options.max_files
            && self.options_fingerprint == options.options_fingerprint()
            && self.symbol_policy_version == PROJECT_INDEX_SYMBOL_POLICY_VERSION
    }
}

fn project_index_signature_fingerprint(
    max_files: usize,
    options_fingerprint: u64,
    truncated: bool,
    entries: &[ProjectEntry],
) -> u64 {
    let mut hash = FNV_OFFSET;
    fnv_hash_u64(&mut hash, max_files as u64);
    fnv_hash_u64(&mut hash, options_fingerprint);
    fnv_hash_u8(&mut hash, u8::from(truncated));
    for entry in entries {
        fnv_hash_path(&mut hash, &entry.relative_path);
        fnv_hash_u8(&mut hash, u8::from(entry.is_dir));
        fnv_hash_u64(&mut hash, entry.len);
        fnv_hash_u64(&mut hash, entry.modified_millis);
        fnv_hash_u64(&mut hash, entry.created_millis);
    }
    hash
}

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

fn fnv_hash_path(hash: &mut u64, path: &Path) {
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            for byte in name.to_string_lossy().as_bytes() {
                fnv_hash_u8(hash, *byte);
            }
            fnv_hash_u8(hash, b'/');
        }
    }
}

fn fnv_hash_u8(hash: &mut u64, value: u8) {
    *hash ^= u64::from(value);
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn fnv_hash_u64(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        fnv_hash_u8(hash, byte);
    }
}

fn metadata_modified_millis(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn metadata_created_millis(metadata: &fs::Metadata) -> u64 {
    metadata
        .created()
        .ok()
        .and_then(|created| created.duration_since(UNIX_EPOCH).ok())
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

fn max_files_remaining(file_count: usize, max_files: usize) -> usize {
    if max_files == 0 {
        usize::MAX
    } else {
        max_files.saturating_sub(file_count)
    }
}

fn project_index_path_is_ignored_by_policy(
    relative_path: &Path,
    is_dir_entry: bool,
    include_hidden_dirs: bool,
    path_filter: Option<&ProjectIndexPathFilter>,
    absolute_path: &Path,
) -> bool {
    if project_index_relative_path_is_protected(relative_path) {
        return true;
    }
    if path_filter.is_some_and(|filter| filter.is_excluded(absolute_path)) {
        return true;
    }
    if include_hidden_dirs {
        return false;
    }
    let components: Vec<_> = relative_path.components().collect();
    let ancestor_count = if is_dir_entry {
        components.len()
    } else {
        components.len().saturating_sub(1)
    };
    components[..ancestor_count].iter().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        let Some(name) = name.to_str() else {
            return false;
        };
        name.starts_with('.')
            && !PROJECT_INDEX_INDEXED_HIDDEN_DIRS
                .iter()
                .any(|indexed| name.eq_ignore_ascii_case(indexed))
    })
}

fn project_index_relative_path_is_protected(relative_path: &Path) -> bool {
    relative_path.components().any(|component| {
        let std::path::Component::Normal(name) = component else {
            return false;
        };
        name.to_str().is_some_and(|name| {
            PROJECT_INDEX_PROTECTED_WORKSPACE_DIRS
                .iter()
                .any(|protected| name.eq_ignore_ascii_case(protected))
        })
    })
}

fn project_index_remove_subtree(
    entries: &mut Vec<ProjectEntry>,
    absolute: &Path,
) -> Vec<ProjectEntry> {
    let mut removed = Vec::new();
    entries.retain(|entry| {
        if project_index_entry_starts_with(&entry.path, absolute) {
            removed.push(entry.clone());
            false
        } else {
            true
        }
    });
    removed
}

fn project_index_entry_starts_with(entry_path: &Path, prefix: &Path) -> bool {
    #[cfg(not(windows))]
    {
        entry_path.starts_with(prefix)
    }
    #[cfg(windows)]
    {
        entry_path.starts_with(prefix)
            || project_index_starts_with_case_insensitively(entry_path, prefix)
    }
}

#[cfg(windows)]
fn project_index_starts_with_case_insensitively(entry_path: &Path, prefix: &Path) -> bool {
    let mut entry_components = entry_path
        .components()
        .map(project_index_windows_component_key);
    let mut prefix_components = prefix.components().map(project_index_windows_component_key);
    loop {
        match (entry_components.next(), prefix_components.next()) {
            (Some(entry), Some(prefix_component)) => {
                if entry != prefix_component {
                    return false;
                }
            }
            (Some(_), None) => return true,
            (None, Some(_)) => return false,
            (None, None) => return true,
        }
    }
}

#[cfg(windows)]
fn project_index_windows_component_key(component: std::path::Component<'_>) -> String {
    let component = component.as_os_str().to_string_lossy();
    if component.is_ascii() {
        let mut lowered = component.into_owned();
        lowered.make_ascii_lowercase();
        lowered
    } else {
        component.to_lowercase()
    }
}

fn project_index_retain_symbols_outside(symbols: &mut Vec<ProjectSymbol>, absolute: &Path) -> bool {
    let before = symbols.len();
    symbols.retain(|symbol| !symbol.path.starts_with(absolute));
    symbols.len() != before
}

fn project_index_scan_subtree(
    workspace_root: &Path,
    subtree_root: &Path,
    include_hidden_dirs: bool,
    path_filter: Option<&ProjectIndexPathFilter>,
    max_new_files: usize,
    file_count: &mut usize,
) -> (Vec<ProjectEntry>, bool) {
    let mut entries = Vec::new();
    let mut new_files = 0usize;
    let mut hit_cap = false;
    let path_filter = match path_filter {
        Some(filter) => filter.clone(),
        None => ProjectIndexPathFilter::new(workspace_root, &[], &[]),
    };
    let walker = ignore::WalkBuilder::new(subtree_root)
        .hidden(false)
        .git_ignore(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(move |entry| {
            project_index_entry_is_not_pruned(entry, &path_filter, include_hidden_dirs)
        })
        .build();

    for entry in walker.flatten() {
        if entry.depth() == 0 {
            entries.push(ProjectEntry::from_metadata_parts(
                subtree_root.to_path_buf(),
                subtree_root
                    .strip_prefix(workspace_root)
                    .unwrap_or(subtree_root)
                    .to_path_buf(),
                true,
                subtree_root
                    .strip_prefix(workspace_root)
                    .unwrap_or(subtree_root)
                    .components()
                    .count()
                    .saturating_sub(1),
                None,
            ));
            continue;
        }
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();
        let is_file = file_type.is_file();
        if is_file {
            if new_files >= max_new_files {
                hit_cap = true;
                break;
            }
            new_files += 1;
            *file_count = file_count.saturating_add(1);
        }
        if !(is_dir || is_file) {
            continue;
        }
        let path = entry.path().to_path_buf();
        let relative_path = path
            .strip_prefix(workspace_root)
            .unwrap_or(&path)
            .to_path_buf();
        let depth = relative_path.components().count().saturating_sub(1);
        let metadata = if is_file { entry.metadata().ok() } else { None };
        entries.push(ProjectEntry::from_metadata_parts(
            path,
            relative_path,
            is_dir,
            depth,
            metadata.as_ref(),
        ));
    }

    entries.sort_unstable_by(|a, b| {
        project_index_entry_sort_cmp(&a.relative_path, a.is_dir, &b.relative_path, b.is_dir)
    });
    (entries, hit_cap)
}

fn project_index_find_file_entry<'a>(
    entries: &'a [ProjectEntry],
    relative_path: &Path,
) -> Option<&'a ProjectEntry> {
    let index = entries
        .binary_search_by(|entry| {
            project_index_entry_sort_cmp(&entry.relative_path, entry.is_dir, relative_path, false)
        })
        .ok()?;
    let entry = &entries[index];
    (!entry.is_dir && entry.relative_path == relative_path).then_some(entry)
}

fn collect_project_symbols(
    entries: &[ProjectEntry],
    total_file_count: usize,
    symbol_cap: usize,
    budget_bytes: u64,
) -> (Vec<ProjectSymbol>, u64) {
    let mut symbols = Vec::new();
    let mut ast_budget = RUST_AST_PARSE_BUDGET_BYTES;
    let budgeted = total_file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT;

    let mut files: Vec<&ProjectEntry> = entries.iter().filter(|entry| !entry.is_dir).collect();
    files.sort_unstable_by_key(|entry| entry.len);
    let mut budget_used = 0u64;
    for entry in files {
        if symbols.len() >= symbol_cap {
            break;
        }
        if budgeted {
            if budget_used >= budget_bytes {
                break;
            }
            budget_used = budget_used.saturating_add(entry.len);
        }
        symbols.extend(extract_project_symbols(
            &entry.path,
            &entry.relative_path,
            (entry.len > 0).then_some(entry.len),
            symbol_cap - symbols.len(),
            &mut ast_budget,
        ));
    }
    (symbols, budget_used)
}

fn project_index_extract_symbols_for_new_entries(
    symbols: &mut Vec<ProjectSymbol>,
    new_entries: &[ProjectEntry],
    total_file_count: usize,
    budget_remaining: &mut u64,
) -> bool {
    let mut changed = false;
    if symbols.len() >= MAX_PROJECT_SYMBOLS {
        return false;
    }
    if total_file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT && *budget_remaining == 0 {
        return false;
    }

    let mut ast_budget = if total_file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT {
        RUST_AST_PARSE_BUDGET_BYTES.min((*budget_remaining).max(1))
    } else {
        RUST_AST_PARSE_BUDGET_BYTES
    };
    for entry in new_entries.iter().filter(|entry| !entry.is_dir) {
        if symbols.len() >= MAX_PROJECT_SYMBOLS {
            break;
        }
        if total_file_count > PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT {
            if entry.len > *budget_remaining {
                continue;
            }
            *budget_remaining = budget_remaining.saturating_sub(entry.len);
        }
        let before = symbols.len();
        symbols.extend(extract_project_symbols(
            &entry.path,
            &entry.relative_path,
            (entry.len > 0).then_some(entry.len),
            MAX_PROJECT_SYMBOLS - symbols.len(),
            &mut ast_budget,
        ));
        changed |= symbols.len() != before;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn project_index_contains_directories_and_files() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-index-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join("src/lib.rs"), "").unwrap();
        fs::write(root.join("src/nested/mod.rs"), "").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let entries = index.entries(&root, 16);

        assert_eq!(index.files().len(), 2);
        assert!(
            entries
                .iter()
                .any(|entry| entry.is_dir && entry.relative_path == Path::new("src"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| entry.is_dir && entry.relative_path == Path::new("src/nested"))
        );
        assert!(
            entries
                .iter()
                .any(|entry| !entry.is_dir && entry.relative_path == Path::new("src/lib.rs"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_reports_when_file_limit_truncates_workspace() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-truncated-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/first.rs"), "").unwrap();
        fs::write(root.join("src/second.rs"), "").unwrap();

        let truncated = ProjectIndex::rebuild(&root, 1);
        assert_eq!(truncated.files().len(), 1);
        assert!(truncated.truncated());

        let complete = ProjectIndex::rebuild(&root, 2);
        assert_eq!(complete.files().len(), 2);
        assert!(!complete.truncated());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_entries_for_subroot_filter_before_limit_and_relabel() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-subroot-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/nested")).unwrap();
        fs::write(root.join("README.md"), "").unwrap();
        fs::write(root.join("src/main.rs"), "").unwrap();
        fs::write(root.join("src/nested/mod.rs"), "").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let entries = index.entries(&root.join("src"), 2);

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].relative_path, Path::new("main.rs"));
        assert_eq!(entries[0].depth, 0);
        assert_eq!(entries[1].relative_path, Path::new("nested"));
        assert_eq!(entries[1].depth, 0);
        assert!(
            entries
                .iter()
                .all(|entry| entry.path.starts_with(root.join("src")))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_signature_matches_rebuilt_index_and_stored_entries() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-signature-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();

        let options = ProjectIndexOptions::new(40_000);
        let (index, signature) = ProjectIndex::rebuild_with_signature(&root, 40_000);
        let rebuilt_again = ProjectIndex::rebuild_with_signature(&root, 40_000).1;

        assert_eq!(index.files().len(), 1);
        assert_eq!(signature, rebuilt_again);

        assert_eq!(index.signature_from_entries(&options), signature);
        assert_eq!(signature.file_count, 1);
        assert!(!signature.truncated);

        fs::write(root.join("src/lib.rs"), "fn indexed() {}\nfn newer() {}\n").unwrap();
        let changed = ProjectIndex::rebuild_with_signature(&root, 40_000).1;
        assert_ne!(signature, changed);
        assert_eq!(
            ProjectIndex::rebuild_with_signature(&root, 40_000)
                .0
                .signature_from_entries(&options),
            changed
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_ignores_workspace_state_dir() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-state-dir-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".kuroya/plugins/example")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(root.join(".kuroya/project-index.json"), "{}").unwrap();
        fs::write(root.join(".kuroya/plugins/example/plugin.toml"), "").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);

        assert_eq!(index.files(), &[root.join("src/lib.rs")]);
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| !entry.relative_path.starts_with(".kuroya"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_prunes_generated_dependency_dirs() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-generated-pruned-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::create_dir_all(root.join("node_modules/dep")).unwrap();
        fs::create_dir_all(root.join("packages/web/.next/cache")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(root.join("target/debug/generated.rs"), "fn skipped() {}\n").unwrap();
        fs::write(root.join("node_modules/dep/index.js"), "skipped();\n").unwrap();
        fs::write(
            root.join("packages/web/.next/cache/page.js"),
            "skipped();\n",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);

        assert_eq!(index.files(), &[root.join("src/lib.rs")]);
        assert!(index.all_entries().iter().all(|entry| {
            !entry.relative_path.starts_with("target")
                && !entry.relative_path.starts_with("node_modules")
                && !entry.relative_path.starts_with("packages/web/.next")
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn merged_exclude_globs_extends_defaults_with_user_globs_and_dedupes() {
        assert_eq!(
            merged_exclude_globs(&[]),
            default_project_index_exclude_globs()
        );
        assert_eq!(
            merged_exclude_globs(&["vendor".to_owned(), "  ".to_owned()]),
            [
                default_project_index_exclude_globs(),
                vec!["vendor".to_owned()]
            ]
            .concat()
        );

        let defaults = default_project_index_exclude_globs();
        let mut user_globs = vec!["Target".to_owned(), " my-cache ".to_owned()];
        user_globs.extend(defaults.iter().take(2).cloned());

        let merged = merged_exclude_globs(&user_globs);

        assert_eq!(merged.len(), defaults.len() + 2);
        assert!(merged.starts_with(&defaults[..]));
        assert_eq!(
            &merged[defaults.len()..],
            &["Target".to_owned(), "my-cache".to_owned()]
        );
    }

    #[test]
    fn project_index_skips_hidden_dirs_except_whitelist_and_keeps_hidden_files() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-hidden-dirs-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".cargo/registry")).unwrap();
        fs::create_dir_all(root.join(".github/workflows")).unwrap();
        fs::create_dir_all(root.join(".vscode")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".cargo/registry/x.txt"), "skipped()\n").unwrap();
        fs::write(root.join(".github/workflows/ci.yml"), "indexed()\n").unwrap();
        fs::write(root.join(".vscode/settings.json"), "{}").unwrap();
        fs::write(root.join(".gitignore"), "target\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new());
        let (index, signature) = ProjectIndex::rebuild_with_signature_options(&root, &options);
        let rebuilt_again = ProjectIndex::rebuild_with_signature_options(&root, &options).1;

        assert_eq!(
            index.files(),
            &[
                root.join(".github/workflows/ci.yml"),
                root.join(".gitignore"),
                root.join(".vscode/settings.json"),
                root.join("src/main.rs"),
            ]
        );
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| !entry.relative_path.starts_with(".cargo"))
        );
        assert_eq!(rebuilt_again, signature);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_include_hidden_dirs_option_indexes_hidden_dirs() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-hidden-dirs-included-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join(".cargo/registry")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join(".cargo/registry/x.txt"), "indexed()\n").unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new())
            .with_include_hidden_dirs(true);
        assert!(options.include_hidden_dirs);
        let index = ProjectIndex::rebuild_with_options(&root, &options);

        assert!(index.files().contains(&root.join(".cargo/registry/x.txt")));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_hidden_dirs_policy_invalidates_cached_signatures() {
        let root = PathBuf::from("workspace");
        let default_options = ProjectIndexOptions::new(40_000);
        let include_options = default_options.clone().with_include_hidden_dirs(true);

        assert_ne!(
            default_options.options_fingerprint(),
            include_options.options_fingerprint()
        );

        let default_signature =
            ProjectIndex::rebuild_with_signature_options(&root, &default_options).1;
        assert!(default_signature.matches_options(&default_options));
        assert!(!default_signature.matches_options(&include_options));
        let include_signature =
            ProjectIndex::rebuild_with_signature_options(&root, &include_options).1;
        assert!(include_signature.matches_options(&include_options));
        assert!(!include_signature.matches_options(&default_options));
    }

    #[test]
    fn project_index_options_can_clear_generated_excludes_but_not_protected_dirs() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-empty-excludes-protected-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target/debug")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join(".kuroya/plugins")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(
            root.join("target/debug/generated.rs"),
            "fn configurable() {}\n",
        )
        .unwrap();
        fs::write(root.join(".git/config"), "").unwrap();
        fs::write(root.join(".kuroya/state.json"), "{}").unwrap();

        let options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new());
        let index = ProjectIndex::rebuild_with_options(&root, &options);

        assert_eq!(
            index.files(),
            &[
                root.join("src/lib.rs"),
                root.join("target/debug/generated.rs")
            ]
        );
        assert!(index.all_entries().iter().all(|entry| {
            !entry.relative_path.starts_with(".git") && !entry.relative_path.starts_with(".kuroya")
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_path_filter_uses_effective_globs_and_protected_dirs() {
        let root = PathBuf::from("workspace");
        let options = ProjectIndexOptions::with_exclude_globs(40_000, vec!["generated".to_owned()]);
        let filter = options.path_filter(&root);

        assert!(filter.is_excluded(&root.join("generated/output.rs")));
        assert!(filter.is_excluded(&root.join("nested/.git/config")));
        assert!(!filter.is_excluded(&root.join("target/output.rs")));
        assert!(!filter.is_excluded(&root.join("src/main.rs")));
    }

    #[cfg(windows)]
    #[test]
    fn project_index_path_filter_matches_globs_case_insensitively_on_windows() {
        let root = PathBuf::from(r"C:\Repo\Project");
        let filter = ProjectIndexOptions::new(40_000).path_filter(&root);

        assert!(filter.is_excluded(Path::new(r"c:\repo\project\NODE_MODULES\dep\index.js")));
    }

    #[test]
    fn project_index_excluded_paths_apply_to_rebuild_signature_and_fingerprint() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-excluded-path-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let app_state = root.join("runtime-state");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(app_state.join("workspaces/current")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(
            app_state.join("workspaces/current/project-index.json"),
            "{}",
        )
        .unwrap();

        let base_options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new());
        let options = base_options.clone().with_excluded_path(app_state.clone());
        let (index, signature) = ProjectIndex::rebuild_with_signature_options(&root, &options);
        let rebuilt_again = ProjectIndex::rebuild_with_signature_options(&root, &options).1;

        assert_eq!(index.files(), &[root.join("src/lib.rs")]);
        assert_eq!(signature, rebuilt_again);
        assert_eq!(signature.file_count, 1);
        assert_ne!(
            options.options_fingerprint(),
            base_options.options_fingerprint()
        );
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| !entry.path.starts_with(&app_state))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_exclude_globs_apply_to_rebuild_and_signature() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-configurable-excludes-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("packages/web/cache")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(root.join("packages/web/cache/generated.js"), "skipped();\n").unwrap();

        let options = ProjectIndexOptions::with_exclude_globs(40_000, vec!["cache".to_owned()]);
        let (index, signature) = ProjectIndex::rebuild_with_signature_options(&root, &options);
        let rebuilt_again = ProjectIndex::rebuild_with_signature_options(&root, &options).1;

        assert_eq!(index.files(), &[root.join("src/lib.rs")]);
        assert_eq!(signature, rebuilt_again);
        assert_eq!(signature.file_count, 1);
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| { !entry.relative_path.starts_with("packages/web/cache") })
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_signature_includes_exclude_options() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-exclude-signature-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        fs::write(root.join("target/generated.rs"), "fn generated() {}\n").unwrap();

        let default_options = ProjectIndexOptions::new(40_000);
        let empty_options = ProjectIndexOptions::with_exclude_globs(40_000, Vec::new());
        let default_signature =
            ProjectIndex::rebuild_with_signature_options(&root, &default_options).1;
        let empty_signature = ProjectIndex::rebuild_with_signature_options(&root, &empty_options).1;

        assert_ne!(default_signature, empty_signature);
        assert!(default_signature.matches_options(&default_options));
        assert!(!default_signature.matches_options(&empty_options));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_signature_ignores_workspace_state_dir_changes() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-state-dir-signature-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        let before = ProjectIndex::rebuild_with_signature(&root, 40_000).1;

        fs::create_dir_all(root.join(".kuroya")).unwrap();
        fs::write(root.join(".kuroya/project-index.json"), "{}").unwrap();
        let after = ProjectIndex::rebuild_with_signature(&root, 40_000).1;

        assert_eq!(before, after);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_signature_changes_when_indexed_file_changes() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-signature-stale-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/lib.rs");
        fs::write(&path, "fn indexed() {}\n").unwrap();
        let first = ProjectIndex::rebuild_with_signature(&root, 40_000).1;

        fs::write(&path, "fn indexed() {}\nfn newer() {}\n").unwrap();
        let second = ProjectIndex::rebuild_with_signature(&root, 40_000).1;

        assert_ne!(first, second);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_signature_fingerprint_includes_created_identity() {
        let entry = ProjectEntry {
            path: PathBuf::from("workspace/src/lib.rs"),
            relative_path: PathBuf::from("src/lib.rs"),
            is_dir: false,
            depth: 1,
            len: 12,
            modified_millis: 34,
            created_millis: 56,
        };
        let changed = ProjectEntry {
            created_millis: 57,
            ..entry.clone()
        };

        let options_fingerprint = ProjectIndexOptions::new(40_000).options_fingerprint();
        let first =
            project_index_signature_fingerprint(40_000, options_fingerprint, false, &[entry]);
        let second =
            project_index_signature_fingerprint(40_000, options_fingerprint, false, &[changed]);

        assert_ne!(first, second);
    }

    #[test]
    fn project_index_extracts_workspace_symbols_from_supported_languages() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbols-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(
            root.join("src/lib.rs"),
            "pub struct AppState {}\nasync fn load_workspace() {}\nconst MAX_ITEMS: usize = 4;\n",
        )
        .unwrap();
        fs::write(
            root.join("src/app.ts"),
            "export class EditorView {}\nexport const launchTask = () => {}\nconst loadData = async () => {}\nconst makeStore = function () {}\nconst MAX_RETRIES = 3;\n",
        )
        .unwrap();
        fs::write(
            root.join("src/app.py"),
            "class Runner:\n    async def run_task(self):\n        pass\n",
        )
        .unwrap();
        fs::write(
            root.join("src/service.go"),
            "type Server struct{}\nfunc (s *Server) Serve() {}\nconst DefaultPort = 8080\n",
        )
        .unwrap();
        fs::write(
            root.join("src/App.java"),
            "public class App {}\nprivate void render() {}\n",
        )
        .unwrap();
        fs::write(
            root.join("src/native.cpp"),
            "struct NativeState {};\nint compute_value(int input) { return input; }\n",
        )
        .unwrap();
        fs::write(
            root.join("src/Program.cs"),
            "public partial class Program {}\ninternal async Task RunAsync() {}\n",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let symbols = index
            .symbols()
            .iter()
            .map(|symbol| {
                (
                    symbol.name.as_str(),
                    symbol.kind,
                    symbol.relative_path.as_path(),
                    symbol.line,
                    symbol.column,
                )
            })
            .collect::<Vec<_>>();

        assert!(symbols.contains(&(
            "AppState",
            ProjectSymbolKind::Struct,
            Path::new("src/lib.rs"),
            1,
            12
        )));
        assert!(symbols.contains(&(
            "load_workspace",
            ProjectSymbolKind::Function,
            Path::new("src/lib.rs"),
            2,
            10
        )));
        assert!(symbols.contains(&(
            "EditorView",
            ProjectSymbolKind::Class,
            Path::new("src/app.ts"),
            1,
            14
        )));
        assert!(symbols.contains(&(
            "launchTask",
            ProjectSymbolKind::Function,
            Path::new("src/app.ts"),
            2,
            14
        )));
        assert!(symbols.contains(&(
            "loadData",
            ProjectSymbolKind::Function,
            Path::new("src/app.ts"),
            3,
            7
        )));
        assert!(symbols.contains(&(
            "makeStore",
            ProjectSymbolKind::Function,
            Path::new("src/app.ts"),
            4,
            7
        )));
        assert!(symbols.contains(&(
            "MAX_RETRIES",
            ProjectSymbolKind::Constant,
            Path::new("src/app.ts"),
            5,
            7
        )));
        assert!(symbols.contains(&(
            "Runner",
            ProjectSymbolKind::Class,
            Path::new("src/app.py"),
            1,
            7
        )));
        assert!(symbols.contains(&(
            "run_task",
            ProjectSymbolKind::Function,
            Path::new("src/app.py"),
            2,
            15
        )));
        assert!(symbols.contains(&(
            "Server",
            ProjectSymbolKind::Struct,
            Path::new("src/service.go"),
            1,
            6
        )));
        assert!(symbols.contains(&(
            "Serve",
            ProjectSymbolKind::Function,
            Path::new("src/service.go"),
            2,
            18
        )));
        assert!(symbols.contains(&(
            "App",
            ProjectSymbolKind::Class,
            Path::new("src/App.java"),
            1,
            14
        )));
        assert!(symbols.contains(&(
            "compute_value",
            ProjectSymbolKind::Function,
            Path::new("src/native.cpp"),
            2,
            5
        )));
        assert!(symbols.contains(&(
            "Program",
            ProjectSymbolKind::Class,
            Path::new("src/Program.cs"),
            1,
            22
        )));
        assert!(symbols.contains(&(
            "RunAsync",
            ProjectSymbolKind::Function,
            Path::new("src/Program.cs"),
            2,
            21
        )));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_reader_enforces_limit_while_reading() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-read-limit-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("large.rs");
        fs::write(&path, "abcdef").unwrap();

        assert_eq!(
            read_symbol_text_with_limit(&path, 6).as_deref(),
            Some("abcdef")
        );
        assert!(read_symbol_text_with_limit(&path, 5).is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_extraction_uses_walk_file_len_to_skip_oversized_files() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-known-large-file-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/lib.rs");
        let relative_path = Path::new("src/lib.rs");
        fs::write(&path, "fn indexed() {}\n").unwrap();

        let mut ast = 1u64 << 20;
        assert!(
            extract_project_symbols(
                &path,
                relative_path,
                Some(MAX_SYMBOL_FILE_BYTES + 1),
                8,
                &mut ast
            )
            .is_empty()
        );

        let mut ast = 1u64 << 20;
        let symbols = extract_project_symbols(&path, relative_path, Some(16), 8, &mut ast);
        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["indexed"]
        );
        assert_eq!(symbols[0].path.as_path(), path.as_path());
        assert_eq!(symbols[0].relative_path.as_path(), relative_path);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_extraction_uses_supplied_relative_path() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-relative-path-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let path = root.join("src/lib.rs");
        let relative_path = Path::new("cached/src/lib.rs");
        fs::write(&path, "fn indexed() {}\n").unwrap();

        let mut ast = 1u64 << 20;
        let symbols = extract_project_symbols(&path, relative_path, Some(16), 8, &mut ast);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name.as_str(), "indexed");
        assert_eq!(symbols[0].relative_path.as_path(), relative_path);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_extraction_uses_source_extensions_before_reading() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-source-extension-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let go_mod = root.join("go.mod");
        let rust_path = root.join("LIB.RS");
        fs::write(&go_mod, "func skipped() {}\n").unwrap();
        fs::write(&rust_path, "fn indexed() {}\n").unwrap();

        let mut ast = 1u64 << 20;
        assert!(
            extract_project_symbols(&go_mod, Path::new("go.mod"), Some(18), 8, &mut ast).is_empty()
        );

        let mut ast = 1u64 << 20;
        let symbols =
            extract_project_symbols(&rust_path, Path::new("LIB.RS"), Some(16), 8, &mut ast);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name.as_str(), "indexed");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_extraction_skips_oversized_lines() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-line-limit-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("lib.rs");
        let text = format!(
            "fn skipped() {{}} {}\nfn indexed() {{}}\n",
            "x".repeat(MAX_SYMBOL_LINE_BYTES)
        );
        fs::write(&path, text).unwrap();

        let mut ast = 1u64 << 20;
        let symbols = extract_project_symbols(&path, Path::new("lib.rs"), None, 8, &mut ast);

        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            vec!["skipped", "indexed"]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_skips_oversized_symbol_files() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-large-file-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let text = format!(
            "fn indexed() {{}}\n{}",
            "x".repeat(MAX_SYMBOL_FILE_BYTES as usize)
        );
        fs::write(root.join("src/large.rs"), text).unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);

        assert!(index.symbols().is_empty());
        assert_eq!(index.files().len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_workspace_symbol_search_scores_names_before_paths() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-search-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/task_runner")).unwrap();
        fs::write(root.join("src/task_runner/mod.rs"), "fn unrelated() {}\n").unwrap();
        fs::write(root.join("src/lib.rs"), "fn task_runner() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let symbols = index.workspace_symbols("task", 8);

        assert_eq!(
            symbols.first().map(|symbol| symbol.name.as_str()),
            Some("task_runner")
        );
        assert!(symbols.iter().any(|symbol| symbol.name == "unrelated"
            && symbol.relative_path == Path::new("src/task_runner/mod.rs")));
        assert!(index.workspace_symbols("", 8).is_empty());
        assert!(index.workspace_symbols("missing", 8).is_empty());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_workspace_symbol_search_matches_mixed_case_paths() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-path-search-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/TaskRunner")).unwrap();
        fs::write(root.join("src/TaskRunner/mod.rs"), "fn unrelated() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let symbols = index.workspace_symbols("taskrunner", 8);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "unrelated");
        assert_eq!(symbols[0].relative_path, Path::new("src/TaskRunner/mod.rs"));

        let symbols = index.workspace_symbols("src/taskrunner", 8);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "unrelated");

        let symbols = index.workspace_symbols("src\\taskrunner", 8);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "unrelated");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_workspace_symbol_query_uses_vec_only_after_two_terms() {
        assert!(ProjectSymbolQuery::new("").is_none());

        let Some(ProjectSymbolQuery::Single(term)) = ProjectSymbolQuery::new("load") else {
            panic!("single-term query should stay stack-backed");
        };
        assert_eq!(term.text, "load");
        assert_eq!(term.path_text.as_ref(), "load");

        let Some(ProjectSymbolQuery::Single(term)) =
            ProjectSymbolQuery::new("load\u{00a0}taskrunner")
        else {
            panic!("non-ASCII whitespace should preserve the existing single-term path");
        };
        assert_eq!(term.text, "load\u{00a0}taskrunner");
        assert_eq!(term.path_text.as_ref(), "load\u{00a0}taskrunner");

        let Some(ProjectSymbolQuery::Pair(terms)) = ProjectSymbolQuery::new("load taskrunner")
        else {
            panic!("two-term query should stay stack-backed");
        };
        assert_eq!(terms[0].text, "load");
        assert_eq!(terms[1].text, "taskrunner");
        assert_eq!(terms[0].path_text.as_ref(), "load");
        assert_eq!(terms[1].path_text.as_ref(), "taskrunner");

        let Some(ProjectSymbolQuery::Many(terms)) =
            ProjectSymbolQuery::new("load taskrunner module")
        else {
            panic!("three-term query should use the general matcher path");
        };
        assert_eq!(
            terms.iter().map(|term| term.text).collect::<Vec<_>>(),
            vec!["load", "taskrunner", "module"]
        );
        assert_eq!(
            terms
                .iter()
                .map(|term| term.path_text.as_ref())
                .collect::<Vec<_>>(),
            vec!["load", "taskrunner", "module"]
        );

        assert!(ProjectSymbolQuery::new(&"x".repeat(MAX_PROJECT_SYMBOL_QUERY_CHARS + 1)).is_none());
        assert!(
            ProjectSymbolQuery::new(&vec!["x"; MAX_PROJECT_SYMBOL_QUERY_TERMS + 1].join(" "))
                .is_none()
        );
    }

    #[test]
    fn project_workspace_symbol_query_folds_ascii_path_terms_once() {
        let Some(ProjectSymbolQuery::Single(term)) = ProjectSymbolQuery::new("LOAD/Über") else {
            panic!("single-term query should stay stack-backed");
        };

        assert_eq!(term.text, "LOAD/Über");
        assert_eq!(term.path_text.as_ref(), "load/Über");
    }

    #[test]
    fn project_workspace_symbol_query_normalizes_path_separators_once() {
        let Some(ProjectSymbolQuery::Single(term)) = ProjectSymbolQuery::new("SRC\\TaskRunner")
        else {
            panic!("single-term query should stay stack-backed");
        };

        assert_eq!(term.text, "SRC\\TaskRunner");
        assert_eq!(term.path_text.as_ref(), "src/taskrunner");
    }

    #[test]
    fn project_workspace_symbol_search_prepares_path_cache_after_rebuild_and_load() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-path-cache-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/TaskRunner")).unwrap();
        fs::write(root.join("src/TaskRunner/mod.rs"), "fn unrelated() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let expected_path = "src/taskrunner/mod.rs";
        assert_eq!(index.data.symbol_search_paths.len(), 1);
        assert_eq!(index.data.symbol_search_paths[0].as_ref(), expected_path);

        let bytes = serde_json::to_vec(&index).unwrap();
        assert!(
            !std::str::from_utf8(&bytes)
                .unwrap()
                .contains("symbol_search_paths")
        );

        let loaded = serde_json::from_slice::<ProjectIndex>(&bytes).unwrap();
        assert_eq!(
            loaded.data.symbol_search_paths,
            index.data.symbol_search_paths
        );
        let symbols = loaded.workspace_symbols("taskrunner", 8);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "unrelated");

        let legacy_index = ProjectIndex::from_parts_for_test(
            loaded.root().to_path_buf(),
            loaded.files().to_vec(),
            loaded.all_entries().to_vec(),
            loaded.symbols().to_vec(),
            Vec::new(),
            loaded.truncated(),
        );
        let symbols = legacy_index.workspace_symbols("taskrunner", 8);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "unrelated");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_clone_shares_backing_storage() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-index-shallow-clone-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let cloned = index.clone();

        assert!(Arc::ptr_eq(&index.data, &cloned.data));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_workspace_symbol_search_ignores_stale_path_cache_lengths() {
        let root = PathBuf::from("workspace");
        let symbols = vec![
            ProjectSymbol {
                name: "unrelated".to_owned(),
                kind: ProjectSymbolKind::Function,
                path: root.join("src/TaskRunner/mod.rs"),
                relative_path: PathBuf::from("src/TaskRunner/mod.rs"),
                line: 1,
                column: 1,
            },
            ProjectSymbol {
                name: "other".to_owned(),
                kind: ProjectSymbolKind::Function,
                path: root.join("src/lib.rs"),
                relative_path: PathBuf::from("src/lib.rs"),
                line: 1,
                column: 1,
            },
        ];
        let index = ProjectIndex::from_parts_for_test(
            root.clone(),
            Vec::new(),
            Vec::new(),
            symbols.clone(),
            vec![project_symbol_search_path(Path::new("src/lib.rs"))],
            false,
        );

        let matches = index.workspace_symbols("taskrunner", 8);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "unrelated");

        let index = ProjectIndex::from_parts_for_test(
            root,
            Vec::new(),
            Vec::new(),
            symbols,
            Vec::new(),
            false,
        );
        let matches = index.workspace_symbols("taskrunner", 8);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "unrelated");
    }

    #[test]
    fn project_workspace_symbol_search_shares_repeated_file_path_cache_entries() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-shared-path-cache-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/TaskRunner")).unwrap();
        fs::write(
            root.join("src/TaskRunner/mod.rs"),
            "fn first_task() {}\nfn second_task() {}\n",
        )
        .unwrap();
        fs::write(root.join("src/lib.rs"), "fn third_task() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let first_index = index
            .symbols()
            .iter()
            .position(|symbol| symbol.name == "first_task")
            .unwrap();
        let second_index = index
            .symbols()
            .iter()
            .position(|symbol| symbol.name == "second_task")
            .unwrap();
        let third_index = index
            .symbols()
            .iter()
            .position(|symbol| symbol.name == "third_task")
            .unwrap();
        let task_runner_path = "src/taskrunner/mod.rs";

        assert_eq!(
            index.data.symbol_search_paths[first_index].as_ref(),
            task_runner_path
        );
        assert!(std::sync::Arc::ptr_eq(
            &index.data.symbol_search_paths[first_index],
            &index.data.symbol_search_paths[second_index]
        ));
        assert!(!std::sync::Arc::ptr_eq(
            &index.data.symbol_search_paths[first_index],
            &index.data.symbol_search_paths[third_index]
        ));

        let symbols = index.workspace_symbols("second taskrunner", 8);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "second_task");

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_symbol_search_paths_share_non_adjacent_normalized_duplicates() {
        let root = PathBuf::from("workspace");
        let symbols = vec![
            ProjectSymbol {
                name: "first_task".to_owned(),
                kind: ProjectSymbolKind::Function,
                path: root.join("src/TaskRunner/mod.rs"),
                relative_path: PathBuf::from("src/TaskRunner/mod.rs"),
                line: 1,
                column: 1,
            },
            ProjectSymbol {
                name: "middle".to_owned(),
                kind: ProjectSymbolKind::Function,
                path: root.join("src/lib.rs"),
                relative_path: PathBuf::from("src/lib.rs"),
                line: 1,
                column: 1,
            },
            ProjectSymbol {
                name: "second_task".to_owned(),
                kind: ProjectSymbolKind::Function,
                path: root.join("src/TaskRunner/mod.rs"),
                relative_path: PathBuf::from("src\\TaskRunner\\mod.rs"),
                line: 2,
                column: 1,
            },
        ];

        let paths = project_symbol_search_paths(&symbols);

        assert_eq!(paths[0].as_ref(), "src/taskrunner/mod.rs");
        assert_eq!(paths[1].as_ref(), "src/lib.rs");
        assert_eq!(paths[2].as_ref(), "src/taskrunner/mod.rs");
        assert!(std::sync::Arc::ptr_eq(&paths[0], &paths[2]));
        assert!(!std::sync::Arc::ptr_eq(&paths[0], &paths[1]));
    }

    #[test]
    fn project_workspace_symbol_search_reuses_prepared_multi_term_matchers() {
        let root = std::env::temp_dir().join(format!(
            "kuroya-project-symbol-multi-term-search-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src/TaskRunner")).unwrap();
        fs::write(
            root.join("src/TaskRunner/mod.rs"),
            "fn load_workspace() {}\nfn unrelated() {}\n",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let symbols = index.workspace_symbols("LOAD taskrunner", 8);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "load_workspace");
        assert_eq!(symbols[0].relative_path, Path::new("src/TaskRunner/mod.rs"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ascii_case_insensitive_symbol_matching_preserves_ascii_only_folding() {
        assert!(contains_ascii_case_insensitive("TaskRunner", "task"));
        assert!(starts_with_ascii_case_insensitive("TaskRunner", "task"));
        assert!(!contains_ascii_case_insensitive("Über", "über"));
    }

    #[test]
    fn project_index_entries_capture_file_metadata() {
        let root = temp_project_dir("project-entry-metadata");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();

        let index = ProjectIndex::rebuild(&root, 40_000);
        let file_entry = index
            .all_entries()
            .iter()
            .find(|entry| entry.relative_path == Path::new("src/lib.rs"))
            .unwrap()
            .clone();
        let dir_entry = index
            .all_entries()
            .iter()
            .find(|entry| entry.relative_path == Path::new("src"))
            .unwrap()
            .clone();

        assert_eq!(
            file_entry.len,
            fs::metadata(root.join("src/lib.rs")).unwrap().len()
        );
        assert!(file_entry.modified_millis > 0);
        assert!(file_entry.created_millis > 0);
        assert_eq!(
            (
                dir_entry.len,
                dir_entry.modified_millis,
                dir_entry.created_millis
            ),
            (0, 0, 0)
        );

        let bytes = serde_json::to_vec(&index).unwrap();
        let loaded = serde_json::from_slice::<ProjectIndex>(&bytes).unwrap();
        let loaded_entry = loaded
            .all_entries()
            .iter()
            .find(|entry| entry.relative_path == Path::new("src/lib.rs"))
            .unwrap();
        assert_eq!(loaded_entry.len, file_entry.len);
        assert_eq!(loaded_entry.modified_millis, file_entry.modified_millis);
        assert_eq!(loaded_entry.created_millis, file_entry.created_millis);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collect_project_symbols_charges_the_byte_budget_and_skips_large_files() {
        let root = temp_project_dir("collect-symbols-budget");
        fs::create_dir_all(&root).unwrap();

        fs::write(
            root.join("small_a.rs"),
            "fn small_a() {}
",
        )
        .unwrap();
        fs::write(
            root.join("small_b.rs"),
            "fn small_b() {}
",
        )
        .unwrap();
        fs::write(
            root.join("big.rs"),
            format!(
                "// {}
",
                "x".repeat(4096)
            ),
        )
        .unwrap();
        let big_len = fs::metadata(root.join("big.rs")).unwrap().len();

        let entry = |path: std::path::PathBuf| {
            let metadata = fs::metadata(&path).unwrap();
            let relative = path.strip_prefix(&root).unwrap().to_path_buf();
            ProjectEntry::from_metadata_parts(path.clone(), relative, false, 0, Some(&metadata))
        };
        let mut entries: Vec<ProjectEntry> = ["small_a.rs", "small_b.rs", "big.rs"]
            .iter()
            .map(|name| entry(root.join(name)))
            .collect();
        let small_len = entries[0].len;
        assert!(small_len > 0);
        assert_eq!(big_len, 4100);

        let budget = small_len * 2;
        let (symbols, used) = collect_project_symbols(
            &entries,
            PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT + 1,
            MAX_PROJECT_SYMBOLS,
            budget,
        );
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["small_a", "small_b"], "smallest files first");
        assert!(
            used <= budget,
            "spend {used} must stay within budget {budget}"
        );

        entries.clear();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn symbol_cap_does_not_starve_late_alphabetical_files() {
        let root = temp_project_dir("symbol-cap-fairness");
        fs::create_dir_all(&root).unwrap();

        let stuffing = "fn filler() {}
"
        .repeat(200);
        fs::write(root.join("a_file.rs"), stuffing.clone()).unwrap();
        fs::write(root.join("b_file.rs"), stuffing).unwrap();

        fs::write(
            root.join("z_tiny.rs"),
            "pub struct TinySymbol;
",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, DEFAULT_PROJECT_INDEX_MAX_FILES);
        let names: Vec<&str> = index.symbols().iter().map(|s| s.name.as_str()).collect();

        assert!(
            names.contains(&"TinySymbol"),
            "the tiny late-alphabetical file must be scanned first: {names:?}"
        );

        let fillers = names.iter().filter(|name| **name == "filler").count();
        assert!(
            fillers < 400,
            "cap should have trimmed the stuffed files: {fillers}"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rust_ast_extraction_beats_the_line_scanner_on_string_literals() {
        let root = temp_project_dir("rust-ast-vs-line-scan");
        fs::create_dir_all(&root).unwrap();

        fs::write(
            root.join("lib.rs"),
            "const SNIPPET: &str = \"fn fake() {}\";
fn real() {}
",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, DEFAULT_PROJECT_INDEX_MAX_FILES);
        let names: Vec<&str> = index.symbols().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"real"), "real symbols extracted: {names:?}");
        assert!(
            !names.contains(&"fake"),
            "string literals must not produce symbols: {names:?}"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_symbol_scan_is_budgeted_beyond_file_limit() {
        let root = temp_project_dir("project-symbol-scan-limit");
        fs::create_dir_all(root.join("src")).unwrap();
        for index in 0..PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT {
            let _ = fs::write(root.join("src").join(format!("mod_{index}.rs")), "");
        }

        fs::write(
            root.join("src").join("lib.rs"),
            "pub struct BudgetedSymbol;
",
        )
        .unwrap();

        let index = ProjectIndex::rebuild(&root, DEFAULT_PROJECT_INDEX_MAX_FILES);

        assert_eq!(
            index.files().len(),
            PROJECT_INDEX_SYMBOL_SCAN_FILE_LIMIT + 1
        );
        assert!(
            index
                .symbols()
                .iter()
                .any(|symbol| symbol.name == "BudgetedSymbol"),
            "indexes above the symbol scan limit keep symbols within the byte budget"
        );
        assert!(!index.truncated());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_is_warm_distinguishes_placeholder() {
        assert!(!ProjectIndex::default().is_warm());
        assert_eq!(ProjectIndex::default().max_files(), 0);

        let root = temp_project_dir("project-is-warm");
        fs::create_dir_all(&root).unwrap();
        let index = ProjectIndex::rebuild(&root, 40_000);
        assert!(index.is_warm());
        assert_eq!(index.max_files(), 40_000);
        assert!(
            ProjectIndex::rebuild(&root, 40_000)
                .clone_for_update()
                .is_warm()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_upserts_new_changed_and_deleted_files() {
        let root = temp_project_dir("project-apply-files");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        assert!(!index.apply_path_changes(&root, &[root.join("src/a.rs")]));

        fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();

        fs::write(root.join("src/a.rs"), "fn a() {}\nfn more() {}\n").unwrap();

        let deleted = root.join("src/a.rs");
        fs::remove_file(&deleted).unwrap();

        assert!(index.apply_path_changes(
            &root,
            &[
                root.join("src/b.rs"),
                root.join("src/a.rs"),
                deleted.clone()
            ],
        ));

        assert_eq!(index.files(), &[root.join("src/b.rs")]);
        assert_eq!(
            index
                .all_entries()
                .iter()
                .map(|entry| entry.relative_path.as_path())
                .collect::<Vec<_>>(),
            &[Path::new("src"), Path::new("src/b.rs")]
        );
        assert!(
            !index
                .all_entries()
                .iter()
                .any(|entry| entry.path == deleted)
        );
        assert!(index.is_warm());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn project_index_apply_path_changes_resolves_case_only_renames_without_ghosts() {
        let root = temp_project_dir("project-apply-case-rename");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/Foo.rs"), "fn foo() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);
        assert_eq!(index.files(), &[root.join("src/Foo.rs")]);

        fs::rename(root.join("src/Foo.rs"), root.join("src/foo.rs")).unwrap();

        assert!(index.apply_path_changes(&root, &[root.join("src/foo.rs")]));
        assert_eq!(index.files(), &[root.join("src/foo.rs")]);
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| entry.path != root.join("src/Foo.rs"))
        );

        fs::rename(root.join("src/foo.rs"), root.join("src/Foo.rs")).unwrap();
        assert!(index.apply_path_changes(&root, &[root.join("src")]));
        assert_eq!(index.files(), &[root.join("src/Foo.rs")]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_scans_created_directory_subtree() {
        let root = temp_project_dir("project-apply-dir-create");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("top.rs"), "fn top() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        fs::create_dir_all(root.join("pkg/nested")).unwrap();
        fs::write(root.join("pkg/inner.rs"), "fn inner() {}\n").unwrap();
        fs::write(root.join("pkg/nested/deep.rs"), "fn deep() {}\n").unwrap();

        assert!(index.apply_path_changes(&root, &[root.join("pkg")]));

        assert_eq!(
            index.files(),
            &[
                root.join("pkg/inner.rs"),
                root.join("pkg/nested/deep.rs"),
                root.join("top.rs"),
            ]
        );
        assert_eq!(
            index
                .all_entries()
                .iter()
                .map(|entry| entry.relative_path.as_path())
                .collect::<Vec<_>>(),
            &[
                Path::new("pkg"),
                Path::new("pkg/inner.rs"),
                Path::new("pkg/nested"),
                Path::new("pkg/nested/deep.rs"),
                Path::new("top.rs"),
            ]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_removes_deleted_directory_subtree() {
        let root = temp_project_dir("project-apply-dir-delete");
        fs::create_dir_all(root.join("pkg/nested")).unwrap();
        fs::write(root.join("top.rs"), "fn top() {}\n").unwrap();
        fs::write(root.join("pkg/inner.rs"), "fn inner() {}\n").unwrap();
        fs::write(root.join("pkg/nested/deep.rs"), "fn deep() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        fs::remove_dir_all(root.join("pkg")).unwrap();

        assert!(index.apply_path_changes(&root, &[root.join("pkg")]));

        assert_eq!(index.files(), &[root.join("top.rs")]);
        assert!(
            index
                .all_entries()
                .iter()
                .all(|entry| !entry.path.starts_with(root.join("pkg")))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_preserves_entry_sort_invariants() {
        let root = temp_project_dir("project-apply-sorted");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/keep.rs"), "fn keep() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        fs::write(root.join("aaa.rs"), "fn aaa() {}\n").unwrap();
        fs::create_dir_all(root.join("zzz")).unwrap();
        fs::write(root.join("zzz/last.rs"), "fn last() {}\n").unwrap();
        fs::write(root.join("mm.rs"), "fn mm() {}\n").unwrap();

        assert!(index.apply_path_changes(
            &root,
            &[root.join("zzz"), root.join("aaa.rs"), root.join("mm.rs")],
        ));

        let relative_paths = index
            .all_entries()
            .iter()
            .map(|entry| entry.relative_path.clone())
            .collect::<Vec<_>>();
        let mut sorted = relative_paths.clone();
        sorted.sort();
        assert_eq!(relative_paths, sorted);
        assert!(index.files().windows(2).all(|pair| pair[0] < pair[1]));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_ignores_excluded_protected_and_hidden_paths() {
        let root = temp_project_dir("project-apply-policy");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn indexed() {}\n").unwrap();
        let options = ProjectIndexOptions::with_exclude_globs(40_000, vec!["generated".to_owned()]);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        fs::create_dir_all(root.join("generated")).unwrap();
        fs::create_dir_all(root.join(".cargo/registry")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        fs::create_dir_all(root.join(".kuroya")).unwrap();
        fs::write(root.join("generated/out.rs"), "fn skipped() {}\n").unwrap();
        fs::write(root.join(".cargo/registry/x.txt"), "skipped()\n").unwrap();
        fs::write(root.join(".git/config"), "").unwrap();
        fs::write(root.join(".kuroya/state.json"), "{}").unwrap();

        assert!(!index.apply_path_changes(
            &root,
            &[
                root.join("generated"),
                root.join("generated/out.rs"),
                root.join(".cargo"),
                root.join(".cargo/registry/x.txt"),
                root.join(".git"),
                root.join(".git/config"),
                root.join(".kuroya"),
                root.join(".kuroya/state.json"),
                root.join("outside-the-workspace.rs"),
                root.join("src/lib.rs"),
            ],
        ));

        assert_eq!(index.files(), &[root.join("src/lib.rs")]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_marks_truncated_at_file_limit() {
        let root = temp_project_dir("project-apply-truncated");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        fs::write(root.join("src/b.rs"), "fn b() {}\n").unwrap();
        let options = ProjectIndexOptions::new(2);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);
        assert!(!index.truncated());

        fs::write(root.join("src/c.rs"), "fn c() {}\n").unwrap();

        assert!(index.apply_path_changes(&root, &[root.join("src/c.rs")]));
        assert!(index.truncated());
        assert_eq!(
            index.files(),
            &[root.join("src/a.rs"), root.join("src/b.rs")]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_replaces_symbols_for_changed_file() {
        let root = temp_project_dir("project-apply-symbols");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn old_name() {}\n").unwrap();
        fs::write(root.join("src/other.rs"), "fn untouched() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);
        assert!(
            index
                .symbols()
                .iter()
                .any(|symbol| symbol.name == "old_name")
        );

        fs::write(root.join("src/lib.rs"), "fn new_name() {}\nfn extra() {}\n").unwrap();

        assert!(index.apply_path_changes(&root, &[root.join("src/lib.rs")]));

        let names = index
            .symbols()
            .iter()
            .map(|symbol| symbol.name.as_str())
            .collect::<Vec<_>>();
        assert!(!names.contains(&"old_name"));
        assert!(names.contains(&"new_name"));
        assert!(names.contains(&"extra"));
        assert!(
            names.contains(&"untouched"),
            "other files keep their symbols"
        );
        assert!(
            index
                .symbols()
                .iter()
                .all(|symbol| symbol.relative_path != Path::new("src/lib.rs")
                    || symbol.name == "new_name"
                    || symbol.name == "extra")
        );

        fs::remove_file(root.join("src/lib.rs")).unwrap();
        assert!(index.apply_path_changes(&root, &[root.join("src/lib.rs")]));
        assert!(
            index
                .symbols()
                .iter()
                .all(|symbol| symbol.relative_path != Path::new("src/lib.rs"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn project_index_apply_path_changes_matches_fresh_rebuild_signature() {
        let root = temp_project_dir("project-apply-signature");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/a.rs"), "fn a() {}\n").unwrap();
        let options = ProjectIndexOptions::new(40_000);
        let mut index = ProjectIndex::rebuild_with_options(&root, &options);

        fs::write(root.join("src/b.rs"), "pub struct NewThing {}\n").unwrap();
        fs::write(root.join("src/a.rs"), "fn a_changed() {}\n").unwrap();
        fs::create_dir_all(root.join("more")).unwrap();
        fs::write(root.join("more/c.rs"), "fn c() {}\n").unwrap();

        assert!(index.apply_path_changes(
            &root,
            &[
                root.join("src/b.rs"),
                root.join("src/a.rs"),
                root.join("more")
            ],
        ));

        let (rebuilt, signature) = ProjectIndex::rebuild_with_signature_options(&root, &options);
        assert_eq!(index.signature_from_entries(&options), signature);
        assert_eq!(index.files(), rebuilt.files());
        assert_eq!(index.all_entries(), rebuilt.all_entries());

        fs::remove_dir_all(root).unwrap();
    }

    fn temp_project_dir(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "kuroya-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
}
