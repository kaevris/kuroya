# Kuroya Blueprint — Architecture Map & the Path to "Lighter Than Zed"

*Compiled 2026-09-11 from six parallel investigations (memory decomposition with live
experiments, binary/dependency audit, startup path, editor render pipeline, services
layer, lightweight-editor research) plus four earlier audits (git pipeline, project
indexing, search performance, overlay bugs) and live measurements from the running app.*

---

## 1. What Kuroya is today (measured, this build)

| Metric | Value | Notes |
|---|---|---|
| Idle CPU | **0.8% of one core** | Frame scheduler parks the app; 2 s session-save heartbeat is the only wake |
| Active CPU | ~1 core (~12% of 8 cores) | Immediate-mode full redraw at ~60 fps while input is happening |
| Working set (idle, project open, wallpaper on) | ~353 MB | Drops to ~15 MB when Windows trims an idle/minimized process |
| Private commit (floor, before any file) | ~268 MB | wgpu/D3D12 runtime + GPU-shared 291 MB are the dominant chunks |
| 500k-line file (47.9 MB text) | **+54.4 MB buffer** (1.14× text size) | Measured with a counting allocator; rendering is virtualized, +~0.1 MB |
| Total system impact incl. GPU-shared | ~560 MB | GPU-shared (291 MB) is real system RAM on the Intel iGPU |
| Release binary | 41.35 MiB (unstripped, no LTO) | Debug 77.6 MiB; debug PDB 443 MB; target dirs 10.3 GB |
| Cold release build | 21 min; incremental app-crate rebuild ~40–60 s | |
| Tests | 937 core + 5,914 app, 0 failures | |

**Context:** Zed idles ~80–128 MB (spends memory for speed; DX11 backend lighter than
Vulkan). Helix idles ~30 MB (TUI — the terminal owns the framebuffer). Sublime ~100 MB.
Notepad++/Scintilla ~13–14 MB (no GPU, no AST, one style byte per char). Lapce idles at
**680 MB with an empty project** — the cautionary tale that Rust + rope is not enough.

**Verdict vs the field:** Kuroya's *architecture* is already Zed-shaped (rope buffers,
virtualized rows, version-keyed syntax checkpoints, scoped git, bounded channels
everywhere, fuel-limited WASM plugins). The gap is not architecture — it is **defaults
and discipline**: the galley cache is off, caches invalidate file-wide per keystroke,
per-frame rebuilds run unconditionally, syntect loads eagerly, and wgpu runs with
factory settings. The measured decomposition proves the weight is the GPU stack plus
fixable slack — the wallpaper, the user's prime suspect, was experimentally exonerated
(~1.45 MB GPU texture).

---

## 2. Where the memory actually goes (private commit ≈ 271 MB + GPU-shared ≈ 291 MB)

| Rank | Component | MB | Evidence |
|---|---|---|---|
| 1 | wgpu/D3D12 runtime + driver heaps | 60–120 private; 150–200 of the GPU-shared | `main.rs:279-283` default NativeOptions; egui-wgpu defaults `HighPerformance`, `AutoVsync`, `Backends::PRIMARY\|GL` |
| 2 | Swapchain + egui buffers/textures (1320×860 @150% DPI) | 60–100 GPU | egui-wgpu renderer buffer growth 2× slack |
| 3 | Allocator retention (startup peak was 402 MB working set) | 40–70 private | Windows heap rarely decommits |
| 4 | syntect full default set — loaded eagerly on UI thread with zero buffers | 15–20 once warmed | `app_startup_state.rs:73`, `syntax.rs:84-97` |
| 5 | Row galley LRU (4,096 × 12–30 KB) | 5 now; 40–120 worst | `editor_row_render_cache.rs:12` |
| 6 | Fonts (embedded 1.4 MB + user stacks ≤16 MB each) | 4–8 | `font_typography.rs:18-54` |
| 7 | Project index + symbols | 2–5 here (150k-file cap = far more) | `project.rs:27,43` |
| 8 | Terminal scrollback (10k rows × 32 B/cell × cols) | 0 now; 38 MB/session worst | `terminal.rs:805-815`, lazy allocation |
| 9 | tokio runtime + notify + blocking pool | 5–10 | 8 workers, ~45 spawn_blocking sites |
| 10 | Per-frame churn (tessellation, row Strings, HashMaps) | 5–15 transient; feeds #3 | render pipeline map |

**Exonerated by experiment:** wallpaper (toggle = Δ≈0; 1.45 MB GPU at current size, 16 MB
at the 4 MP cap), terminal scrollback (lazily allocated), project index (few MB here).

**Theoretical floor with aggressive-but-realistic tuning: ~120–160 MB private +
100–150 MB GPU-shared ≈ 250–300 MB total.** Below ~150 MB total requires leaving the
wgpu/DX12 stack (glow backend or software rendering) — see §5.

---

## 3. Current-state architecture map (consolidated)

### 3.1 Process & threads
One process. One tokio multi-thread runtime (workers = cores, blocking pool ≤512).
UI thread (eframe). Per-LSP-server tokio task (child process kill_on_drop, framed JSON
64 KB/16 MB caps, pending map ≤512). Per-terminal-session: 4 OS threads (parent, PTY
reader, close-waiter, cmd-writer), max 32 sessions, scrollback 10k rows (lazily
allocated), persisted metadata ≤1 MB total. 2 notify watcher threads → crossbeam
bounded(4096). Plugins: wasmi Store/Instance created per command run, fuel 10 M,
module cache 16, no persistent threads. ~45 `spawn_blocking` call sites.
All production channels are bounded. Idle: 1 frame per 2 s (session-save heartbeat).

### 3.2 Frame pipeline (editor)
`update()` → panels → per pane: `prepare_editor_pane_data` (≈140-field struct rebuilt
unconditionally every frame: diagnostics maps, semantic spans, blame/inlay/lens clones,
folding, tree-sitter injection check with **O(file) text_equals every frame**) →
viewport (`ScrollArea::show_rows` → visible rows only) → per visible row: String
snapshot alloc + metrics scan + syntect job (cache: 8 ranges/buffer, scroll past =
re-parse ~96-192 lines) + galley (row render cache — **off by default**; when on, keyed
by buffer version → **whole-file invalidation per keystroke**; O(n) LRU lookup) → ~15
overlay passes → sticky scroll (no galley cache) + minimap (whole-file downsample, ≤4096
samples, repaint every frame) + overview ruler. Cursor blink: `request_repaint_after`
(120 ms) → **~8 full pipeline redraws/second while idle with a visible caret**.

### 3.3 Startup sequence (main → first frame)
Sync before window: argv probe, wgpu device/surface, tokio runtime, state.json,
recent-project dir probes (network drives can stall), settings TOML, font install
(≤10 files, 16 MB cap each), theme, session JSON (≤8 MB) + recovered-buffer rebuild on
the UI thread, watcher (recursive whole-root), syntect SyntaxSet+ThemeSet (untimed),
save_app_state ×1-2. Async after frame: index (cache-first full walk), git scan, plugin
discovery, LSP spawn (trust-gated), PTY only if terminal visible, update check.
Timed stages: 6 (runtime, settings, UI, persistence, terminal, watcher); syntect,
session restore, state writes, and first-frame cost are **untimed**.

### 3.4 Git pipeline (post-remake, current)
Full scans: startup, workspace open/restore, commit, branch/stash ops, `.git` metadata
events, watcher overflow. Scoped (`status_entries_for_paths` + `merge_scoped_statuses`):
saves, watcher file batches (≤64), stage/unstage/discard/hunk ops. Snapshot carries
monotonic revision (drives the SCM row cache). Panel distinguishes no-repo vs scan
error. `.git`-as-file and parent-repo `.git` now watched. Known gaps: no per-request
LSP-style timeout analog for hung git ops; parent-of-parent repos; non-UTF-8 paths
dropped.

### 3.5 Index & search (post-remake, current)
Incremental snapshot: full walk on cold/overflow/settings change, `apply_path_changes`
for small watcher batches (≤64 paths); cap 150k as emergency brake; dot-dirs excluded
by default (whitelist `.github/.gitlab/.config/.vscode`) + `include_hidden_dirs` escape;
user globs extend defaults; per-entry metadata drives cache validation (no second walk).
Quick Open: always-background ranking, 150 ms debounce, prefix-reuse, generation-keyed
cache. Search: live streaming reads (like VS Code's ripgrep model), metadata cache,
one-active-one-pending admission. Symbols: LSP-first, lazy ≤200 fallback.

---

## 4. Full recommendations (ranked; effort × payoff)

### Phase 0 — Free wins, days (est. −60–180 MB total impact, −6–14 MB binary, big idle-CPU cut)
1. **wgpu memory knobs**: `MemoryHints::MemoryUsage`, poll the device, prefer
   `LowPower`/iGPU by default, drop the `gles` backend feature on Windows
   (−1.5–2.5 MB binary; keep for Linux builds). Est. 20–60 MB + lighter GPU-shared.
2. **MSAA**: make it a setting, default off (est. 15–60 MB at high resolutions).
3. **`[profile.release]`**: `lto = "thin"`, `codegen-units = 1`, `strip = true`
   (−4–8 MB binary, faster release runtime). Three lines.
4. **Idle heartbeat**: session-save heartbeat wakes the app 43,000×/day even when
   unchanged. Return the full interval when the last save had no changes (or 15–30 s).
   Kills the last recurring wake while idle.
5. **LSP child-exit poll** (250 ms busy loop per server) → `child.wait()` in a
   `select!` arm. Tokio workers actually park when idle.
6. **Terminal repaint pacing**: reader thread sets an output-pending flag + one
   repaint request; UI drains at its 512-event budget (~16–33 ms pace while output
   is continuous) instead of repainting per 4 KB chunk.
7. **Cursor blink**: repaint only the caret layer (or raise the interval). Removes
   ~8 full pipeline redraws/second while idle-with-caret — the single biggest idle win
   after the heartbeat.
8. **Bound the LSP message body** (16 MB → 2–4 MB + kill-on-violation) and capture
   server stderr into a 64 KB ring for real diagnostics.

### Phase 1 — Correctness & waste (already done this session)
Settings trio root cause (memory-wipe/style/focus), Reset preview, popup viewport
clamps (all 51 windows), full-page Git History, indexing remake (uncapped incremental
snapshot, dot-dir excludes, user-globs-extend), git remake (path-scoped status, watcher
holes closed, error-vs-no-repo), per-frame git UI waste (revision-keyed row cache,
borrowed entries), dead-setting removal, devtools cache panel.

### Phase 2 — Rendering per-frame costs (days; closes the typing/scroll gap)
1. Galley cache **on by default**; stop whole-file invalidation per keystroke
   (per-line versioning; only the edited line re-lays-out). Fix the O(n) LRU lookup.
2. Cache per-row line snapshots (`Arc<str>` keyed by version+line) — kills ~150
   allocations/frame.
3. Version-key the tree-sitter `text_equals` full-file compare (runs every frame).
4. Version-key diagnostics/semantic/tag maps and blame/inlay/lens filtered clones
   (rebuild on event, not per frame).
5. Syntax visible-range cache: raise 8 → 32+ ranges (scroll past cached window
   re-parses ~96–192 lines today).
6. Route sticky-scroll rows through the galley cache.
7. Version-key the diff-move/patch-overview O(file) scans and indentation folding.

### Phase 3 — Startup (days; first-frame win)
1. Lazy syntect load (plain-text fallback covers the gap) — biggest sync blocker.
2. Async session JSON load + buffer materialization.
3. Watcher starts after first frame (or non-recursive + escalate) — seconds on
   home-dir workspaces.
4. Defer custom font install via the existing `fonts_dirty` hook.
5. Skip the startup synchronous `save_app_state` when the fingerprint is unchanged.
6. Timeout-hedged recent-project probes (dead network drives stall startup today).
7. Time the untimed segments (syntect, session restore, state writes, first frame).

### Phase 4 — Architecture (weeks; the memory-for-speed tradeoffs)
1. **Per-request LSP timeout watchdog** (sweep on the existing 250 ms tick).
2. **Rope + mmap hybrid for view mode**: mmap the file as rope leaves until first
   edit — a 500 MB log opens at ~0 buffer cost (mind Sublime's SIGBUS war story).
3. **Process isolation (Lapce-proxy pattern)** for LSP/indexers/TS workers — their
   RSS stops counting against the editor; crash isolation for free.
4. **Curated syntect set** (or lazy per-language): −10–20 MB RAM, hundreds of ms
   startup, −1–2 MB binary.
5. **Trim image codecs** (tiff −1–2 MB) and consider dropping `reqwest` for `ureq`
   (−5–7 MB; one file uses it).
6. **Feature-gate the WASM plugin runtime** (−3–5 MB for builds without plugins).
7. Multi-repo SCM (the dead `git_scan_repositories` promise) — implement or keep removed.

### Phase 5 — "Lite" profile (the 100 MB machine answer)
A settings profile: glow/OpenGL backend (the documented lightweight eframe path),
MSAA off, wallpaper off, syntax off for files >1 MB, scrollback 1k, atlas capped,
row cache 512, single window, no plugins. Evidence from the field: an egui app went
**135 MB → 30 MB** by dropping the GPU stack; egui's own core is ~3 MiB.
**Realistic Lite target: 60–120 MB total working set on Windows.** Below that means a
TUI (Helix territory) — a different product, not a profile. Anti-recommendation: do not
chase 32-bit builds (2–4 GB address-space wall, VS Code dropped them).

### Anti-goals (do not do)
- Do not mmap mutable buffers (Sublime's SIGBUS war story).
- Do not chase the `ash` 68 MB rlib (inert header constants, dead-strips).
- Do not add a 32-bit target; do not use `tokio::full`; do not add ron.
- Do not "fix" the wallpaper: 1.45 MB measured — it only mattered in theory.

---

## 5. Outcome path (evidence-based projections)

| Stage | Working set (est.) | vs Zed (~80–128 MB) |
|---|---|---|
| Today | ~353 MB idle-warm, ~560 MB total impact | heavier |
| + Phase 0 (free wins) | ~180–230 MB | at Zed's level |
| + Phases 2–3 (render + startup) | ~150–200 MB active-typing, idle near-zero CPU | lighter while working |
| + Phase 4 (architecture) | ~120–180 MB with real projects | lighter than Zed |
| + Phase 5 (Lite profile) | ~60–120 MB | Helix/Sublime class |

Typing latency: 3–10 ms today (one frame budget); Phases 2–3 target a stable
< 3 ms worst case. Startup: defer-to-frame pattern targets sub-500 ms to interactive
on cold disks with custom fonts.

## 6. Hygiene status
Tests 937 core + 5,914 app green; fmt + workspace clippy (`--all-targets -D warnings`)
clean. 2.3 GB stale build junk removed; dead `git_scan_repositories` setting removed;
`state.json` leaked test entries cleaned. Everything uncommitted — this whole session's
work lives in the working tree.
