# Kuroya Architecture Map

## Purpose

This document maps how the current Kuroya worktree operates across search,
workspace lifecycle, buffers, persistence, recovery, settings, file watching,
Git, LSP, tasks, plugins, terminal sessions, background images, and the egui
frame scheduler.

It is based on twelve parallel read-only subsystem audits plus direct
cross-checking of the shared runtime paths. It describes the current dirty
worktree on 2026-07-11, not only the last committed revision.

The document has four goals:

1. Make subsystem ownership and data flow explicit.
2. Separate current behavior from intended invariants.
3. Record confirmed defects without guessing.
4. Define a production-focused implementation order that improves correctness,
   speed, and memory use without a broad rewrite.

The existing `PROJECT_SEARCH_PERFORMANCE_PLAN.md` remains the deeper project
search performance note. This document is the whole-application map.

## Executive Summary

Kuroya is a native Rust editor with a sound basic split:

- `kuroya-core` owns reusable domain logic such as rope buffers, project
  indexing, filesystem search, settings parsing, Git operations, LSP models,
  tasks, and plugins.
- `kuroya-app` owns egui rendering, orchestration, persistence, async workers,
  process lifecycles, request state, and delivery back to the UI thread.

The dominant architectural constraint is that `KuroyaApp` owns nearly all live
mutable state. Feature modules are mostly separate `impl KuroyaApp` blocks.
This keeps control flow direct, but correctness depends on every subsystem
implementing the same lifecycle rules by hand.

The highest-value work is not a rewrite. It is to standardize five contracts:

1. Background completions must never be dropped, and every accepted event must
   wake egui.
2. Every async job must carry one consistent identity: workspace generation,
   request ID, and buffer version where relevant.
3. Workspace replacement must be a two-phase transition that cannot partially
   commit.
4. Search must use a latest-request-wins coordinator instead of queuing blocking
   jobs behind a process-global mutex.
5. Session persistence must be driven by dirty generations, not a full snapshot
   construction every two seconds.

These changes address the real reliability and idle-resource problems while
preserving the existing product and module layout.

## System Shape

```text
OS input / commands / file notifications / child processes
                         |
                         v
                 kuroya-app orchestration
       +-----------------+------------------+
       |                 |                  |
       v                 v                  v
   UI thread       Tokio async tasks   blocking workers / threads
   KuroyaApp       timers + processes  index/search/git/files/PTY/GIF
       ^                 |                  |
       +-----------------+------------------+
                         |
                  UiEvent delivery
                         |
                         v
              validate identity, mutate state,
                    schedule next frame

Domain operations and immutable models live primarily in kuroya-core.
Persistent files live in global app state and per-workspace external buckets.
```

## Ownership Boundaries

| Owner | Current responsibilities | Important sources |
|---|---|---|
| `kuroya-core` | Rope-backed buffers, find/replace, project index, filesystem search, settings schema and validation, Git operations, task/plugin models, LSP protocol models | `crates/kuroya-core/src/` |
| `KuroyaApp` | All live editor/UI state, request IDs, generations, caches, panels, buffers, workspace state, Git/LSP/task/plugin state, save queues | `crates/kuroya-app/src/app_state.rs` |
| Frame runtime | Drain events, terminal and watcher queues, flush timers, dispatch commands, render, choose next repaint | `app_update.rs`, `app_frame_scheduler.rs`, `runtime_ticks.rs` |
| Background workers | File I/O, index/search/Git work, persistence serialization, plugin/task discovery, image decoding | Feature runtime modules under `crates/kuroya-app/src/` |
| Persistence layer | Paths, bounded parsing, quarantine, snapshots, atomic replacement | `persistence*.rs`, `persistence_storage/` |
| External processes | LSP servers, PTY shells, installer/update process | `lsp_client/`, `terminal/`, `update_checker.rs` |

### Current coupling

`KuroyaApp` is a central mutable coordinator with hundreds of fields. That is
not automatically wrong for an immediate-mode UI, but the repeated request
state machines are already diverging:

- Some jobs use `next_request_id`, `active_request_id`, `in_flight_request_id`,
  and a queued flag.
- Some use a workspace event generation plus per-buffer version.
- Project search uses an atomic cancellation generation but no real pending-job
  slot.
- Session saves have one global in-flight root and a map of queued roots.
- LSP has an independent client generation and its own bounded command queue.

The target should be shared lifecycle helpers, not a new framework or actor
system.

## Frame And Event Spine

`KuroyaApp::update` in `app_update.rs` currently performs this order:

1. Apply zoom, close handling, fonts, and theme changes.
2. Attach the egui repaint context to the terminal.
3. Drain up to 512 `UiEvent` values.
4. Dispatch shortcuts and dropped files.
5. Drain terminal output.
6. Drain the filesystem watcher.
7. Flush workspace/plugin refresh debounce state.
8. Run autosave and periodic session persistence.
9. Flush LSP debounce/restart/diagnostic work and update checks.
10. Drain commands.
11. Synchronize and render the background image.
12. Render panels and overlays.
13. Select the next repaint deadline.

### Current delivery model

- The shared UI channel is bounded to 4,096 events.
- Normal `send_ui_event` calls are lossy when the channel is full.
- `send_critical_ui_event` waits up to 100 ms and then is also lossy.
- The sender does not request an egui repaint.
- Terminal and background-image paths hold an egui context and explicitly wake
  the UI, so they behave differently from general background work.
- Idle scheduling can sleep for up to the two-second session interval.

### Required invariant

Every launched stateful job must produce one terminal completion that the UI
will eventually observe. Progress can be coalesced or dropped. Completion
cannot be dropped because it owns cleanup of in-flight state.

The current implementation violates that invariant. A lost index, Git, task,
plugin, file, search, or session completion can leave its subsystem marked in
flight indefinitely. Session save completions currently use the normal lossy
send path, making that failure particularly direct.

## Async Identity Model

The code already contains most of the right guard data, but it is not expressed
as one contract.

### Existing identities

| Identity | Protects against |
|---|---|
| Workspace root | A result for a different folder |
| `workspace_event_generation` | A stale file operation after reset/switch |
| Subsystem request ID | An older index, Git, task, plugin, search, or SCM request |
| Buffer ID | A result for another open document |
| Buffer version | Applying I/O or LSP work over newer edits |
| Index generation | Search/symbol results built from an obsolete file set |
| LSP client generation | Messages from a replaced server process |

### Target job key

Use one small shared value type for app-owned background jobs:

```text
JobKey {
    workspace_generation,
    request_id,
    buffer_id?,
    buffer_version?,
    source_generation?
}
```

The workspace root remains payload/context, but generation is the primary
identity. A completion handler should have one obvious `is_current(JobKey)`
check and one shared begin/finish transition.

## Search Family

Kuroya does not have one search subsystem. It has six related surfaces with
different data sources and performance behavior.

### 1. Project index

`ProjectIndex` in `crates/kuroya-core/src/project.rs` is an immutable, cloneable
snapshot containing root-relative file paths, project entries, symbols, and
truncation state. It does not contain file text.

Current index flow:

1. Startup or a workspace refresh calls `spawn_index` in `startup_tasks.rs`.
2. If an index cache exists, it can be published immediately as a preview.
3. A fresh workspace signature is scanned.
4. If the cache signature matches, the cached index becomes final.
5. Otherwise the workspace is walked again to rebuild the index and symbols.
6. The final index is cached and sent to the UI.
7. Each accepted preview/final index increments both project-index and
   project-search index generations and clears search metadata.

Index, Git, task, and plugin request paths generally allow one active operation
and one collapsed rerun. This is the correct shape and should become the shared
job-state pattern.

Current costs:

- A stale cache causes a signature walk followed by a rebuild walk.
- Symbol extraction is part of rebuilding.
- Index limits and excludes are settings-backed, with internal safety caps.
- A dropped completion can leave index state permanently in flight.

### 2. Quick Open

Quick Open uses:

- `ProjectIndex::files()` as the main candidate set.
- Open buffer paths missing from the index.
- Recent files, navigation history, and query memory as ranking signals.
- Skim fuzzy matching plus filename, recency, open-file, and navigation bonuses.
- A cache keyed by query and all ranking inputs.

The ranking work is synchronous inside overlay rendering on a cache miss. That
is acceptable for modest workspaces but scales linearly with the index and can
block a frame when the configured index limit is large.

Source: `quick_open.rs`, `quick_open/ranking.rs`, `quick_open_overlay.rs`.

### 3. Project Search

Project Search uses indexed paths but reads current file contents during every
query. The production path does not use a persistent full-text index.

Current flow:

1. Query/options/include/exclude edits invalidate the active request.
2. The overlay immediately calls `spawn_project_search` on each control change.
3. The app snapshots the current index, generation, root, settings, and metadata
   cache.
4. A Tokio blocking job waits for a process-global mutex.
5. The core scanner reads indexed files, checks cancellation, and emits progress.
6. A final result event is accepted only when request, root, index generation,
   query, options, filters, and limits still match.
7. Opening a result goes through the generic file-open and selection path.

The metadata cache records whether a file is searchable based on file metadata
and limits. It does not cache file contents or search results.

Confirmed problems:

- Every changed frame can enqueue another blocking job. The mutex serializes
  execution but does not coalesce queued work, so stale jobs can occupy the
  blocking pool and delay the latest query.
- Result activation and keyboard navigation check current inputs but omit the
  index generation, so results from an older file set can remain actionable.
- Search-only settings changes invalidate requests and clear metadata but do
  not start a replacement search, leaving state stale until another user action.
- Production search still pays live filesystem I/O for each query.

Sources: `project_search.rs`, `project_search_state.rs`,
`project_search_panel.rs`, `crates/kuroya-core/src/search.rs`.

### 4. Buffer Find

Local find is rope-backed and does not use the project index or project-search
worker. It supports literal/regex, case, whole-word, selection scope,
replacement, preserve-case, history, highlighting, and navigation.

Current state is application-global rather than per buffer/pane. A single-entry
cache key includes buffer ID, version, length, normalized query, options, and
scope.

Confirmed split-pane defects:

- Rendering different buffers alternately replaces the one-entry cache every
  frame.
- One global active match ordinal is painted in every visible pane, although
  navigation and replacement operate on the active buffer only.

Local literal case-insensitive matching is ASCII-only, and leading/trailing
query whitespace is removed. These are current semantic limitations, not
project-search behavior.

Sources: `buffer_find.rs`, `buffer_find/replace.rs`,
`crates/kuroya-core/src/buffer/find.rs`.

### 5. Workspace Symbols

Workspace symbols prefer LSP results and fall back to symbols extracted into
the project index. Events use current root/path/query guards.

Confirmed defect: a query longer than the accepted bound can leave the surface
showing a searching state while dispatch returns without starting either the
LSP request or index fallback.

### 6. Explorer

The explorer does not render from `ProjectIndex`. It synchronously calls
`fs::read_dir` for expanded directory snapshots and recursively flattens the
visible tree. The cache is cleared by index/workspace refresh paths.

This independence is useful for showing paths outside search eligibility, but
filesystem reads and large-tree flattening currently happen on the UI thread.

### Search target architecture

Introduce a small `SearchCoordinator` owned by `KuroyaApp`:

- `IndexSnapshot { generation, Arc<ProjectIndex> }` is the common file/symbol
  source.
- Project search has exactly one running request and one replaceable pending
  request. Latest request wins.
- Cancellation prevents queued stale work from starting.
- `SearchKey` contains the index generation and every semantic input. Results
  are renderable and actionable only when the full key matches.
- Progress is coalesced by request rather than queued without bound.
- Open-buffer text is searched before disk text so unsaved edits are the source
  of truth.
- Quick Open ranking moves off the frame path once candidate count exceeds a
  configured threshold, using the same generation/key rules.
- Explorer directory reads become async snapshots with explicit loading/error
  state; the visible flatten pass receives a frame budget.

This coordinator should reuse current core search/index logic. It does not need
a full-text in-memory index as its first step.

## Buffer And File Lifecycle

`TextBuffer` in `kuroya-core` owns rope text, version, dirty/read-only state,
selections, and undo/redo history. App code owns panes, views, async I/O, LSP
notifications, file conflicts, image/binary previews, and save queues.

### Open

1. Normalize/deduplicate the path and reserve a pending open.
2. Read off the UI thread with bounded file and image limits.
3. Classify UTF-8, lossy text, binary preview, or image preview.
4. Accept only when workspace root/generation/path still match.
5. Insert or activate the buffer, restore pending view/history state, and notify
   diagnostics/LSP for normal text files.

### Edit

1. Mutate the rope and increment the buffer version.
2. Keep the buffer dirty.
3. Invalidate render/find/diagnostic caches.
4. Debounce diagnostics and LSP `didChange`.

### Save

1. Serialize one save at a time per buffer; retain the latest queued target.
2. Optionally format and clean text.
3. Snapshot the outgoing buffer version.
4. Save the previous disk content to local history when eligible.
5. Atomically write a same-directory temporary file, sync, and replace.
6. Clear dirty state only if the live buffer still has the saved version.

### External changes

- Clean buffers reload asynchronously.
- Dirty buffers retain edits and get an external-change marker.
- One reload runs per buffer and later requests coalesce.
- Completions validate request ID, root, generation, path, version, and force
  mode.

Confirmed edge cases:

- A save can be misclassified as an external change when newer unsaved edits
  exist because watcher events have no self-write identity.
- Open files outside the workspace are not watched.
- Local history can be skipped when the outgoing buffer exceeds the limit even
  if the previous on-disk file was small enough to preserve.
- A protected preview that reloads to identical safe text can return before LSP
  activation is repaired.

Sources: `file_runtime.rs`, `file_io.rs`, `file_save_dispatch.rs`,
`file_reload_runtime.rs`, `ui_file_*_events.rs`, `buffer_lifecycle.rs`.

## Workspace Lifecycle

### Startup

`AppStartupContext::load`:

1. Creates the Tokio runtime and UI event channel.
2. Loads global app state.
3. Selects the newest usable recent workspace or an external empty-workspace
   placeholder.
4. Loads global settings and the workspace session.
5. Constructs terminal state and the workspace watcher.
6. Builds `KuroyaApp`.

`KuroyaApp::new` then restores the session and starts index, Git, task, and
plugin work. LSP starts lazily when eligible files open.

### Switch

The intended flow is save old session -> stop old LSP -> reset old state ->
install new root/trust/watcher -> load settings/session -> start workspace jobs.

The current implementation changes the workspace root, trust, recents, and
watcher before `reset_open_workspace_state`. That reset can return early when a
keybinding-capture restoration cannot be saved. The caller cannot observe the
failure and continues loading the new workspace. This can leave old buffers and
request generations attached to a new root.

### Target two-phase transition

1. **Preflight:** validate target, resolve keybinding capture, finish or queue
   required saves, and build a transition snapshot. Nothing changes roots yet.
2. **Quiesce:** close old LSP and terminal/workspace processes and invalidate old
   job generations.
3. **Commit:** replace root and all workspace-owned state in one non-failing
   operation.
4. **Activate:** construct watcher/terminal, restore session, and start index,
   Git, task, and plugin jobs.
5. **Report:** surface watcher/session/settings failures instead of silently
   converting them to empty state.

`reset_open_workspace_state` should return a result, or all fallible preflight
work should move before root mutation. There should also be a real close-folder
transition back to the placeholder workspace.

Other confirmed lifecycle gaps:

- A directory passed on the command line is treated as a file instead of a
  workspace.
- Recovered dirty buffers do not send LSP `didOpen`.
- Plugin-language discovery can finish after restored files are classified;
  affected buffers are not fully re-opened/reclassified for LSP.

Sources: `app_startup_context.rs`, `app_startup.rs`,
`workspace_lifecycle.rs`, `workspace_reset_state.rs`,
`app_session_restore.rs`, `startup_arguments.rs`.

## Persistence, Sessions, Recovery, And History

### Storage ownership

| Artifact | Owner and location |
|---|---|
| `settings.toml` | Global app-state directory |
| `state.json` | Global recent projects, trust, Vim fallback, themes, font paths |
| `session.json` | External per-workspace state bucket |
| Project index cache | External per-workspace state bucket |
| Session backups | Per-workspace `snapshots/` |
| Workspace snapshots | Per-workspace `workspace-snapshots/` |
| Local file history | Per-workspace `history/`, with an external-file bucket |
| Legacy state | Workspace-local `.kuroya` read/migration fallback |

The external workspace bucket name is derived from normalized workspace
identity. No machine-specific path is stored in source. `KUROYA_STATE_DIR` is
the supported override.

Sources: `persistence_storage/paths.rs`, `persistence_models.rs`.

### Session capture

A session contains open/active files, panes, view state, selections, scrolling,
folds, bounded undo/redo, explorer/panel state, local/project search state,
source control state, navigation, terminal state, recents, and dirty-buffer
recovery text.

Every two seconds in a real workspace, the UI thread builds a
`SessionSaveSnapshot`. Rope snapshots are cheap, but the traversal still scans
buffers, panes, history, and terminal state. A background task materializes
recovery text, serializes JSON, trims to the session size budget, compares with
the existing file, archives the old valid session, and atomically replaces it.

Only one session save runs globally. The latest queued snapshot is retained per
workspace. The next queued root is selected lexicographically.

### Restore

1. Try the current external session.
2. Quarantine corrupt, oversized, or mismatched candidates.
3. Try newest valid external backups.
4. Fall back to legacy current/backups.
5. Restore recovered dirty buffers before clean files.
6. Reopen clean files asynchronously and apply pending view/history/pane state.

### Recovery and history

- Crash recovery is part of normal session persistence, not a separate journal.
- Dirty text has per-buffer, aggregate, and count budgets.
- Workspace snapshots store the same persisted session shape and keep a bounded
  history.
- Local history stores the previous disk contents before a successful save and
  opens revisions read-only.

### Confirmed problems

- There is no dirty generation before session snapshot construction. Even an
  unchanged editor performs the UI-side traversal every two seconds.
- A lost `SessionSaved` or `SessionSaveFailed` event leaves the single global
  save slot occupied forever, disabling subsequent crash-recovery updates.
- The lexicographic queue policy can starve another workspace if the current
  root is continually requeued.
- Failed saves for a switched-away workspace are discarded without retry and
  are not visible in the current workspace status.
- Equivalent Windows path aliases can produce separate workspace buckets
  because bucket identity is not fully canonical/case-folded.
- Startup and workspace-switch session load failures are mostly suppressed.
- Restore labels all skipped recovery entries as oversized even when the reason
  was duplicate path, count budget, aggregate budget, or final session trimming.
- Legacy workspace-local history accepts symlinked snapshot files and follows
  them. A repository-controlled legacy history path can escape its intended
  storage ownership when opened.

### Target persistence model

- Track a monotonic `session_dirty_generation` and the last successfully saved
  generation per workspace.
- Mark it from explicit domain mutations: buffers, panes/views, panels, terminal
  persistence state, navigation, and recovery-relevant settings.
- Do not build a session snapshot when generations match.
- Keep one in-flight save per workspace or use a fair FIFO across workspaces.
- Make completion delivery reliable and retry failures with a bounded,
  centrally configured backoff policy.
- Preserve current size budgets, quarantine, atomic replacement, and backup
  fallback.
- Reject symlinks/reparse-point escapes in all legacy state readers before
  migration or display.

Sources: `app_session.rs`, `runtime_ticks.rs`, `save_lifecycle.rs`,
`persistence_session.rs`, `recovery.rs`, `file_history.rs`,
`workspace_snapshot_runtime.rs`.

## File Watching And Refresh

One recursive `notify` watcher currently watches the active workspace root.
Its callback filters event kinds and app-owned paths, deduplicates paths, and
uses a bounded queue. Overflow or rescan conditions trigger broad recovery.

Drained paths are classified into:

- Settings reload.
- Task configuration/inference reload.
- Plugin discovery debounce.
- Project index refresh debounce.
- Optional Git auto-refresh through the workspace refresh.
- Clean-buffer reload or dirty-buffer external-change marker.

Current correctness guards for per-buffer reload are strong. Current wake and
coverage behavior is not:

- The notify callback does not wake egui, so an external change can wait until
  the next scheduled frame.
- Workspace and plugin debounce deadlines are absent from the central scheduler.
- Global `settings.toml` is normally outside the only watched root.
- Continuous plugin writes can postpone its trailing debounce indefinitely.
- Git refresh is coupled to index-eligible path changes; `.git`, generated, or
  user-excluded changes can leave Git status stale.
- Watcher construction errors are swallowed and the retry path is ineffective
  once no watcher exists.
- Open files outside the workspace are uncovered.

### Target watch service

Use one app-owned watch service with multiple registrations:

1. Workspace content root.
2. Global settings file or parent directory.
3. Resolved Git metadata root, including a parent repository.
4. Explicit external open files where supported.

The callback should emit normalized domain invalidations and wake egui. The
central scheduler must include every debounce deadline. Project refresh and Git
refresh should be separate invalidation domains. Retry watcher construction
with visible state and a bounded policy from central configuration.

Sources: `fs_watcher.rs`, `runtime_ticks/watcher.rs`,
`workspace_state/watched_paths.rs`, `startup_tasks.rs`.

## Settings And App State

`EditorSettings` is a serde-defaulted, versioned TOML schema in `kuroya-core`.
Loading applies migrations, rejects future schemas, bounds input size, validates
and clamps fields, quarantines corrupt input, and can rewrite sanitized values.

The settings panel has a committed `settings` value and a separate draft.
Apply validates a normalized candidate, saves TOML first, then replaces live
settings and synchronizes buffers, terminal, fonts/theme, Vim, LSP, index,
search, and Git. A TOML save failure leaves live settings unchanged.

Important ownership distinctions:

- General editor settings live in global `settings.toml`.
- Recent projects, trusted workspaces, selected/custom theme state, Vim
  fallback, and font paths also have global `state.json` ownership.
- Session-only UI state belongs in `session.json`.
- Installer file associations and Explorer actions are Inno Setup options, not
  runtime settings.

Current background-image selection changes the settings draft. Runtime app
background state synchronizes after Apply or Reload; selection is not a true
whole-app live preview before saving.

Confirmed settings issue: automatic external reload normally cannot see edits
to the global settings file because it is outside the workspace watcher.

Sources: `crates/kuroya-core/src/settings.rs`,
`crates/kuroya-core/src/settings/io.rs`,
`preferences.rs`, `preference_panels/apply.rs`, `persistence.rs`.

## Git And Source Control

Core owns repository discovery, status, mutations, diffs, history, blame,
branches, and stashes. App code owns request IDs, panels, async execution, and
result application.

The primary Git scan allows one active request plus one collapsed rerun. It can
discover the workspace repository, parent repositories, subfolders, or open
editor repositories according to settings. UI operations have additional
request state for branches, history, stashes, hunks, blame, and virtual diffs.

Confirmed problems:

- External Git metadata changes are not reliably watched.
- Scan errors collapse to the same empty snapshot as no repository, hiding the
  difference.
- Non-UTF-8 Git paths are omitted.
- Subfolder discovery stops after a bounded unsorted filesystem sample, making
  the selected repositories nondeterministic in very large folders.
- A cached discovery root can miss a newly created nearer repository.
- The persisted `git_scan_repositories` setting is not used by the current scan
  path.

Sources: `crates/kuroya-core/src/git.rs`, `startup_tasks.rs`,
`source_control_runtime.rs`, `git_diff_runtime.rs`.

## LSP, Tasks, Plugins, And Terminal

### LSP

- One client handle exists per language.
- Each client has a bounded command queue, child process, stdout reader, and
  shutdown watch.
- App-side results generally validate root, client generation, path, buffer ID,
  and version.
- Diagnostics, completion, signature help, format-on-type, restarts, and symbol
  refreshes use scheduler deadlines.

Confirmed problems:

- `try_send` can drop `didOpen` or `didChange` when a client queue is full, with
  no retry or full-document resynchronization marker.
- Recovered dirty buffers are not opened in LSP.
- Plugin-language arrival does not fully repair already opened buffers.

### Tasks and plugins

- Discovery is bounded and uses one active request plus one queued rerun.
- Workspace trust gates process and plugin execution.
- Explicit Wasm plugin commands have memory/fuel limits and a restricted host
  interface.
- Startup/language activation is partly bookkeeping; not every declared
  activation produces runtime behavior.
- Lexical path containment does not prevent symlink/junction escapes for all
  task/plugin-owned paths.

### Terminal

- PTY I/O is split across native threads with bounded output queues.
- Output wakes egui and is drained with per-frame budgets.
- Running sessions continue draining even when the terminal panel is hidden.
- Many sessions can retain native thread stacks, parsers, scrollback, and output
  queue capacity while idle.

Sources: `lsp_client/`, `lsp_runtime.rs`, `workspace_tasks_runtime.rs`,
`plugin_command_runtime.rs`, `plugin_activation_runtime.rs`, `terminal/`,
`terminal_process.rs`.

## Background Images And Idle CPU

Static backgrounds schedule no frames by themselves. GIF backgrounds use a
single-frame request/response pipeline, bounded dimensions, and a minimum frame
interval. The worker blocks between requests.

Current GIF animation pauses when minimized but not merely unfocused. An
unfocused window can therefore continue decoding, uploading textures, and
repainting near the animation frame rate.

Other recurring idle work:

- Session snapshot construction every two seconds.
- LSP child polling every 250 ms per active client.
- Devtools/profiling holds the frame ceiling near 12.5 FPS when enabled.
- Hidden terminal output can force immediate continuous repaint.

The project index, project search, and Git workers are request-driven and should
consume no active CPU when no request is running. Their idle cost is retained
state, not periodic computation.

### Target scheduler policy

- Every producer wakes egui after publishing work.
- Every timer/debounce is registered with the central deadline calculation.
- Window state is a central policy input: active, unfocused, occluded, minimized.
- GIF animation pauses or reduces rate when unfocused according to a setting.
- Hidden terminal sessions use bounded adaptive draining without delaying task
  completion or allowing queue growth.
- Session persistence wakes only when dirty and due.
- Devtools behavior remains explicitly opt-in.

Sources: `app_frame_scheduler.rs`, `app_update.rs`,
`background_image_animation.rs`, `background_image_runtime.rs`,
`terminal/lifecycle.rs`, `lsp_client/runtime.rs`.

## Confirmed Defects By Priority

### P0: Reliability and state ownership

1. **Lossy terminal completions can wedge state machines.** The UI channel can
   drop even critical events. In-flight state is cleared only by those events.
   Session completion uses the normal lossy path, so future crash-recovery saves
   can stop permanently after one dropped event.
2. **Workspace switching can partially commit.** Root/trust/watcher are replaced
   before a fallible reset. A failed keybinding-capture restoration leaves old
   workspace-owned state under the new root.
3. **Legacy local-history symlink escape.** Legacy workspace-controlled history
   entries can follow links outside the intended state directory.

### P1: User-visible correctness and performance

1. Project-search results from an obsolete index generation remain actionable.
2. Project-search typing queues stale blocking jobs instead of retaining only
   the latest request.
3. General background completions and watcher events do not wake egui and can
   appear up to roughly two seconds late while idle.
4. Session snapshot construction performs recurring UI work even when nothing
   changed, and the global queue can be unfair across workspaces.
5. Recovered dirty buffers never send LSP `didOpen`.
6. LSP `didOpen`/`didChange` can be dropped without recovery.
7. Quick Open ranking and explorer filesystem reads can block the UI frame on
   large workspaces.
8. Buffer Find cache and active ordinal are global and incorrect across visible
   split panes.
9. Git status can remain stale for metadata-only or index-excluded changes.

### P2: Completeness and diagnostics

1. Global settings are not automatically watched.
2. Plugin debounce can starve under continuous writes.
3. Directory startup arguments are treated as files.
4. Plugin-language discovery does not fully reclassify existing buffers.
5. Workspace-symbol overlong queries can stay in a false searching state.
6. Git scan errors are indistinguishable from no repository.
7. Background-image selection is not a true unsaved whole-app preview.
8. No close-folder transition exists.
9. Persistence load/save failures are suppressed in several lifecycle paths.

## Recommended Implementation Order

### Phase 1: Make background delivery reliable

Create a shared UI dispatcher that owns event publication and repaint wakeup.
Separate coalescible progress/invalidation traffic from reliable completion
traffic. Convert every in-flight state machine to a shared begin/finish helper
and add saturation tests proving terminal completion cannot be lost.

Do this first because every later subsystem depends on trustworthy completion.

### Phase 2: Make workspace transition atomic

Move all fallible preflight work before root mutation. Make reset/commit
non-failing, invalidate one workspace generation, and start new services only
after commit. Add a regression test for keybinding-capture save failure during a
workspace switch.

### Phase 3: Fix search correctness and lifecycle

Require full `SearchKey` equality for display and activation. Replace the global
mutex queue with one active plus one latest pending request. Trigger replacement
searches after search-setting changes. Add tests for rapid typing, index change,
settings change, cancellation, and clicked-result selection.

### Phase 4: Remove recurring session work

Add session dirty generations, fair workspace save scheduling, reliable
completion, and bounded retries. Keep current atomic writes, backups, quarantine,
and recovery budgets. Add unchanged-idle and cross-workspace fairness tests.

### Phase 5: Unify watching and scheduler deadlines

Watch global settings and Git metadata separately from project content. Wake the
UI from notify callbacks. Register workspace/plugin debounce deadlines in the
scheduler. Add retry state for watcher construction and an OS-notify integration
test where supported.

### Phase 6: Move large-workspace UI work off-frame

Background Quick Open ranking beyond a configured candidate threshold. Load
explorer directory snapshots asynchronously and budget tree flattening. Preserve
the current cache keys and stale-result guards.

### Phase 7: Repair LSP/plugin lifecycle

Guarantee or recover from dropped document synchronization. Open recovered
buffers in LSP. Reclassify and reopen affected buffers after plugin language
registries change.

### Phase 8: Close security and edge-case gaps

Reject legacy symlink/reparse-point escapes, correct recovery skip reasons,
handle folder startup arguments, distinguish Git errors, and add close-folder.

## Testing Matrix

| Area | Required regression coverage |
|---|---|
| Event delivery | Full progress queue plus every completion type; completion arrives and in-flight clears |
| Wake latency | Background event and notify event wake an otherwise idle egui loop promptly |
| Workspace switch | Dirty guard, failed preflight, stale old events, watcher failure, session failure |
| Project search | Rapid typing, cancellation, index generation change, settings change, stale click/F4 rejection |
| Quick Open | Large candidate set, stale index, open files absent from index, target line/column |
| Buffer Find | Two visible panes with different buffers and independent active matches |
| Session | Unchanged idle, dirty generation, crash recovery, channel pressure, fair multi-root queue, shutdown flush |
| Watcher | Overflow, app-owned filtering, settings outside root, Git metadata, external file, debounce deadline |
| File I/O | Save during newer edits, self-write watcher event, external conflict, preview-to-text LSP activation |
| LSP | Queue saturation, full resync, recovered buffer, plugin language arriving after open |
| Security | Legacy symlink/junction/reparse-point escape attempts for history/tasks/plugins/state |
| Performance | Idle CPU, wake latency, first search result, total search, Quick Open frame time, explorer expansion |

## Configuration Rule

Production behavior must not depend on machine-specific paths, IDs, URLs, or
hidden literals. User-facing limits and policies belong in the existing settings
schema. Internal safety bounds should be named, documented, centralized, and
tested. Environment-specific values belong in environment variables or platform
path APIs.

This preserves Kuroya's current production/open-source requirement without
turning every low-level safety cap into an exposed preference.

## Primary Source Index

### Application spine

- `crates/kuroya-app/src/app_state.rs`
- `crates/kuroya-app/src/app_update.rs`
- `crates/kuroya-app/src/app_frame_scheduler.rs`
- `crates/kuroya-app/src/ui_event_channel.rs`
- `crates/kuroya-app/src/ui_event_handler.rs`
- `crates/kuroya-app/src/ui_events.rs`

### Search and explorer

- `crates/kuroya-core/src/project.rs`
- `crates/kuroya-core/src/search.rs`
- `crates/kuroya-app/src/startup_tasks.rs`
- `crates/kuroya-app/src/project_index_cache.rs`
- `crates/kuroya-app/src/project_search.rs`
- `crates/kuroya-app/src/project_search_panel.rs`
- `crates/kuroya-app/src/quick_open.rs`
- `crates/kuroya-app/src/quick_open_overlay.rs`
- `crates/kuroya-app/src/buffer_find.rs`
- `crates/kuroya-app/src/explorer_tree_panel.rs`

### Workspace, files, and persistence

- `crates/kuroya-app/src/app_startup_context.rs`
- `crates/kuroya-app/src/app_startup.rs`
- `crates/kuroya-app/src/workspace_lifecycle.rs`
- `crates/kuroya-app/src/workspace_reset_state.rs`
- `crates/kuroya-app/src/file_runtime.rs`
- `crates/kuroya-app/src/file_save_dispatch.rs`
- `crates/kuroya-app/src/file_reload_runtime.rs`
- `crates/kuroya-app/src/app_session.rs`
- `crates/kuroya-app/src/app_session_restore.rs`
- `crates/kuroya-app/src/persistence_session.rs`
- `crates/kuroya-app/src/persistence_storage/paths.rs`
- `crates/kuroya-app/src/recovery.rs`
- `crates/kuroya-app/src/file_history.rs`

### Services

- `crates/kuroya-app/src/fs_watcher.rs`
- `crates/kuroya-app/src/runtime_ticks/watcher.rs`
- `crates/kuroya-app/src/workspace_state/watched_paths.rs`
- `crates/kuroya-app/src/lsp_client/`
- `crates/kuroya-app/src/lsp_runtime.rs`
- `crates/kuroya-app/src/workspace_tasks_runtime.rs`
- `crates/kuroya-app/src/plugin_command_runtime.rs`
- `crates/kuroya-app/src/plugin_activation_runtime.rs`
- `crates/kuroya-app/src/source_control_runtime.rs`
- `crates/kuroya-app/src/terminal/`
- `crates/kuroya-app/src/background_image_runtime.rs`
- `crates/kuroya-app/src/background_image_animation.rs`

## Bottom Line

Kuroya already has bounded data structures, stale-event guards, atomic
persistence, recovery budgets, and substantial test coverage. The problem is
not that these systems are missing. The problem is that their lifecycle rules
are duplicated and do not form one reliable contract.

The correct next move is to standardize delivery, job identity, and workspace
transition first. Then search, sessions, watching, LSP, and large-workspace UI
paths can be improved independently without introducing another architecture.
