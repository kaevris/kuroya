use crate::{
    ProjectIndex,
    buffer::regex_query_is_line_local,
    settings::{DEFAULT_PROJECT_SEARCH_MAX_FILE_SIZE_MB, DEFAULT_PROJECT_SEARCH_MAX_RESULTS},
    text_match::AsciiCaseInsensitiveMatcher,
};
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::io;
use std::{
    collections::{HashMap, HashSet},
    fmt::{self, Write as _},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, UNIX_EPOCH},
};

const MAX_SEARCH_PREVIEW_CHARS: usize = 240;
const SEARCH_PREVIEW_CONTEXT_CHARS: usize = 96;
const SEARCH_FILE_CHUNK_SIZE: usize = 256;
const SEARCH_STREAM_BUFFER_BYTES: usize = 16 * 1024;
const SEARCH_CANCEL_LINE_INTERVAL: usize = 64;
const SEARCH_CANCEL_MATCH_INTERVAL: usize = 64;
const SEARCH_CANCEL_BYTE_INTERVAL: usize = 16 * 1024;
const MAX_SEARCH_QUERY_BYTES: usize = SEARCH_CANCEL_BYTE_INTERVAL;
#[cfg(test)]
const PROJECT_SEARCH_INDEX_MAX_TEXT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SEARCH_GLOB_PATTERNS: usize = 1024;
const MAX_SEARCH_GLOB_PATTERN_BYTES: usize = 4096;
const GLOB_ERROR_PATTERN_MAX_CHARS: usize = 120;
const GLOB_ERROR_DETAIL_MAX_CHARS: usize = 240;
const MIN_PROJECT_SEARCH_WORKER_THREADS: usize = 2;
const MAX_PROJECT_SEARCH_WORKER_THREADS: usize = 8;
const PROJECT_SEARCH_CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(1);
const PROJECT_SEARCH_CANCEL_SPIN_ITERATIONS: usize = 256;
const REGEX_MULTILINE_QUERY_ERROR: &str =
    "Invalid regular expression: pattern must match within a single line";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchOptions {
    pub query: String,
    pub max_file_bytes: u64,
    pub max_results: usize,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    pub include_globs: Vec<String>,
    pub exclude_globs: Vec<String>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            query: String::new(),
            max_file_bytes: DEFAULT_PROJECT_SEARCH_MAX_FILE_SIZE_MB * 1024 * 1024,
            max_results: DEFAULT_PROJECT_SEARCH_MAX_RESULTS,
            case_sensitive: false,
            whole_word: false,
            regex: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub path: PathBuf,
    pub line: usize,
    pub column: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SearchStats {
    pub searched_files: usize,
    pub matched_files: usize,
    pub skipped_large_files: usize,
    pub skipped_binary_files: usize,
    pub skipped_unreadable_files: usize,
    #[cfg(test)]
    pub(crate) skipped_index_budget_files: usize,
}

impl SearchStats {
    pub fn skipped_files(self) -> usize {
        self.skipped_large_files
            .saturating_add(self.skipped_binary_files)
            .saturating_add(self.skipped_unreadable_files)
            .saturating_add(self.skipped_index_budget_files_for_tests())
    }

    pub fn merge(&mut self, other: Self) {
        self.searched_files = self.searched_files.saturating_add(other.searched_files);
        self.matched_files = self.matched_files.saturating_add(other.matched_files);
        self.skipped_large_files = self
            .skipped_large_files
            .saturating_add(other.skipped_large_files);
        self.skipped_binary_files = self
            .skipped_binary_files
            .saturating_add(other.skipped_binary_files);
        self.skipped_unreadable_files = self
            .skipped_unreadable_files
            .saturating_add(other.skipped_unreadable_files);
        self.merge_skipped_index_budget_files_for_tests(other);
    }

    #[cfg(test)]
    fn skipped_index_budget_files_for_tests(self) -> usize {
        self.skipped_index_budget_files
    }

    #[cfg(not(test))]
    fn skipped_index_budget_files_for_tests(self) -> usize {
        0
    }

    #[cfg(test)]
    fn merge_skipped_index_budget_files_for_tests(&mut self, other: Self) {
        self.skipped_index_budget_files = self
            .skipped_index_budget_files
            .saturating_add(other.skipped_index_budget_files);
    }

    #[cfg(not(test))]
    fn merge_skipped_index_budget_files_for_tests(&mut self, _other: Self) {}
}

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    pub error: Option<String>,
    pub stats: SearchStats,
}

#[derive(Debug, Clone, Default)]
pub struct SearchProgress {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    pub stats: SearchStats,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectSearchMetadataCache {
    entries: Arc<Mutex<HashMap<PathBuf, CachedProjectSearchMetadata>>>,
}

impl ProjectSearchMetadataCache {
    pub fn clear(&self) {
        self.with_entries(|entries| entries.clear());
    }

    pub fn len(&self) -> usize {
        self.with_entries(|entries| entries.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn get(
        &self,
        path: &Path,
        max_file_bytes: u64,
        signature: SearchFileMetadataSignature,
    ) -> Option<ProjectSearchFileMetadataState> {
        self.with_entries(|entries| {
            entries.get(path).and_then(|entry| {
                (entry.max_file_bytes == max_file_bytes && entry.signature == signature)
                    .then_some(entry.state)
            })
        })
    }

    fn insert(
        &self,
        path: &Path,
        max_file_bytes: u64,
        signature: SearchFileMetadataSignature,
        state: ProjectSearchFileMetadataState,
    ) {
        self.with_entries(|entries| {
            entries.insert(
                path.to_path_buf(),
                CachedProjectSearchMetadata {
                    max_file_bytes,
                    signature,
                    state,
                },
            );
        });
    }

    fn with_entries<T>(
        &self,
        use_entries: impl FnOnce(&mut HashMap<PathBuf, CachedProjectSearchMetadata>) -> T,
    ) -> T {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        use_entries(&mut entries)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CachedProjectSearchMetadata {
    max_file_bytes: u64,
    signature: SearchFileMetadataSignature,
    state: ProjectSearchFileMetadataState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SearchFileMetadataSignature {
    len: u64,
    modified_nanos: u128,
    created_nanos: u128,
}

impl SearchFileMetadataSignature {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified_nanos: metadata_modified_nanos(metadata),
            created_nanos: metadata_created_nanos(metadata),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProjectSearchFileMetadataState {
    Searchable,
    TooLarge,
    BinaryOrInvalid,
    Unreadable,
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
struct ProjectSearchIndex {
    root: PathBuf,
    max_file_bytes: u64,
    files: Vec<ProjectSearchIndexedFile>,
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct ProjectSearchIndexedFile {
    path: PathBuf,
    signature: Option<ProjectSearchIndexedFileSignature>,
    content: ProjectSearchIndexedFileContent,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectSearchIndexedFileSignature {
    len: u64,
    modified_nanos: u128,
    created_nanos: u128,
}

#[cfg(test)]
#[derive(Debug, Clone)]
enum ProjectSearchIndexedFileContent {
    Text {
        text: String,
        byte_len: u64,
    },
    TooLarge,
    BinaryOrInvalid,
    Unreadable,
    #[cfg(test)]
    IndexBudgetExceeded,
}

#[cfg(test)]
impl ProjectSearchIndex {
    fn build(index: &ProjectIndex, max_file_bytes: u64) -> Self {
        Self::build_with_cancel(index, max_file_bytes, || false)
            .expect("non-cancellable search index build should complete")
    }

    fn build_with_cancel(
        index: &ProjectIndex,
        max_file_bytes: u64,
        is_cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        Self::build_with_text_budget_and_cancel(
            index,
            max_file_bytes,
            PROJECT_SEARCH_INDEX_MAX_TEXT_BYTES,
            is_cancelled,
        )
    }

    fn build_with_text_budget(
        index: &ProjectIndex,
        max_file_bytes: u64,
        max_text_bytes: u64,
    ) -> Self {
        Self::build_with_text_budget_and_cancel(index, max_file_bytes, max_text_bytes, || false)
            .expect("non-cancellable search index build should complete")
    }

    fn build_with_text_budget_and_cancel(
        index: &ProjectIndex,
        max_file_bytes: u64,
        max_text_bytes: u64,
        is_cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        let root = index.root().to_path_buf();
        let max_file_bytes = effective_search_file_byte_limit(max_file_bytes);
        let mut indexed_text_bytes = 0u64;
        let source_files = index.files();
        let mut files = Vec::with_capacity(source_files.len());
        for path in source_files {
            if is_cancelled() {
                return None;
            }
            let (signature, content) = read_indexed_search_file_content(
                path,
                max_file_bytes,
                max_text_bytes,
                &mut indexed_text_bytes,
            );
            if is_cancelled() {
                return None;
            }
            files.push(ProjectSearchIndexedFile {
                path: path.clone(),
                signature,
                content,
            });
        }
        Some(Self {
            root,
            max_file_bytes,
            files,
        })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn len(&self) -> usize {
        self.files.len()
    }

    fn max_file_bytes(&self) -> u64 {
        self.max_file_bytes
    }
}

pub fn search_project(index: &ProjectIndex, options: &SearchOptions) -> SearchResult {
    search_project_with_cancel(index, options, || false).unwrap_or_default()
}

pub fn search_project_with_cancel(
    index: &ProjectIndex,
    options: &SearchOptions,
    is_cancelled: impl Fn() -> bool,
) -> Option<SearchResult> {
    search_project_with_cancel_and_progress(index, options, is_cancelled, |_| {})
}

pub fn search_project_with_cancel_and_progress(
    index: &ProjectIndex,
    options: &SearchOptions,
    is_cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(SearchProgress),
) -> Option<SearchResult> {
    search_project_with_optional_metadata_cache_and_progress(
        index,
        options,
        None,
        is_cancelled,
        &mut on_progress,
    )
}

pub fn search_project_with_metadata_cache_and_progress(
    index: &ProjectIndex,
    options: &SearchOptions,
    metadata_cache: &ProjectSearchMetadataCache,
    is_cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(SearchProgress),
) -> Option<SearchResult> {
    search_project_with_optional_metadata_cache_and_progress(
        index,
        options,
        Some(metadata_cache),
        is_cancelled,
        &mut on_progress,
    )
}

fn search_project_with_optional_metadata_cache_and_progress(
    index: &ProjectIndex,
    options: &SearchOptions,
    metadata_cache: Option<&ProjectSearchMetadataCache>,
    is_cancelled: impl Fn() -> bool,
    on_progress: &mut dyn FnMut(SearchProgress),
) -> Option<SearchResult> {
    let prepared = match PreparedProjectSearch::new_with_cancel(options, &is_cancelled)? {
        Ok(Some(prepared)) => prepared,
        Ok(None) => return Some(SearchResult::default()),
        Err(error) => return Some(search_error_result(error)),
    };
    search_project_prepared(
        index,
        options,
        &prepared,
        metadata_cache,
        &is_cancelled,
        on_progress,
    )
}

#[cfg(test)]
fn search_project_index(index: &ProjectSearchIndex, options: &SearchOptions) -> SearchResult {
    search_project_index_with_cancel(index, options, || false).unwrap_or_default()
}

#[cfg(test)]
fn search_project_index_with_cancel(
    index: &ProjectSearchIndex,
    options: &SearchOptions,
    is_cancelled: impl Fn() -> bool,
) -> Option<SearchResult> {
    let prepared = match PreparedProjectSearch::new_with_cancel(options, &is_cancelled)? {
        Ok(Some(prepared)) => prepared,
        Ok(None) => return Some(SearchResult::default()),
        Err(error) => return Some(search_error_result(error)),
    };

    search_project_index_prepared(index, options, &prepared, &is_cancelled)
}

fn search_error_result(error: String) -> SearchResult {
    SearchResult {
        error: Some(error),
        ..SearchResult::default()
    }
}

struct PreparedProjectSearch<'a> {
    line_needle: LineSearchNeedle<'a>,
    include_globs: Option<GlobSet>,
    exclude_globs: Option<GlobSet>,
}

impl<'a> PreparedProjectSearch<'a> {
    #[cfg(test)]
    fn new(options: &'a SearchOptions) -> Result<Option<Self>, String> {
        let never_cancelled = || false;
        match Self::new_with_cancel(options, &never_cancelled) {
            Some(result) => result,
            None => Ok(None),
        }
    }

    fn new_with_cancel(
        options: &'a SearchOptions,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Option<Result<Option<Self>, String>> {
        let needle = options.query.trim();
        if needle.is_empty() {
            return Some(Ok(None));
        }
        if is_cancelled() {
            return None;
        }
        if needle.len() > MAX_SEARCH_QUERY_BYTES {
            return Some(Err(format!(
                "Search query is too long; maximum is {MAX_SEARCH_QUERY_BYTES} bytes"
            )));
        }

        let include_globs = match build_glob_set(&options.include_globs) {
            Ok(globs) => globs,
            Err(error) => return Some(Err(error)),
        };
        if is_cancelled() {
            return None;
        }
        let exclude_globs = match build_glob_set(&options.exclude_globs) {
            Ok(globs) => globs,
            Err(error) => return Some(Err(error)),
        };
        if is_cancelled() {
            return None;
        }
        let line_needle =
            match LineSearchNeedle::prepare(needle, options.case_sensitive, options.regex) {
                Ok(line_needle) => line_needle,
                Err(error) => return Some(Err(error)),
            };
        Some(Ok(Some(Self {
            line_needle,
            include_globs,
            exclude_globs,
        })))
    }
}

/// Outcome of scanning one file during a parallel project search chunk.
///
/// `consumed_matches` counts every match that charged the worker's result
/// budget (collected and hidden), so the merge thread can replay the exact
/// consumption order against the shared budget and keep `truncated` exact.
struct ProjectScannedFile {
    matches: Vec<SearchMatch>,
    consumed_matches: usize,
    stats: SearchStats,
}

enum ProjectSearchWorkerMessage {
    File {
        file_index: usize,
        scanned: ProjectScannedFile,
    },
    Done {
        chunk_index: usize,
        cancelled: bool,
    },
}

struct ProjectSearchChunkJob<'a> {
    chunk_index: usize,
    file_offset: usize,
    files: &'a [PathBuf],
    root: &'a Path,
    line_needle: &'a LineSearchNeedle<'a>,
    options: &'a SearchOptions,
    include_globs: Option<&'a GlobSet>,
    exclude_globs: Option<&'a GlobSet>,
    metadata_cache: Option<&'a ProjectSearchMetadataCache>,
    worker_cancelled: &'a AtomicBool,
    stop_spawning: &'a AtomicBool,
    sender: mpsc::Sender<ProjectSearchWorkerMessage>,
}

fn project_search_worker_thread_count() -> usize {
    thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1)
        .clamp(
            MIN_PROJECT_SEARCH_WORKER_THREADS,
            MAX_PROJECT_SEARCH_WORKER_THREADS,
        )
}

fn search_project_prepared(
    index: &ProjectIndex,
    options: &SearchOptions,
    prepared: &PreparedProjectSearch<'_>,
    metadata_cache: Option<&ProjectSearchMetadataCache>,
    is_cancelled: &dyn Fn() -> bool,
    on_progress: &mut dyn FnMut(SearchProgress),
) -> Option<SearchResult> {
    // Mirrors the sequential loop-top cancellation check that used to run
    // before the first file was touched.
    if (is_cancelled)() {
        return None;
    }

    let files = index.files();
    let worker_count = project_search_worker_thread_count();
    let chunk_size = files.len().div_ceil(worker_count).max(1);
    // The cancellation callback is only ever invoked from this thread; worker
    // threads observe the shared flag instead so arbitrary non-Sync closures
    // keep working.
    let worker_cancelled = Arc::new(AtomicBool::new(false));
    let stop_spawning = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();

    let mut result_budget = SearchResultBudget::new(options.max_results);
    let mut stats = SearchStats::default();
    let mut progress_stats = SearchStats::default();
    let mut truncated = false;
    let mut matches = Vec::with_capacity(result_budget.limit().min(1024));
    let mut emitted_matches = 0usize;
    let mut outcome = None;

    thread::scope(|scope| {
        let mut spawned_chunks = 0usize;
        for chunk_index in 0..worker_count {
            let chunk_start = chunk_index * chunk_size;
            if chunk_start >= files.len() {
                break;
            }
            let chunk_end = (chunk_start + chunk_size).min(files.len());
            spawned_chunks += 1;
            let job = ProjectSearchChunkJob {
                chunk_index,
                file_offset: chunk_start,
                files: &files[chunk_start..chunk_end],
                root: index.root(),
                line_needle: &prepared.line_needle,
                options,
                include_globs: prepared.include_globs.as_ref(),
                exclude_globs: prepared.exclude_globs.as_ref(),
                metadata_cache,
                worker_cancelled: &worker_cancelled,
                stop_spawning: &stop_spawning,
                sender: sender.clone(),
            };
            scope.spawn(move || run_project_search_chunk(job));
        }
        // All workers hold a sender clone; dropping this one lets the merge
        // loop detect the disconnected channel once every chunk reported done.
        drop(sender);

        let mut chunk_done = vec![false; spawned_chunks];
        let mut done_chunks = 0usize;
        let mut pending: HashMap<usize, ProjectScannedFile> = HashMap::new();
        let mut next_file_index = 0usize;
        let mut merging_complete = files.is_empty();
        let mut cancelled = false;

        while done_chunks < spawned_chunks {
            // Re-check the callback with a bounded spin so even scans that
            // finish between messages still observe cancellation checkpoints,
            // then surface it to the workers through the shared flag. Once
            // cancelled, stop consulting the callback entirely, mirroring the
            // sequential early return.
            if !cancelled {
                for _ in 0..PROJECT_SEARCH_CANCEL_SPIN_ITERATIONS {
                    if (is_cancelled)() {
                        worker_cancelled.store(true, Ordering::Relaxed);
                        cancelled = true;
                        break;
                    }
                }
            }
            if worker_cancelled.load(Ordering::Relaxed) {
                cancelled = true;
            }
            match receiver.recv_timeout(PROJECT_SEARCH_CANCEL_POLL_INTERVAL) {
                Ok(ProjectSearchWorkerMessage::File {
                    file_index,
                    scanned,
                }) => {
                    pending.insert(file_index, scanned);
                }
                Ok(ProjectSearchWorkerMessage::Done {
                    chunk_index,
                    cancelled: worker_cancel,
                }) => {
                    if !chunk_done[chunk_index] {
                        chunk_done[chunk_index] = true;
                        done_chunks += 1;
                    }
                    cancelled |= worker_cancel;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            // Merge buffered per-file results strictly in file order so the
            // output, stats, and truncation match a sequential scan exactly.
            while !cancelled && !merging_complete && next_file_index < files.len() {
                let chunk_index = next_file_index / chunk_size;
                if chunk_index >= spawned_chunks {
                    merging_complete = true;
                    break;
                }
                let scanned = match pending.remove(&next_file_index) {
                    Some(scanned) => scanned,
                    None => {
                        if chunk_done[chunk_index] {
                            // The chunk finished without a result for this
                            // file (glob-skipped or stopped early).
                            next_file_index += 1;
                            continue;
                        }
                        break;
                    }
                };
                next_file_index += 1;

                stats.merge(scanned.stats);
                progress_stats.merge(scanned.stats);
                let mut consumed = scanned.consumed_matches;
                let mut scanned_matches = scanned.matches.into_iter();
                while consumed > 0 {
                    consumed -= 1;
                    match result_budget.consume_match() {
                        SearchMatchBudget::Collect => {
                            matches.push(
                                scanned_matches
                                    .next()
                                    .expect("worker collected every consumed match"),
                            );
                        }
                        SearchMatchBudget::HiddenTruncation | SearchMatchBudget::Exhausted => break,
                    }
                }
                let file_local_truncated =
                    result_budget.is_exhausted() && scanned.consumed_matches > 0;
                if result_budget.is_exhausted() {
                    truncated = true;
                    // Stop spawning new work; workers may finish their
                    // current file but will not start another one.
                    stop_spawning.store(true, Ordering::Relaxed);
                    merging_complete = true;
                }
                if should_emit_file_search_progress(
                    progress_stats,
                    scanned.stats.matched_files > 0,
                    file_local_truncated,
                ) {
                    emit_project_search_progress(
                        &matches,
                        &mut emitted_matches,
                        progress_stats,
                        truncated,
                        on_progress,
                    );
                    progress_stats = SearchStats::default();
                }
            }
            if next_file_index >= files.len() {
                merging_complete = true;
            }
            if merging_complete {
                pending.clear();
            }
        }
        // The flag may have been raised while the final messages drained;
        // honour it like a loop-top check before returning results.
        if worker_cancelled.load(Ordering::Relaxed) {
            cancelled = true;
        }

        if cancelled {
            outcome = None;
            return;
        }
        truncated |= matches.len() > options.max_results;
        matches.truncate(options.max_results);
        emit_project_search_progress(
            &matches,
            &mut emitted_matches,
            progress_stats,
            truncated,
            on_progress,
        );
        outcome = Some(SearchResult {
            matches,
            truncated,
            error: None,
            stats,
        });
    });

    outcome
}

fn run_project_search_chunk(job: ProjectSearchChunkJob<'_>) {
    let ProjectSearchChunkJob {
        chunk_index,
        file_offset,
        files,
        root,
        line_needle,
        options,
        include_globs,
        exclude_globs,
        metadata_cache,
        worker_cancelled,
        stop_spawning,
        sender,
    } = job;
    let is_cancelled = || worker_cancelled.load(Ordering::Relaxed);
    let mut scratch = ProjectSearchScratch::new();
    let mut worker_budget = SearchResultBudget::new(options.max_results);
    let mut cancelled = false;

    for (offset, path) in files.iter().enumerate() {
        if is_cancelled() {
            cancelled = true;
            break;
        }
        if stop_spawning.load(Ordering::Relaxed) || worker_budget.is_exhausted() {
            break;
        }
        if !path_allowed_by_globs(root, path, include_globs, exclude_globs) {
            continue;
        }

        let mut file_matches = Vec::new();
        let mut file_budget = worker_budget.clone();
        let budget_before = file_budget.remaining_until_truncation;
        let file_result = match search_project_live_file(ProjectSearchLiveFileSearch {
            path,
            line_needle,
            options,
            metadata_cache,
            result_budget: &mut file_budget,
            matches: &mut file_matches,
            is_cancelled: &is_cancelled,
            scratch: &mut scratch,
        }) {
            Some(file_result) => file_result,
            None => {
                cancelled = true;
                break;
            }
        };
        worker_budget = file_budget;
        let consumed_matches =
            budget_before.saturating_sub(worker_budget.remaining_until_truncation);
        let _ = sender.send(ProjectSearchWorkerMessage::File {
            file_index: file_offset + offset,
            scanned: ProjectScannedFile {
                matches: file_matches,
                consumed_matches,
                stats: file_result.stats,
            },
        });
    }
    let _ = sender.send(ProjectSearchWorkerMessage::Done {
        chunk_index,
        cancelled,
    });
}

fn emit_project_search_progress(
    matches: &[SearchMatch],
    emitted_matches: &mut usize,
    stats: SearchStats,
    truncated: bool,
    on_progress: &mut dyn FnMut(SearchProgress),
) {
    let new_matches = if *emitted_matches < matches.len() {
        matches[*emitted_matches..].to_vec()
    } else {
        Vec::new()
    };
    *emitted_matches = matches.len();

    if new_matches.is_empty() && stats == SearchStats::default() && !truncated {
        return;
    }

    on_progress(SearchProgress {
        matches: new_matches,
        truncated,
        stats,
    });
}

#[cfg(test)]
fn search_project_index_prepared(
    index: &ProjectSearchIndex,
    options: &SearchOptions,
    prepared: &PreparedProjectSearch<'_>,
    is_cancelled: &dyn Fn() -> bool,
) -> Option<SearchResult> {
    let context = ProjectSearchContext {
        root: index.root(),
        indexed_max_file_bytes: index.max_file_bytes(),
        line_needle: &prepared.line_needle,
        options,
        include_globs: prepared.include_globs.as_ref(),
        exclude_globs: prepared.exclude_globs.as_ref(),
        is_cancelled,
    };

    let mut result_budget = SearchResultBudget::new(options.max_results);
    let mut stats = SearchStats::default();
    let mut truncated = false;
    let mut matches = Vec::with_capacity(result_budget.limit().min(1024));
    for files in index.files.chunks(SEARCH_FILE_CHUNK_SIZE) {
        if (context.is_cancelled)() {
            return None;
        }
        if result_budget.is_exhausted() {
            truncated = true;
            break;
        }
        let result = search_project_index_chunk(&context, files, &mut result_budget, &mut matches)?;
        stats.merge(result.stats);
        truncated |= result.truncated;
    }
    truncated |= matches.len() > options.max_results;
    matches.truncate(options.max_results);

    Some(SearchResult {
        matches,
        truncated,
        error: None,
        stats,
    })
}

#[cfg(test)]
fn reserve_search_text_budget(
    indexed_text_bytes: &mut u64,
    bytes: u64,
    max_text_bytes: u64,
) -> bool {
    if max_text_bytes == 0 {
        return true;
    }

    let Some(next) = indexed_text_bytes.checked_add(bytes) else {
        return false;
    };
    if next > max_text_bytes {
        return false;
    }
    *indexed_text_bytes = next;
    true
}

#[cfg(test)]
fn read_indexed_search_file_content(
    path: &Path,
    max_file_bytes: u64,
    max_text_bytes: u64,
    indexed_text_bytes: &mut u64,
) -> (
    Option<ProjectSearchIndexedFileSignature>,
    ProjectSearchIndexedFileContent,
) {
    let mut max_file_bytes = effective_search_file_byte_limit(max_file_bytes);
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            return indexed_search_open_error_content(path, max_file_bytes);
        }
    };
    let metadata = match file.metadata() {
        Ok(metadata) if metadata.len() > max_file_bytes => {
            return (
                Some(ProjectSearchIndexedFileSignature::from_metadata(&metadata)),
                ProjectSearchIndexedFileContent::TooLarge,
            );
        }
        Ok(metadata) => metadata,
        Err(_) => return (None, ProjectSearchIndexedFileContent::Unreadable),
    };
    let signature = Some(ProjectSearchIndexedFileSignature::from_metadata(&metadata));

    if search_text_budget_exhausted(*indexed_text_bytes, max_text_bytes) {
        return (
            signature,
            ProjectSearchIndexedFileContent::IndexBudgetExceeded,
        );
    }
    if let Some(remaining_budget) =
        remaining_search_text_budget(*indexed_text_bytes, max_text_bytes)
    {
        if metadata.len() > remaining_budget {
            return (
                signature,
                ProjectSearchIndexedFileContent::IndexBudgetExceeded,
            );
        }
        max_file_bytes = max_file_bytes.min(remaining_budget);
    }

    let content = match read_searchable_text_with_metadata(&mut file, max_file_bytes, &metadata) {
        SearchTextRead::Text(text) => {
            let byte_len = u64::try_from(text.len()).unwrap_or(u64::MAX);
            if reserve_search_text_budget(indexed_text_bytes, byte_len, max_text_bytes) {
                ProjectSearchIndexedFileContent::Text { text, byte_len }
            } else {
                ProjectSearchIndexedFileContent::IndexBudgetExceeded
            }
        }
        SearchTextRead::TooLarge => ProjectSearchIndexedFileContent::TooLarge,
        SearchTextRead::BinaryOrInvalid => ProjectSearchIndexedFileContent::BinaryOrInvalid,
        SearchTextRead::Unreadable => ProjectSearchIndexedFileContent::Unreadable,
    };
    (signature, content)
}

#[cfg(test)]
fn indexed_search_open_error_content(
    path: &Path,
    max_file_bytes: u64,
) -> (
    Option<ProjectSearchIndexedFileSignature>,
    ProjectSearchIndexedFileContent,
) {
    match fs::metadata(path) {
        Ok(metadata) if metadata.len() > max_file_bytes => (
            Some(ProjectSearchIndexedFileSignature::from_metadata(&metadata)),
            ProjectSearchIndexedFileContent::TooLarge,
        ),
        Ok(metadata) => (
            Some(ProjectSearchIndexedFileSignature::from_metadata(&metadata)),
            ProjectSearchIndexedFileContent::Unreadable,
        ),
        Err(_) => (None, ProjectSearchIndexedFileContent::Unreadable),
    }
}

#[cfg(test)]
fn search_text_budget_exhausted(indexed_text_bytes: u64, max_text_bytes: u64) -> bool {
    max_text_bytes > 0 && indexed_text_bytes >= max_text_bytes
}

#[cfg(test)]
fn remaining_search_text_budget(indexed_text_bytes: u64, max_text_bytes: u64) -> Option<u64> {
    if max_text_bytes == 0 {
        return None;
    }
    max_text_bytes.checked_sub(indexed_text_bytes)
}

fn effective_search_file_byte_limit(max_file_bytes: u64) -> u64 {
    max_file_bytes.min(MAX_SEARCH_FILE_BYTES)
}

#[cfg(test)]
impl ProjectSearchIndexedFileSignature {
    fn from_metadata(metadata: &fs::Metadata) -> Self {
        Self {
            len: metadata.len(),
            modified_nanos: metadata_modified_nanos(metadata),
            created_nanos: metadata_created_nanos(metadata),
        }
    }
}

#[cfg(test)]
fn current_indexed_file_signature(path: &Path) -> Option<ProjectSearchIndexedFileSignature> {
    fs::metadata(path)
        .ok()
        .map(|metadata| ProjectSearchIndexedFileSignature::from_metadata(&metadata))
}

fn metadata_modified_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn metadata_created_nanos(metadata: &fs::Metadata) -> u128 {
    metadata
        .created()
        .ok()
        .and_then(|created| created.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

#[cfg(test)]
#[derive(Debug)]
struct SearchChunkResult {
    truncated: bool,
    stats: SearchStats,
}

#[derive(Debug, Clone)]
struct SearchResultBudget {
    visible_limit: usize,
    remaining_visible: usize,
    remaining_until_truncation: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMatchBudget {
    Collect,
    HiddenTruncation,
    Exhausted,
}

impl SearchResultBudget {
    fn new(max_results: usize) -> Self {
        Self {
            visible_limit: max_results,
            remaining_visible: max_results,
            remaining_until_truncation: max_results.saturating_add(1).max(1),
        }
    }

    fn limit(&self) -> usize {
        self.visible_limit
    }

    fn is_exhausted(&self) -> bool {
        self.remaining_until_truncation == 0
    }

    fn consume_match(&mut self) -> SearchMatchBudget {
        if self.is_exhausted() {
            return SearchMatchBudget::Exhausted;
        }
        self.remaining_until_truncation -= 1;
        if self.remaining_visible == 0 {
            return SearchMatchBudget::HiddenTruncation;
        }
        self.remaining_visible -= 1;
        SearchMatchBudget::Collect
    }
}

#[cfg(test)]
struct ProjectSearchContext<'a, 'needle> {
    root: &'a Path,
    indexed_max_file_bytes: u64,
    line_needle: &'a LineSearchNeedle<'needle>,
    options: &'a SearchOptions,
    include_globs: Option<&'a GlobSet>,
    exclude_globs: Option<&'a GlobSet>,
    is_cancelled: &'a dyn Fn() -> bool,
}

struct ProjectSearchScratch {
    read_buffer: Vec<u8>,
    line_buffer: Vec<u8>,
}

impl ProjectSearchScratch {
    fn new() -> Self {
        Self {
            read_buffer: vec![0; SEARCH_STREAM_BUFFER_BYTES],
            line_buffer: Vec::with_capacity(SEARCH_STREAM_BUFFER_BYTES),
        }
    }

    fn clear_file_state(&mut self) {
        self.line_buffer.clear();
    }
}

#[cfg(test)]
fn search_project_index_chunk(
    context: &ProjectSearchContext<'_, '_>,
    files: &[ProjectSearchIndexedFile],
    result_budget: &mut SearchResultBudget,
    matches: &mut Vec<SearchMatch>,
) -> Option<SearchChunkResult> {
    let mut truncated = false;
    let mut stats = SearchStats::default();

    for file in files {
        if (context.is_cancelled)() {
            return None;
        }
        if result_budget.is_exhausted() {
            truncated = true;
            break;
        }
        let result = match search_project_index_file(context, file, result_budget, matches) {
            FileSearchOutcome::Searched(result) => result,
            FileSearchOutcome::Skipped => continue,
            FileSearchOutcome::Cancelled => return None,
        };
        stats.merge(result.stats);
        // The file scanner stops charging the budget before returning, so an
        // exhausted budget after a file means it hit the truncation point.
        truncated |= result_budget.is_exhausted();

        if result_budget.is_exhausted() {
            truncated = true;
            break;
        }
    }

    Some(SearchChunkResult { truncated, stats })
}

#[derive(Debug)]
struct FileSearchResult {
    stats: SearchStats,
}

#[cfg(test)]
#[derive(Debug)]
enum FileSearchOutcome {
    Searched(FileSearchResult),
    Skipped,
    Cancelled,
}

#[cfg(test)]
impl FileSearchOutcome {
    fn from_result(result: Option<FileSearchResult>) -> Self {
        match result {
            Some(result) => Self::Searched(result),
            None => Self::Cancelled,
        }
    }
}

impl FileSearchResult {
    fn searched(matched_file: bool) -> Self {
        let matched_files = usize::from(matched_file);
        Self {
            stats: SearchStats {
                searched_files: 1,
                matched_files,
                ..SearchStats::default()
            },
        }
    }

    fn skipped_large() -> Self {
        Self::skipped(SearchStats {
            skipped_large_files: 1,
            ..SearchStats::default()
        })
    }

    fn skipped_binary() -> Self {
        Self::skipped(SearchStats {
            skipped_binary_files: 1,
            ..SearchStats::default()
        })
    }

    fn skipped_unreadable() -> Self {
        Self::skipped(SearchStats {
            skipped_unreadable_files: 1,
            ..SearchStats::default()
        })
    }

    #[cfg(test)]
    fn skipped_index_budget() -> Self {
        Self::skipped(SearchStats {
            skipped_index_budget_files: 1,
            ..SearchStats::default()
        })
    }

    fn skipped(stats: SearchStats) -> Self {
        Self { stats }
    }

    fn metadata_state(&self) -> Option<ProjectSearchFileMetadataState> {
        if self.stats.skipped_large_files > 0 {
            Some(ProjectSearchFileMetadataState::TooLarge)
        } else if self.stats.skipped_binary_files > 0 {
            Some(ProjectSearchFileMetadataState::BinaryOrInvalid)
        } else if self.stats.skipped_unreadable_files > 0 {
            Some(ProjectSearchFileMetadataState::Unreadable)
        } else if self.stats.searched_files > 0 {
            Some(ProjectSearchFileMetadataState::Searchable)
        } else {
            None
        }
    }
}

fn cached_file_result_for_state(state: ProjectSearchFileMetadataState) -> Option<FileSearchResult> {
    match state {
        ProjectSearchFileMetadataState::Searchable => None,
        ProjectSearchFileMetadataState::TooLarge => Some(FileSearchResult::skipped_large()),
        ProjectSearchFileMetadataState::BinaryOrInvalid => Some(FileSearchResult::skipped_binary()),
        ProjectSearchFileMetadataState::Unreadable => Some(FileSearchResult::skipped_unreadable()),
    }
}

fn store_file_metadata_state(
    metadata_cache: Option<&ProjectSearchMetadataCache>,
    path: &Path,
    max_file_bytes: u64,
    signature: SearchFileMetadataSignature,
    state: ProjectSearchFileMetadataState,
) {
    if let Some(metadata_cache) = metadata_cache {
        metadata_cache.insert(path, max_file_bytes, signature, state);
    }
}

struct SearchFileMetadataCheck {
    metadata: fs::Metadata,
    signature: SearchFileMetadataSignature,
    cached_state: Option<ProjectSearchFileMetadataState>,
}

fn check_search_file_metadata(
    path: &Path,
    max_file_bytes: u64,
    metadata_cache: Option<&ProjectSearchMetadataCache>,
) -> Result<SearchFileMetadataCheck, FileSearchResult> {
    let metadata = fs::metadata(path).map_err(|_| FileSearchResult::skipped_unreadable())?;
    let signature = SearchFileMetadataSignature::from_metadata(&metadata);
    let cached_state = metadata_cache.and_then(|cache| cache.get(path, max_file_bytes, signature));
    Ok(SearchFileMetadataCheck {
        metadata,
        signature,
        cached_state,
    })
}

fn progress_stats_file_count(stats: SearchStats) -> usize {
    stats.searched_files.saturating_add(stats.skipped_files())
}

fn should_emit_file_search_progress(
    progress_stats: SearchStats,
    file_matched: bool,
    file_local_truncated: bool,
) -> bool {
    file_matched
        || file_local_truncated
        || progress_stats_file_count(progress_stats) >= SEARCH_FILE_CHUNK_SIZE
}

#[cfg(test)]
fn search_project_index_file(
    context: &ProjectSearchContext<'_, '_>,
    file: &ProjectSearchIndexedFile,
    result_budget: &mut SearchResultBudget,
    matches: &mut Vec<SearchMatch>,
) -> FileSearchOutcome {
    if !path_allowed_by_globs(
        context.root,
        &file.path,
        context.include_globs,
        context.exclude_globs,
    ) {
        return FileSearchOutcome::Skipped;
    }
    let needs_live_read = indexed_search_file_is_stale(file)
        || indexed_search_file_needs_larger_limit_read(
            file,
            context.indexed_max_file_bytes,
            context.options.max_file_bytes,
        );
    if needs_live_read {
        if (context.is_cancelled)() {
            return FileSearchOutcome::Cancelled;
        }
        let content = read_live_search_file_content(&file.path, context.options.max_file_bytes);
        if (context.is_cancelled)() {
            return FileSearchOutcome::Cancelled;
        }
        return FileSearchOutcome::from_result(search_project_file_content(
            &file.path,
            &content,
            context.line_needle,
            context.options,
            result_budget,
            matches,
            context.is_cancelled,
        ));
    }
    if matches!(
        file.content,
        ProjectSearchIndexedFileContent::IndexBudgetExceeded
    ) {
        return FileSearchOutcome::from_result(search_project_file_content(
            &file.path,
            &file.content,
            context.line_needle,
            context.options,
            result_budget,
            matches,
            context.is_cancelled,
        ));
    }
    FileSearchOutcome::from_result(search_project_file_content(
        &file.path,
        &file.content,
        context.line_needle,
        context.options,
        result_budget,
        matches,
        context.is_cancelled,
    ))
}

#[cfg(test)]
fn indexed_search_file_needs_larger_limit_read(
    file: &ProjectSearchIndexedFile,
    indexed_max_file_bytes: u64,
    search_max_file_bytes: u64,
) -> bool {
    effective_search_file_byte_limit(search_max_file_bytes) > indexed_max_file_bytes
        && matches!(file.content, ProjectSearchIndexedFileContent::TooLarge)
}

#[cfg(test)]
fn indexed_search_file_is_stale(file: &ProjectSearchIndexedFile) -> bool {
    current_indexed_file_signature(&file.path) != file.signature
}

#[cfg(test)]
fn read_live_search_file_content(
    path: &Path,
    max_file_bytes: u64,
) -> ProjectSearchIndexedFileContent {
    match read_searchable_text(path, max_file_bytes) {
        SearchTextRead::Text(text) => {
            let byte_len = u64::try_from(text.len()).unwrap_or(u64::MAX);
            ProjectSearchIndexedFileContent::Text { text, byte_len }
        }
        SearchTextRead::TooLarge => ProjectSearchIndexedFileContent::TooLarge,
        SearchTextRead::BinaryOrInvalid => ProjectSearchIndexedFileContent::BinaryOrInvalid,
        SearchTextRead::Unreadable => ProjectSearchIndexedFileContent::Unreadable,
    }
}

#[cfg(test)]
fn search_project_file_content(
    path: &Path,
    content: &ProjectSearchIndexedFileContent,
    line_needle: &LineSearchNeedle<'_>,
    options: &SearchOptions,
    result_budget: &mut SearchResultBudget,
    matches: &mut Vec<SearchMatch>,
    is_cancelled: &dyn Fn() -> bool,
) -> Option<FileSearchResult> {
    match content {
        ProjectSearchIndexedFileContent::Text { text, byte_len } => {
            if *byte_len > options.max_file_bytes {
                return Some(FileSearchResult::skipped_large());
            }
            search_text_file(
                path,
                text,
                line_needle,
                options,
                result_budget,
                matches,
                is_cancelled,
            )
        }
        ProjectSearchIndexedFileContent::TooLarge => Some(FileSearchResult::skipped_large()),
        ProjectSearchIndexedFileContent::BinaryOrInvalid => {
            Some(FileSearchResult::skipped_binary())
        }
        ProjectSearchIndexedFileContent::Unreadable => Some(FileSearchResult::skipped_unreadable()),
        #[cfg(test)]
        ProjectSearchIndexedFileContent::IndexBudgetExceeded => {
            Some(FileSearchResult::skipped_index_budget())
        }
    }
}

#[cfg(test)]
fn search_text_file(
    path: &Path,
    text: &str,
    line_needle: &LineSearchNeedle<'_>,
    options: &SearchOptions,
    result_budget: &mut SearchResultBudget,
    matches: &mut Vec<SearchMatch>,
    is_cancelled: &dyn Fn() -> bool,
) -> Option<FileSearchResult> {
    let mut state = TextFileSearchState::default();
    'lines: for (line_idx, line) in text.lines().enumerate() {
        if line_idx.is_multiple_of(SEARCH_CANCEL_LINE_INTERVAL) && is_cancelled() {
            return None;
        }
        let mut line_search = TextLineSearch {
            path,
            line_needle,
            whole_word: options.whole_word,
            result_budget,
            matches,
            is_cancelled,
            state: &mut state,
        };
        let line_scan = search_text_line(line_idx, line, &mut line_search);
        if !matches!(line_scan, LineMatchScan::Completed) {
            if matches!(line_scan, LineMatchScan::Cancelled) {
                return None;
            }
            if state.cancelled {
                return None;
            }
            break 'lines;
        }
    }
    Some(FileSearchResult::searched(state.matched_file))
}

#[derive(Debug, Default)]
struct TextFileSearchState {
    matched_file: bool,
    cancelled: bool,
    matches_since_cancel_check: usize,
}

struct TextLineSearch<'a, 'needle> {
    path: &'a Path,
    line_needle: &'a LineSearchNeedle<'needle>,
    whole_word: bool,
    result_budget: &'a mut SearchResultBudget,
    matches: &'a mut Vec<SearchMatch>,
    is_cancelled: &'a dyn Fn() -> bool,
    state: &'a mut TextFileSearchState,
}

fn search_text_line(
    line_idx: usize,
    line: &str,
    search: &mut TextLineSearch<'_, '_>,
) -> LineMatchScan {
    let mut column_counter = SearchLineColumnCounter::new(line);
    for_each_line_match_with_cancel(
        line,
        search.line_needle,
        search.whole_word,
        search.is_cancelled,
        |byte_col| {
            search.state.matches_since_cancel_check =
                search.state.matches_since_cancel_check.saturating_add(1);
            if search.state.matches_since_cancel_check >= SEARCH_CANCEL_MATCH_INTERVAL {
                search.state.matches_since_cancel_check = 0;
                if (search.is_cancelled)() {
                    search.state.cancelled = true;
                    return false;
                }
            }
            match search.result_budget.consume_match() {
                SearchMatchBudget::Collect => {}
                SearchMatchBudget::HiddenTruncation => {
                    search.state.matched_file = true;
                    return false;
                }
                SearchMatchBudget::Exhausted => {
                    return false;
                }
            }
            search.state.matched_file = true;
            let column = column_counter.column_for_byte(byte_col);
            search.matches.push(SearchMatch {
                path: search.path.to_path_buf(),
                line: line_idx.saturating_add(1),
                column,
                preview: search_preview(line, byte_col),
            });
            true
        },
    )
}

struct ProjectSearchLiveFileSearch<'a, 'needle> {
    path: &'a Path,
    line_needle: &'a LineSearchNeedle<'needle>,
    options: &'a SearchOptions,
    metadata_cache: Option<&'a ProjectSearchMetadataCache>,
    result_budget: &'a mut SearchResultBudget,
    matches: &'a mut Vec<SearchMatch>,
    is_cancelled: &'a dyn Fn() -> bool,
    scratch: &'a mut ProjectSearchScratch,
}

fn search_project_live_file(
    search: ProjectSearchLiveFileSearch<'_, '_>,
) -> Option<FileSearchResult> {
    let ProjectSearchLiveFileSearch {
        path,
        line_needle,
        options,
        metadata_cache,
        result_budget,
        matches,
        is_cancelled,
        scratch,
    } = search;
    let max_file_bytes = effective_search_file_byte_limit(options.max_file_bytes);
    let metadata_check = match check_search_file_metadata(path, max_file_bytes, metadata_cache) {
        Ok(metadata_check) => metadata_check,
        Err(result) => return Some(result),
    };
    if let Some(result) = metadata_check
        .cached_state
        .and_then(cached_file_result_for_state)
    {
        return Some(result);
    }
    if metadata_check.metadata.is_file() && metadata_check.metadata.len() > max_file_bytes {
        store_file_metadata_state(
            metadata_cache,
            path,
            max_file_bytes,
            metadata_check.signature,
            ProjectSearchFileMetadataState::TooLarge,
        );
        return Some(FileSearchResult::skipped_large());
    }

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            store_file_metadata_state(
                metadata_cache,
                path,
                max_file_bytes,
                metadata_check.signature,
                ProjectSearchFileMetadataState::Unreadable,
            );
            return Some(file_search_result_from_open_error(path, max_file_bytes));
        }
    };

    let result = search_streaming_text_file(StreamingTextFileSearch {
        path,
        file: &mut file,
        max_file_bytes,
        line_needle,
        whole_word: options.whole_word,
        result_budget,
        matches,
        is_cancelled,
        scratch,
    });
    if let Some(result) = &result
        && let Some(state) = result.metadata_state()
    {
        store_file_metadata_state(
            metadata_cache,
            path,
            max_file_bytes,
            metadata_check.signature,
            state,
        );
    }
    result
}

fn file_search_result_from_open_error(path: &Path, max_file_bytes: u64) -> FileSearchResult {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > max_file_bytes => {
            FileSearchResult::skipped_large()
        }
        _ => FileSearchResult::skipped_unreadable(),
    }
}

struct StreamingTextFileSearch<'a, 'needle> {
    path: &'a Path,
    file: &'a mut fs::File,
    max_file_bytes: u64,
    line_needle: &'a LineSearchNeedle<'needle>,
    whole_word: bool,
    result_budget: &'a mut SearchResultBudget,
    matches: &'a mut Vec<SearchMatch>,
    is_cancelled: &'a dyn Fn() -> bool,
    scratch: &'a mut ProjectSearchScratch,
}

fn search_streaming_text_file(search: StreamingTextFileSearch<'_, '_>) -> Option<FileSearchResult> {
    let StreamingTextFileSearch {
        path,
        file,
        max_file_bytes,
        line_needle,
        whole_word,
        result_budget,
        matches,
        is_cancelled,
        scratch,
    } = search;
    scratch.clear_file_state();
    let mut bytes_read = 0u64;
    let mut line_idx = 0usize;
    let mut state = TextFileSearchState::default();
    let mut local_budget = result_budget.clone();
    let mut local_matches = Vec::new();
    let mut search_stopped = false;

    loop {
        if is_cancelled() {
            return None;
        }

        let read_limit = max_file_bytes.saturating_add(1).saturating_sub(bytes_read);
        if read_limit == 0 {
            return Some(FileSearchResult::skipped_large());
        }
        let read_cap = usize::try_from(read_limit)
            .unwrap_or(usize::MAX)
            .min(scratch.read_buffer.len());
        let read_len = match file.read(&mut scratch.read_buffer[..read_cap]) {
            Ok(read_len) => read_len,
            Err(_) => return Some(FileSearchResult::skipped_unreadable()),
        };

        if read_len == 0 {
            if !scratch.line_buffer.is_empty() {
                if search_stopped {
                    if !streamed_line_is_valid_utf8(scratch, false) {
                        return Some(FileSearchResult::skipped_binary());
                    }
                } else {
                    let mut line_search = TextLineSearch {
                        path,
                        line_needle,
                        whole_word,
                        result_budget: &mut local_budget,
                        matches: &mut local_matches,
                        is_cancelled,
                        state: &mut state,
                    };
                    match search_streamed_line(line_idx, false, scratch, &mut line_search) {
                        StreamedLineSearch::Completed => {}
                        StreamedLineSearch::StopFile => {}
                        StreamedLineSearch::Cancelled => return None,
                        StreamedLineSearch::BinaryOrInvalid => {
                            return Some(FileSearchResult::skipped_binary());
                        }
                    }
                }
            }
            break;
        }

        bytes_read = bytes_read.saturating_add(u64::try_from(read_len).unwrap_or(u64::MAX));
        if bytes_read > max_file_bytes {
            return Some(FileSearchResult::skipped_large());
        }

        if scratch.read_buffer[..read_len].contains(&0) {
            return Some(FileSearchResult::skipped_binary());
        }

        let mut chunk_start = 0usize;
        while let Some(relative_newline) = scratch.read_buffer[chunk_start..read_len]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let newline = chunk_start.saturating_add(relative_newline);
            scratch
                .line_buffer
                .extend_from_slice(&scratch.read_buffer[chunk_start..newline]);
            if search_stopped {
                if !streamed_line_is_valid_utf8(scratch, true) {
                    return Some(FileSearchResult::skipped_binary());
                }
            } else {
                let mut line_search = TextLineSearch {
                    path,
                    line_needle,
                    whole_word,
                    result_budget: &mut local_budget,
                    matches: &mut local_matches,
                    is_cancelled,
                    state: &mut state,
                };
                match search_streamed_line(line_idx, true, scratch, &mut line_search) {
                    StreamedLineSearch::Completed => {}
                    StreamedLineSearch::StopFile => {
                        search_stopped = true;
                    }
                    StreamedLineSearch::Cancelled => return None,
                    StreamedLineSearch::BinaryOrInvalid => {
                        return Some(FileSearchResult::skipped_binary());
                    }
                }
            }
            scratch.line_buffer.clear();
            line_idx = line_idx.saturating_add(1);
            chunk_start = newline.saturating_add(1);
        }
        scratch
            .line_buffer
            .extend_from_slice(&scratch.read_buffer[chunk_start..read_len]);
    }

    commit_streamed_file_search(result_budget, matches, local_budget, &mut local_matches);
    Some(FileSearchResult::searched(state.matched_file))
}

fn commit_streamed_file_search(
    result_budget: &mut SearchResultBudget,
    matches: &mut Vec<SearchMatch>,
    local_budget: SearchResultBudget,
    local_matches: &mut Vec<SearchMatch>,
) {
    *result_budget = local_budget;
    matches.append(local_matches);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamedLineSearch {
    Completed,
    StopFile,
    Cancelled,
    BinaryOrInvalid,
}

fn search_streamed_line(
    line_idx: usize,
    line_ended_by_newline: bool,
    scratch: &ProjectSearchScratch,
    search: &mut TextLineSearch<'_, '_>,
) -> StreamedLineSearch {
    if line_idx.is_multiple_of(SEARCH_CANCEL_LINE_INTERVAL) && (search.is_cancelled)() {
        return StreamedLineSearch::Cancelled;
    }
    let line_bytes = streamed_line_bytes(&scratch.line_buffer, line_ended_by_newline);
    let Ok(line) = std::str::from_utf8(line_bytes) else {
        return StreamedLineSearch::BinaryOrInvalid;
    };
    match search_text_line(line_idx, line, search) {
        LineMatchScan::Completed => StreamedLineSearch::Completed,
        LineMatchScan::Cancelled => StreamedLineSearch::Cancelled,
        LineMatchScan::Stopped if search.state.cancelled => StreamedLineSearch::Cancelled,
        LineMatchScan::Stopped => StreamedLineSearch::StopFile,
    }
}

fn streamed_line_bytes(line: &[u8], line_ended_by_newline: bool) -> &[u8] {
    if line_ended_by_newline && line.ends_with(b"\r") {
        &line[..line.len().saturating_sub(1)]
    } else {
        line
    }
}

fn streamed_line_is_valid_utf8(
    scratch: &ProjectSearchScratch,
    line_ended_by_newline: bool,
) -> bool {
    std::str::from_utf8(streamed_line_bytes(
        &scratch.line_buffer,
        line_ended_by_newline,
    ))
    .is_ok()
}

struct SearchLineColumnCounter<'a> {
    line: &'a str,
    line_is_ascii: Option<bool>,
    previous_byte: usize,
    previous_chars: usize,
}

impl<'a> SearchLineColumnCounter<'a> {
    fn new(line: &'a str) -> Self {
        Self {
            line,
            line_is_ascii: None,
            previous_byte: 0,
            previous_chars: 0,
        }
    }

    fn column_for_byte(&mut self, byte_offset: usize) -> usize {
        let line_is_ascii = match self.line_is_ascii {
            Some(line_is_ascii) => line_is_ascii,
            None => {
                let line_is_ascii = self.line.is_ascii();
                self.line_is_ascii = Some(line_is_ascii);
                line_is_ascii
            }
        };
        if line_is_ascii {
            return byte_offset.min(self.line.len()).saturating_add(1);
        }
        let byte_offset = floor_char_boundary(self.line, byte_offset);

        self.previous_chars = char_count_to_byte_offset(
            self.line,
            self.previous_byte,
            self.previous_chars,
            byte_offset,
        );
        self.previous_byte = byte_offset;
        self.previous_chars.saturating_add(1)
    }
}

fn char_count_to_byte_offset(
    text: &str,
    previous_byte: usize,
    previous_chars: usize,
    byte_offset: usize,
) -> usize {
    let previous_byte = floor_char_boundary(text, previous_byte);
    let byte_offset = floor_char_boundary(text, byte_offset);
    if byte_offset >= previous_byte {
        previous_chars.saturating_add(char_count_fast(&text[previous_byte..byte_offset]))
    } else {
        char_count_fast(&text[..byte_offset])
    }
}

fn char_count_fast(text: &str) -> usize {
    let bytes = text.as_bytes();
    let Some(first_non_ascii) = bytes.iter().position(|byte| !byte.is_ascii()) else {
        return bytes.len();
    };
    first_non_ascii + text[first_non_ascii..].chars().count()
}

#[cfg(test)]
enum SearchTextRead {
    Text(String),
    TooLarge,
    BinaryOrInvalid,
    Unreadable,
}

#[cfg(test)]
fn read_searchable_text(path: &Path, max_file_bytes: u64) -> SearchTextRead {
    let max_file_bytes = effective_search_file_byte_limit(max_file_bytes);
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return search_text_open_error(path, max_file_bytes),
    };
    let metadata = match file.metadata() {
        Ok(metadata) => metadata,
        Err(_) => return SearchTextRead::Unreadable,
    };
    read_searchable_text_with_metadata(&mut file, max_file_bytes, &metadata)
}

#[cfg(test)]
fn search_text_open_error(path: &Path, max_file_bytes: u64) -> SearchTextRead {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() > max_file_bytes => {
            SearchTextRead::TooLarge
        }
        _ => SearchTextRead::Unreadable,
    }
}

#[cfg(test)]
fn read_searchable_text_with_metadata(
    file: &mut fs::File,
    max_file_bytes: u64,
    metadata: &fs::Metadata,
) -> SearchTextRead {
    let max_file_bytes = effective_search_file_byte_limit(max_file_bytes);
    if metadata.is_file() && metadata.len() > max_file_bytes {
        return SearchTextRead::TooLarge;
    }
    let Ok(bytes) = read_file_prefix(file, max_file_bytes) else {
        return SearchTextRead::Unreadable;
    };
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max_file_bytes {
        return SearchTextRead::TooLarge;
    }
    if bytes.contains(&0) {
        return SearchTextRead::BinaryOrInvalid;
    }
    match String::from_utf8(bytes) {
        Ok(text) => SearchTextRead::Text(text),
        Err(_) => SearchTextRead::BinaryOrInvalid,
    }
}

#[cfg(test)]
fn read_file_prefix(file: &mut fs::File, max_file_bytes: u64) -> io::Result<Vec<u8>> {
    let limit = max_file_bytes.saturating_add(1);
    let capacity = usize::try_from(limit.min(64 * 1024)).unwrap_or(64 * 1024);
    let mut reader = file.take(limit);
    let mut bytes = Vec::with_capacity(capacity);
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn build_glob_set(patterns: &[String]) -> Result<Option<GlobSet>, String> {
    let mut builder = GlobSetBuilder::new();
    let mut added = HashSet::with_capacity(
        patterns
            .len()
            .min(MAX_SEARCH_GLOB_PATTERNS)
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
        if pattern_count > MAX_SEARCH_GLOB_PATTERNS {
            return Err(format_too_many_glob_patterns_error());
        }
        if pattern.len() > MAX_SEARCH_GLOB_PATTERN_BYTES {
            return Err(format_oversized_glob_pattern_error(pattern));
        }
        has_patterns = true;
        add_glob_pattern(&mut builder, &mut added, pattern)?;
        if is_bare_glob_pattern(pattern) {
            add_bare_glob_pattern_variants(&mut builder, &mut added, pattern)?;
        }
    }
    if !has_patterns {
        return Ok(None);
    }

    builder
        .build()
        .map(Some)
        .map_err(format_invalid_glob_build_error)
}

fn is_bare_glob_pattern(pattern: &str) -> bool {
    !pattern.contains(['/', '\\']) && !pattern.starts_with("**")
}

fn add_glob_pattern(
    builder: &mut GlobSetBuilder,
    added: &mut HashSet<String>,
    pattern: &str,
) -> Result<(), String> {
    if !added.insert(pattern.to_owned()) {
        return Ok(());
    }
    let glob =
        Glob::new(pattern).map_err(|error| format_invalid_glob_pattern_error(pattern, error))?;
    builder.add(glob);
    Ok(())
}

fn add_bare_glob_pattern_variants(
    builder: &mut GlobSetBuilder,
    added: &mut HashSet<String>,
    pattern: &str,
) -> Result<(), String> {
    let mut descendant_pattern = String::with_capacity(pattern.len() + 6);
    descendant_pattern.push_str("**/");
    descendant_pattern.push_str(pattern);
    add_glob_pattern(builder, added, &descendant_pattern)?;

    descendant_pattern.push_str("/**");
    add_glob_pattern(builder, added, &descendant_pattern)
}

fn format_too_many_glob_patterns_error() -> String {
    format!("Too many glob patterns; maximum is {MAX_SEARCH_GLOB_PATTERNS}")
}

fn format_oversized_glob_pattern_error(pattern: &str) -> String {
    format!(
        "Glob pattern `{}` is too long; maximum is {MAX_SEARCH_GLOB_PATTERN_BYTES} bytes",
        sanitize_glob_error_text(pattern, GLOB_ERROR_PATTERN_MAX_CHARS)
    )
}

fn format_invalid_glob_pattern_error(pattern: &str, detail: impl fmt::Display) -> String {
    format!(
        "Invalid glob `{}`: {}",
        sanitize_glob_error_text(pattern, GLOB_ERROR_PATTERN_MAX_CHARS),
        sanitize_glob_error_text(&detail.to_string(), GLOB_ERROR_DETAIL_MAX_CHARS)
    )
}

fn format_invalid_glob_build_error(detail: impl fmt::Display) -> String {
    format!(
        "Invalid glob pattern: {}",
        sanitize_glob_error_text(&detail.to_string(), GLOB_ERROR_DETAIL_MAX_CHARS)
    )
}

fn sanitize_glob_error_text(text: &str, max_chars: usize) -> String {
    let mut sanitized = String::with_capacity(text.len().min(max_chars));
    let mut used_chars = 0usize;
    for ch in text.chars() {
        let Some(fragment_chars) = sanitized_glob_error_escape_len(ch) else {
            if used_chars.saturating_add(1) > max_chars {
                append_truncation_marker(&mut sanitized, max_chars, &mut used_chars);
                return sanitized;
            }
            sanitized.push(ch);
            used_chars += 1;
            continue;
        };
        if used_chars.saturating_add(fragment_chars) > max_chars {
            append_truncation_marker(&mut sanitized, max_chars, &mut used_chars);
            return sanitized;
        }
        push_sanitized_glob_error_escape(&mut sanitized, ch);
        used_chars += fragment_chars;
    }
    sanitized
}

fn sanitized_glob_error_escape_len(ch: char) -> Option<usize> {
    match ch {
        '\n' | '\r' | '\t' => Some(2),
        ch if ch.is_control() || is_bidi_control(ch) => Some(unicode_escape_len(ch)),
        _ => None,
    }
}

fn unicode_escape_len(ch: char) -> usize {
    4 + hex_digit_count(u32::from(ch)).max(4)
}

fn hex_digit_count(mut value: u32) -> usize {
    let mut digits = 1;
    while value >= 16 {
        value /= 16;
        digits += 1;
    }
    digits
}

fn push_sanitized_glob_error_escape(output: &mut String, ch: char) {
    match ch {
        '\n' => output.push_str("\\n"),
        '\r' => output.push_str("\\r"),
        '\t' => output.push_str("\\t"),
        ch if ch.is_control() || is_bidi_control(ch) => {
            let _ = write!(output, "\\u{{{:04x}}}", u32::from(ch));
        }
        _ => {}
    }
}

fn append_truncation_marker(text: &mut String, max_chars: usize, used_chars: &mut usize) {
    const MARKER: &str = "...";
    const MARKER_CHARS: usize = MARKER.len();

    if max_chars <= MARKER_CHARS {
        text.clear();
        for _ in 0..max_chars {
            text.push('.');
        }
        *used_chars = max_chars;
        return;
    }

    while used_chars.saturating_add(MARKER_CHARS) > max_chars {
        if text.pop().is_none() {
            break;
        }
        *used_chars = used_chars.saturating_sub(1);
    }
    text.push_str(MARKER);
    *used_chars += MARKER_CHARS;
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn path_allowed_by_globs(
    root: &Path,
    path: &Path,
    include_globs: Option<&GlobSet>,
    exclude_globs: Option<&GlobSet>,
) -> bool {
    if include_globs.is_none() && exclude_globs.is_none() {
        return true;
    }

    let relative = path.strip_prefix(root).unwrap_or(path);
    let file_name = path.file_name().map(Path::new);
    if let Some(include_globs) = include_globs
        && !include_globs.is_match(relative)
        && !file_name.is_some_and(|name| include_globs.is_match(name))
    {
        return false;
    }
    if let Some(exclude_globs) = exclude_globs
        && (exclude_globs.is_match(relative)
            || file_name.is_some_and(|name| exclude_globs.is_match(name)))
    {
        return false;
    }

    true
}

#[derive(Debug, Clone)]
enum LineSearchNeedle<'a> {
    CaseSensitive(&'a str),
    CaseInsensitive(AsciiCaseInsensitiveMatcher<'a>),
    Regex(Regex),
}

impl<'a> LineSearchNeedle<'a> {
    fn new(needle: &'a str, case_sensitive: bool) -> Self {
        if case_sensitive {
            Self::CaseSensitive(needle)
        } else {
            Self::CaseInsensitive(AsciiCaseInsensitiveMatcher::new(needle))
        }
    }

    fn prepare(needle: &'a str, case_sensitive: bool, regex: bool) -> Result<Self, String> {
        if regex {
            Ok(Self::Regex(build_search_line_regex(
                needle,
                case_sensitive,
            )?))
        } else {
            Ok(Self::new(needle, case_sensitive))
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::CaseSensitive(needle) => needle.len(),
            Self::CaseInsensitive(matcher) => matcher.needle_len(),
            // The regex scanner never consults a needle length; the literal
            // search loop below only dispatches on the non-regex variants.
            Self::Regex(_) => 0,
        }
    }

    fn find_next(&self, haystack: &str, search_from: usize) -> Option<usize> {
        match self {
            Self::CaseSensitive(needle) => {
                if search_from > haystack.len() {
                    return None;
                }
                let search_from = ceil_char_boundary(haystack, search_from);
                haystack[search_from..]
                    .find(needle)
                    .map(|offset| search_from + offset)
            }
            Self::CaseInsensitive(matcher) => matcher.find_from(haystack, search_from),
            Self::Regex(_) => None,
        }
    }
}

fn build_search_line_regex(query: &str, case_sensitive: bool) -> Result<Regex, String> {
    if !regex_query_is_line_local(query) {
        return Err(REGEX_MULTILINE_QUERY_ERROR.to_owned());
    }
    // Mirrors buffer find: line-local regex matching with the case
    // sensitivity folded into a single compilation per search.
    RegexBuilder::new(query)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| {
            format!(
                "Invalid regular expression: {}",
                sanitize_glob_error_text(&error.to_string(), GLOB_ERROR_DETAIL_MAX_CHARS)
            )
        })
}

#[cfg(test)]
fn for_each_line_match<F>(
    line: &str,
    needle: &str,
    case_sensitive: bool,
    whole_word: bool,
    on_match: F,
) -> bool
where
    F: FnMut(usize) -> bool,
{
    let needle = LineSearchNeedle::new(needle, case_sensitive);
    for_each_line_match_with(line, &needle, whole_word, on_match)
}

#[cfg(test)]
fn for_each_line_match_with<F>(
    line: &str,
    needle: &LineSearchNeedle<'_>,
    whole_word: bool,
    mut on_match: F,
) -> bool
where
    F: FnMut(usize) -> bool,
{
    matches!(
        for_each_line_match_with_cancel(
            line,
            needle,
            whole_word,
            || false,
            |byte_col| { on_match(byte_col) }
        ),
        LineMatchScan::Completed
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineMatchScan {
    Completed,
    Stopped,
    Cancelled,
}

fn for_each_line_match_with_cancel<F, C>(
    line: &str,
    needle: &LineSearchNeedle<'_>,
    whole_word: bool,
    mut is_cancelled: C,
    mut on_match: F,
) -> LineMatchScan
where
    F: FnMut(usize) -> bool,
    C: FnMut() -> bool,
{
    if let LineSearchNeedle::Regex(regex) = needle {
        return for_each_regex_line_match_with_cancel(
            line,
            regex,
            whole_word,
            is_cancelled,
            on_match,
        );
    }
    let needle_len = needle.len();
    if needle_len == 0 {
        return LineMatchScan::Completed;
    }

    let mut search_from = 0;
    let cancel_byte_interval = SEARCH_CANCEL_BYTE_INTERVAL;
    let mut next_cancel_check = cancel_byte_interval.min(line.len());
    let mut whole_word_matcher = SearchLineWholeWordMatcher::new(line, whole_word);
    while search_from <= line.len() {
        if search_from >= next_cancel_check {
            if is_cancelled() {
                return LineMatchScan::Cancelled;
            }
            next_cancel_check = next_cancel_check
                .max(search_from)
                .saturating_add(cancel_byte_interval)
                .min(line.len());
        }

        let search_limit = line_match_search_limit(line, needle_len, next_cancel_check);
        let haystack = &line[..search_limit];
        let Some(start) = needle.find_next(haystack, search_from) else {
            if search_limit == line.len() {
                break;
            }
            search_from = next_line_match_search_start(line, search_limit, needle_len);
            continue;
        };
        let end = start + needle_len;
        if whole_word_matcher.is_match(start, end) && !on_match(start) {
            return LineMatchScan::Stopped;
        }
        search_from = end.max(start + 1);
    }
    LineMatchScan::Completed
}

fn for_each_regex_line_match_with_cancel<F, C>(
    line: &str,
    regex: &Regex,
    whole_word: bool,
    mut is_cancelled: C,
    mut on_match: F,
) -> LineMatchScan
where
    F: FnMut(usize) -> bool,
    C: FnMut() -> bool,
{
    let mut whole_word_matcher = SearchLineWholeWordMatcher::new(line, whole_word);
    let mut matches_since_cancel_check = 0usize;
    for matched in regex.find_iter(line) {
        // Zero-width matches have no highlightable column, mirroring the
        // buffer find scanner which skips empty matches.
        if matched.is_empty() {
            continue;
        }
        matches_since_cancel_check = matches_since_cancel_check.saturating_add(1);
        if matches_since_cancel_check >= SEARCH_CANCEL_MATCH_INTERVAL {
            matches_since_cancel_check = 0;
            if is_cancelled() {
                return LineMatchScan::Cancelled;
            }
        }
        if whole_word_matcher.is_match(matched.start(), matched.end()) && !on_match(matched.start())
        {
            return LineMatchScan::Stopped;
        }
    }
    LineMatchScan::Completed
}

fn line_match_search_limit(line: &str, needle_len: usize, next_cancel_check: usize) -> usize {
    if next_cancel_check >= line.len() {
        return line.len();
    }
    let limit = next_cancel_check
        .saturating_add(needle_len.saturating_sub(1))
        .min(line.len());
    ceil_char_boundary(line, limit)
}

fn next_line_match_search_start(line: &str, search_limit: usize, needle_len: usize) -> usize {
    if search_limit >= line.len() {
        return line.len();
    }
    ceil_char_boundary(
        line,
        search_limit.saturating_sub(needle_len).saturating_add(1),
    )
}

fn ceil_char_boundary(text: &str, mut byte_idx: usize) -> usize {
    byte_idx = byte_idx.min(text.len());
    while byte_idx < text.len() && !text.is_char_boundary(byte_idx) {
        byte_idx += 1;
    }
    byte_idx
}

fn floor_char_boundary(text: &str, mut byte_idx: usize) -> usize {
    byte_idx = byte_idx.min(text.len());
    while byte_idx > 0 && !text.is_char_boundary(byte_idx) {
        byte_idx -= 1;
    }
    byte_idx
}

struct SearchLineWholeWordMatcher<'a> {
    line: &'a str,
    whole_word: bool,
    line_is_ascii: Option<bool>,
}

impl<'a> SearchLineWholeWordMatcher<'a> {
    fn new(line: &'a str, whole_word: bool) -> Self {
        Self {
            line,
            whole_word,
            line_is_ascii: None,
        }
    }

    fn is_match(&mut self, start: usize, end: usize) -> bool {
        if !self.whole_word {
            return true;
        }

        let line_is_ascii = match self.line_is_ascii {
            Some(line_is_ascii) => line_is_ascii,
            None => {
                let line_is_ascii = self.line.is_ascii();
                self.line_is_ascii = Some(line_is_ascii);
                line_is_ascii
            }
        };
        if line_is_ascii {
            return is_ascii_whole_word_match(self.line.as_bytes(), start, end);
        }

        is_unicode_whole_word_match(self.line, start, end)
    }
}

fn is_ascii_whole_word_match(line: &[u8], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).and_then(|index| line.get(index));
    let after = line.get(end);
    !before.is_some_and(|byte| is_ascii_word_byte(*byte))
        && !after.is_some_and(|byte| is_ascii_word_byte(*byte))
}

fn is_ascii_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_unicode_whole_word_match(line: &str, start: usize, end: usize) -> bool {
    if start > end
        || end > line.len()
        || !line.is_char_boundary(start)
        || !line.is_char_boundary(end)
    {
        return false;
    }
    let before = line[..start].chars().next_back();
    let after = line[end..].chars().next();
    !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char)
}

fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn search_preview(line: &str, match_byte_col: usize) -> String {
    let match_byte_col = floor_char_boundary(line, match_byte_col);
    let trim_start_byte = line.len().saturating_sub(line.trim_start().len());
    let trimmed = line[trim_start_byte..].trim_end();
    if trimmed.len() <= MAX_SEARCH_PREVIEW_CHARS {
        return trimmed.to_owned();
    }

    if trimmed.is_ascii() {
        let match_offset = match_byte_col
            .saturating_sub(trim_start_byte)
            .min(trimmed.len().saturating_sub(1));
        let start = match_offset
            .saturating_sub(SEARCH_PREVIEW_CONTEXT_CHARS)
            .min(trimmed.len().saturating_sub(MAX_SEARCH_PREVIEW_CHARS));
        let end = start
            .saturating_add(MAX_SEARCH_PREVIEW_CHARS)
            .min(trimmed.len());
        let mut preview = String::with_capacity(end.saturating_sub(start).saturating_add(6));
        if start > 0 {
            preview.push_str("...");
        }
        preview.push_str(&trimmed[start..end]);
        if end < trimmed.len() {
            preview.push_str("...");
        }
        return preview;
    }

    let total_chars = trimmed.chars().count();
    if total_chars <= MAX_SEARCH_PREVIEW_CHARS {
        return trimmed.to_owned();
    }
    let match_char = if match_byte_col <= trim_start_byte {
        0
    } else {
        line[trim_start_byte..match_byte_col]
            .chars()
            .count()
            .min(total_chars.saturating_sub(1))
    };
    let start_char = match_char
        .saturating_sub(SEARCH_PREVIEW_CONTEXT_CHARS)
        .min(total_chars.saturating_sub(MAX_SEARCH_PREVIEW_CHARS));
    let end_char = start_char
        .saturating_add(MAX_SEARCH_PREVIEW_CHARS)
        .min(total_chars);
    let mut preview = String::with_capacity(
        end_char
            .saturating_sub(start_char)
            .saturating_mul(4)
            .min(trimmed.len())
            .saturating_add(6),
    );
    if start_char > 0 {
        preview.push_str("...");
    }
    preview.push_str(slice_chars(trimmed, start_char, end_char));
    if end_char < total_chars {
        preview.push_str("...");
    }
    preview
}

fn slice_chars(text: &str, start_char: usize, end_char: usize) -> &str {
    let (start, end) = byte_range_for_char_window(text, start_char, end_char);
    &text[start..end]
}

fn byte_range_for_char_window(text: &str, start_char: usize, end_char: usize) -> (usize, usize) {
    if start_char == 0 && end_char == 0 {
        return (0, 0);
    }

    let mut start = if start_char == 0 { Some(0) } else { None };
    for (char_idx, (byte_idx, _)) in text.char_indices().enumerate() {
        if char_idx == start_char {
            start = Some(byte_idx);
        }
        if char_idx == end_char {
            return (start.unwrap_or(byte_idx), byte_idx);
        }
    }
    let end = text.len();
    (start.unwrap_or(end), end)
}

#[cfg(test)]
mod tests;
