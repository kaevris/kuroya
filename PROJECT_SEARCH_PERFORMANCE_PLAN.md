# Kuroya Project Search Performance Plan

## Purpose

This note documents how Zed handles project search, what is currently hurting
Kuroya, and the implementation path Kuroya should take to become fast on large
workspaces without turning memory usage into the main cost.

The target behavior is simple:

- Index large workspaces without freezing the app.
- Search tens or hundreds of thousands of files with low memory pressure.
- Show first useful results quickly.
- Avoid duplicate background searches wasting CPU and disk I/O.
- Keep correctness for ignored files, binary files, invalid UTF-8, open buffers,
  cancellation, and stale workspace state.

## Sources Checked

- Zed blog, "Nerd-sniped: Project Search", published November 26, 2025:
  https://zed.dev/blog/nerd-sniped-project-search
- Zed current project search source:
  https://github.com/zed-industries/zed/blob/main/crates/project/src/project_search.rs
- Zed current query/matching source:
  https://github.com/zed-industries/zed/blob/main/crates/project/src/search.rs
- Kuroya local project search and indexing code:
  - `crates/kuroya-core/src/search.rs`
  - `crates/kuroya-core/src/project.rs`
  - `crates/kuroya-app/src/project_search.rs`
  - `crates/kuroya-app/src/project_search_panel.rs`
  - `crates/kuroya-app/src/startup_tasks.rs`
  - `crates/kuroya-app/src/project_index_cache.rs`
  - `crates/kuroya-app/src/ui_event_handler/background.rs`

## What Zed Does

Zed does not solve project search by keeping a giant full-text index of every
file, and it does not simply replace its search with ripgrep. Its design is a
multi-stage streaming pipeline.

The core behavior:

1. Build a stream of candidate paths from the project worktree.
2. Apply path filters, settings, ignored-file rules, and include/exclude rules
   before reading file contents.
3. Prefer already-open in-memory buffers. If a file is already open, scan that
   buffer directly because it is the user's current truth.
4. For unopened files, read from the filesystem only to confirm whether the file
   has at least one match.
5. Only files confirmed to contain a match move into the expensive full-match
   stage.
6. Full-match scanning produces the actual match ranges shown in the UI.
7. Results are streamed and capped, not accumulated without bounds.

The important implementation detail is task priority. Zed found that throughput
alone was not the real issue. The bad user experience was first-result latency:
a file with a known match could wait behind a huge queue of candidate scans.

Zed fixed this by making worker tasks prioritize stages explicitly:

1. Full-match scans for files already known to match.
2. First-match confirmation for candidate files.
3. Scanning new paths.

In source, this is done with a biased select loop. The earlier branches get
priority, so useful results are not starved by thousands of "probably no match"
files.

Zed also:

- Uses multiple background workers, based on CPU count.
- Uses bounded channels to avoid uncontrolled memory growth.
- Uses `BufReader` and streaming detection for first-match checks.
- Rejects invalid UTF-8 early.
- Caps result files and match ranges.
- Measures both first-match latency and total throughput in benchmarks.

The lesson for Kuroya is not "copy all of Zed." Kuroya does not need Zed's full
buffer/project model. The lesson is: stream, stage, prioritize, cap, and measure.

## What Kuroya Does Now

Kuroya already has a real project index and search UI, but the production search
path is too expensive for huge workspaces.

Current behavior:

- `ProjectIndex` stores project paths, entries, symbols, root, and truncation
  state.
- Production project search iterates indexed files and reads file contents live
  from disk for each query.
- The content-based `ProjectSearchIndex` exists in `crates/kuroya-core/src/search.rs`,
  but it is test-only and is not used by the normal app.
- The default max file size for project search is 2 MiB.
- For each file, Kuroya may allocate a buffer up to `max_file_bytes + 1`,
  validate text/binary state, then search the loaded text.
- Search progress and result handling are protected against stale UI events, but
  the background work itself can still keep running.

The app UI is not the main bottleneck. The result panel already limits prepared
rows and avoids rendering unlimited results. The real cost is repeated disk
reading, repeated allocations, and duplicate background work.

## Main Problems In Kuroya

### 1. Search reads whole files per query

The biggest problem is that production search loads file content for each query.
On a large workspace, a no-match query can force the app to scan almost every
indexed file. With a 2 MiB per-file limit, this creates heavy disk I/O and memory
allocation pressure even when most files do not match.

This is why large searches can feel slow or hang-like: the app is doing real
work, but it is doing too much of the wrong shape.

### 2. Binary and invalid UTF-8 checks happen too late

Kuroya reads a large prefix before deciding that a file is binary or invalid
UTF-8. That means bad candidates can still cost a large allocation and disk read.

The better model is to inspect early chunks and skip quickly.

### 3. Cancellation is cooperative but not interrupting file reads

Kuroya checks cancellation around chunks of work, but a file read that is already
running is not interrupted. If the user changes the query, the old search may
continue burning CPU and I/O until it reaches the next cancellation point.

### 4. Duplicate searches can run at the same time

`spawn_project_search` can start a new blocking search for each explicit search.
Old searches are marked stale/cancelled, but they can still run in the
background. On a large workspace, this can multiply the load exactly when the
user is trying to recover from a slow search.

### 5. Cached index preview can create search churn

Startup may load a cached project index first, then later publish the freshly
rebuilt index. If the user searches while the cached index is active, the fresh
index can invalidate the result generation and make the app do extra work.

### 6. Indexing has avoidable memory spikes

`ProjectIndex::rebuild_with_signature_inner` and `ProjectIndex::scan_signature`
collect signature entries into vectors and sort them. That is acceptable for
medium workspaces, but it is not the final shape for very large workspaces.

The index also has a hardcoded app cap of 40,000 files, while some internal
tracked-entry/cache paths have separate bounds. A directory-heavy workspace can
still create memory pressure outside just "number of searchable files."

### 7. Filters are too weak for huge real-world roots

The index prunes some common directories, but it still includes hidden paths
because hidden files are enabled. Large generated/cache directories can be
indexed unless ignored elsewhere.

For huge workspaces, Kuroya needs stronger default excludes and user-configurable
workspace search/index filters.

### 8. There is no benchmark suite for this path

The repo has many tests, but no dedicated benchmark harness for project search
and project indexing. Without benchmarks, it is too easy to improve one case
while making another one worse.

## What Kuroya Should Do

### Stage 1: Fix the production search shape

Replace whole-file search reads with a streaming scanner.

Required behavior:

- Use a reusable buffer per worker.
- Read files in chunks or lines instead of loading the whole file into memory.
- Detect binary/invalid UTF-8 early and skip quickly.
- Check cancellation between reads.
- Preserve current search semantics for literal search, case sensitivity,
  whole-word matching, result limits, skipped-large files, skipped-binary files,
  and unreadable files.

This is the highest-impact first change because it attacks the current main
cost directly: repeated full-file allocations and reads.

### Stage 2: Add one-search-at-a-time lifecycle control

Kuroya should keep only one active project search worker at a time.

Required behavior:

- Track the active project search request id.
- If a new query arrives while a search is active, cancel the old request and
  store only the latest pending request.
- When the active search finishes, start the latest pending request if it still
  exists and is still valid.
- Drop stale progress/result events as it already does today.

This prevents old searches from competing with the latest search.

### Stage 3: Split search into candidate and full-result stages

After streaming search is stable, move toward a Zed-style staged pipeline.

Recommended pipeline:

1. Path producer streams indexed files in stable order.
2. Path filter rejects impossible paths before content reads.
3. First-match scanner checks unopened files with streaming reads.
4. Full-result scanner computes match ranges only for files that matched.
5. Result merger reports matches in stable enough order and obeys result caps.

Priority order should be:

1. Full-result scans for files already known to match.
2. First-match confirmations.
3. More path scanning.

This is the change that improves first-result latency on huge workspaces.

### Stage 4: Add lightweight file searchability metadata cache

Do not build a giant full-text cache first. Cache cheap metadata instead.

Useful cached data:

- File size.
- Modified time and creation/change time where available.
- File type/searchability state: text, binary/invalid, too large, unreadable.
- Optional line count or newline index only if later benchmarks prove it helps.

The cache key should include file identity/signature and search settings that
affect searchability. It must invalidate on project index generation changes and
watcher updates.

This avoids repeatedly rediscovering that the same files are binary, too large,
or unreadable.

### Stage 5: Add configurable index and search filters

Move hardcoded search/index limits and excludes into settings.

Recommended settings:

- `project_index_max_files`
- `project_index_exclude_globs`
- `project_search_exclude_globs`
- `project_search_max_file_size_mb`
- `project_search_max_results`

Keep internal safety excludes protected. Users should not accidentally index
internal app state or VCS metadata.

Default excludes should cover common heavy generated/cache directories, while
still respecting `.gitignore`.

### Stage 6: Improve huge-workspace startup behavior

For huge or truncated indexes:

- Do not auto-run restored project search immediately on startup.
- Show that the index is still building or truncated.
- Avoid running search against a cached preview if a fresh index is already
  actively replacing it, unless the user explicitly starts the search.

The goal is to avoid expensive work during app startup and avoid invalidating
fresh results immediately after they appear.

### Stage 7: Add real benchmarks

Add a benchmark harness before deeper algorithm work. Measure both latency and
throughput.

Required benchmark cases:

- Index build on 10k, 40k, and larger synthetic workspaces.
- No-match project search.
- One-match project search where the matching file is early.
- One-match project search where the matching file is late.
- Many-match project search.
- Binary-heavy workspace.
- Large-file-heavy workspace.
- Search cancellation under load.
- Cache validation and cache load.

Metrics to record:

- Time to first result.
- Total search time.
- Files scanned.
- Bytes read.
- Peak memory.
- CPU time.
- Cancel latency.
- Result count.
- Skipped binary/large/unreadable count.

These numbers are the only way to know whether Kuroya is actually improving.

## Concrete First Implementation Slice

The first PR should be small and focused:

1. Add a streaming file scanner in `kuroya-core`.
2. Route production project search through it.
3. Preserve existing public search result behavior.
4. Add tests for:
   - no-match search;
   - matching text search;
   - binary/invalid UTF-8 skip;
   - too-large skip;
   - cancellation during scanning;
   - max result cap.
5. Add basic benchmark/dev measurement code for 10k and 40k file workspaces.

Do not start by adding a full text index. That would trade search time for high
memory and invalidation complexity. Kuroya should first make the normal
filesystem search path efficient.

## Success Criteria

Kuroya is on the right track when:

- A no-match search does not create large memory spikes.
- Cancelling or changing the query stops old work quickly.
- First result appears quickly when a matching file is found.
- Search remains responsive on a 10k-file workspace.
- Large generated/cache directories are not indexed by default.
- Benchmarks show lower bytes read, lower peak memory, and better first-result
  latency after the change.

The immediate goal is not to beat every editor in every benchmark. The immediate
goal is to remove the current architecture problem: production search should not
repeatedly load whole files and should not allow stale background searches to
fight the latest query.
