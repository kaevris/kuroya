;; example kuroya plugin command, written directly in WebAssembly text format.
;;
;; build the binary module with wabt:
;;   wat2wasm plugin.wat -o plugin.wasm
;; or round-trip through the wat crate that kuroya-app already uses in its
;; tests (wat::parse_str). see ./README.md.
;;
;; host api v2 contract (crates/kuroya-app/src/plugin_command_runtime.rs):
;;   - guests must export linear memory as "memory"
;;   - string-returning host functions hand data back by calling the guest
;;     export kuroya_alloc(size: i32) -> i32 and writing into the returned
;;     buffer; a negative return means allocation failed
;;   - the command entrypoint signature is () -> i32. the runner looks for an
;;     export named exactly after the contributed command id first, then falls
;;     back to a default export named kuroya_plugin_command. this module
;;     exports "example.hello", matching [[contributes.commands]] id in
;;     plugin.toml.
(module
    ;; host imports, module "kuroya". installed by the linker for every run.
    ;; both take (ptr, len) pointing at utf-8 bytes in guest memory and return
    ;; an i32 status code (0 = ok, negative = error; see the table in
    ;; ./README.md). because the imports return a value, every call site must
    ;; consume the result — usually (drop (call ...)) — or module validation
    ;; fails with leftover values on the stack.
    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
    (import "kuroya" "log" (func $log (param i32 i32) (result i32)))

    ;; required guest export. one page (64 KiB) is plenty here; the sandbox
    ;; caps memory at 16 MiB regardless.
    (memory (export "memory") 1)

    ;; static strings. lengths must be byte counts, not char counts.
    ;;   "example plugin ran"              -> 18 bytes at offset 0   [0, 18)
    ;;   "Hello from Kuroya example plugin" -> 32 bytes at offset 32  [32, 64)
    (data (i32.const 0) "example plugin ran")
    (data (i32.const 32) "Hello from Kuroya example plugin")

    ;; bump allocator backing store. starts past both data segments so host
    ;; writes never clobber them. the sandbox caps tables/memories but does
    ;; not manage guest heap; this global is ours alone.
    (global $heap_next (mut i32) (i32.const 1024))

    ;; required for string-returning host calls (workspace_root,
    ;; active_buffer_path, read_file). exact signature (i32) -> i32. returns
    ;; a pointer the host writes into, or a negative value on failure.
    (func $alloc (export "kuroya_alloc") (param $size i32) (result i32)
        (local $ptr i32)
        (local $end i32)
        ;; reject negative and zero requests
        (if (i32.lt_s (local.get $size) (i32.const 1))
            (then (return (i32.const -1))))
        ;; round the bump pointer up to the next 8-byte boundary
        (local.set $ptr
            (i32.and
                (i32.add (global.get $heap_next) (i32.const 7))
                (i32.const -8)))
        (local.set $end (i32.add (local.get $ptr) (local.get $size)))
        ;; stay inside the single declared page
        (if (i32.gt_u (local.get $end) (i32.const 65536))
            (then (return (i32.const -1))))
        (global.set $heap_next (local.get $end))
        (local.get $ptr))

    ;; the command entrypoint. name must equal the contributed command id;
    ;; signature () -> i32; return value becomes the run's exit code where
    ;; zero means success.
    (func (export "example.hello") (result i32)
        ;; log lines surface in the run result (last 64 kept, 240 chars each)
        (drop (call $log (i32.const 0) (i32.const 18)))
        ;; status text shows in the app status line (240 chars max)
        (drop (call $status (i32.const 32) (i32.const 32)))
        (i32.const 0)))
