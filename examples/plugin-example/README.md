# kuroya example plugin

a minimal, working kuroya plugin. it contributes one command that runs a
webassembly module in the sandboxed command runtime, plus one language
mapping so you can see contributions end to end.

## what it demonstrates

- a valid `plugin.toml`: id/name/version, entry point, activation event,
  capabilities, command and language contributions
- the host api v2 abi every command plugin follows:
  - export linear memory as `memory`
  - export `kuroya_alloc(size: i32) -> i32` (string-returning host calls
    allocate guest buffers through it)
  - export one wasm function per command, named exactly like the contributed
    command id, with signature `() -> i32`. if that export is missing the
    runner falls back to a default export named `kuroya_plugin_command`
  - return zero for success; nonzero exit codes surface as failures
- calling `kuroya.log(ptr, len) -> i32` and `kuroya.status(ptr, len) -> i32`
  with data segments; both return `0` on success and a negative error code
  (see the table below) instead of trapping, so a malformed argument costs
  the call, not the whole run

running the `Example: Say hello` command logs `example plugin ran`, sets the
status text to `Hello from Kuroya example plugin`, and exits with code `0`.
the app status line ends up reading something like:

```text
Plugin command Example: Say hello completed: Hello from Kuroya example plugin
```

## host api v2: buffer mutation

a plugin that declares `workspace_write = true` gets two more host calls,
both taking a workspace-relative path that must resolve to the buffer that
was active when the command started:

- `kuroya.buffer_get_text(path_ptr, path_len) -> i32`: returns the captured
  text of the active buffer through the `kuroya_alloc` convention (the
  return value is the byte length; negative values are errors)
- `kuroya.buffer_set_text(path_ptr, path_len, text_ptr, text_len) -> i32`:
  stages a full-text replacement for the active buffer; returns 0 on
  success. calling it multiple times in one run is fine — the last staged
  text wins

the staged text is applied after the run finishes, whether the run succeeded
or failed: a plugin that set the text and then errored still has its last
staged write applied. the app applies it on the ui thread with
`replace_text_from_ui`, so the change lands in the regular undo history —
Ctrl+Z reverts it like a manual edit. if the buffer was closed while the
plugin ran, the staged text is dropped and the status line says so.

size caps: buffers over 8,000,000 bytes are captured without text (get
returns `-7`) and staged replacements over 8,000,000 bytes are rejected with
`-7`.

## install

plugins are per-workspace and discovered from `<workspace>/.kuroya/plugins/`.

1. copy this folder into your workspace:

   ```powershell
   New-Item -ItemType Directory -Force <workspace>\.kuroya\plugins
   Copy-Item -Recurse . <workspace>\.kuroya\plugins\plugin-example
   ```

2. make sure the workspace is trusted. untrusted workspaces block all
   plugins.
3. open settings and check the plugins section: the global enable toggle is
   on by default; individual plugins can be disabled by id there.
4. reload the workspace so discovery picks the folder up.
5. open the command palette and run `Example: Say hello`.

the contributed `.example` extension maps files like `notes.example` to the
`example` language (display name `Example`).

## rebuilding plugin.wasm

`plugin.wasm` is built from `plugin.wat`. two options:

- wabt's `wat2wasm`:

  ```powershell
  wat2wasm plugin.wat -o plugin.wasm
  ```

- the repo already depends on the `wat` crate for kuroya-app tests, so a
  small test harness can compile it too:
  `wat::parse_str(include_str!("../plugin-example/plugin.wat"))` produces the
  same bytes `wat2wasm` would emit for this module.

after rebuilding, restart or reload the workspace; the runner caches compiled
modules per file size/mtime and recompiles when the file changes.

## notes and limits

everything below mirrors what the runner actually enforces today
(`crates/kuroya-app/src/plugin_command_runtime.rs` and
`crates/kuroya-core/src/plugin.rs`):

- sandbox guarantees: fuel-capped execution (10,000,000 units), memory capped
  at 16 MiB, tables at 4096 elements, wasm entries up to 8 MiB. no filesystem
  access, no process spawning, no network unless a capability says otherwise,
  and unsupported capabilities fail closed before any code runs.
- status text: read from guest memory as utf-8, capped at 16 KiB per call,
  sanitized and truncated to 240 chars.
- log lines: 240 chars each, the last 64 lines are surfaced in the run
  result.
- `read_file` returns at most 256 KiB and only resolves paths inside the
  workspace root (symlink-safe canonicalization).
- negative return codes from string-returning host functions:

  | code | meaning |
  | ---- | ------- |
  | -1 | guest alloc failed (`kuroya_alloc` missing/failing) |
  | -2 | not found |
  | -3 | path escapes the workspace |
  | -4 | no active buffer |
  | -5 | capability denied (including a missing workspace root) |
  | -6 | misc error: malformed argument (negative pointer/length), missing or unreadable guest `memory`, invalid utf-8, host i/o failure |
  | -7 | input exceeds a host size cap (8,000,000-byte buffer cap, 256 KiB `read_file` cap, 16 KiB `status`/`log` argument cap, 4 KiB path cap) |

### why workspace_read and workspace_write are false here

declaring `workspace_read = true` enables the host functions
`workspace_root`, `active_buffer_path`, `open_buffer`, `read_file`, and
`buffer_get_text` inside the sandbox; `workspace_write = true` enables the
one buffer mutation, `buffer_set_text`. neither affects runnability: a
plugin that declares them still contributes palette-runnable commands. this
example leaves both off only because it does not read the workspace or edit
buffers; turn them on if you copy this skeleton for a plugin that does.
