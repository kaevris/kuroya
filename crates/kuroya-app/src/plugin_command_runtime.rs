use crate::{
    path_display::sanitized_display_label_cow,
    ui_event_channel::{Sender, send_critical_ui_event},
    ui_events::UiEvent,
    workspace_state::paths_match_lexically,
};
use anyhow::{Context, anyhow, bail};
use kuroya_core::{
    PluginCapabilities, PluginRuntimeRegistration, TextBuffer, normalize_child_path,
};
use std::{
    collections::VecDeque,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime},
};
use wasmi::{
    Caller, Config, Engine, Instance, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
    TypedFunc,
};

const PLUGIN_COMMAND_WASM_MAX_BYTES: u64 = 8 * 1024 * 1024;
const PLUGIN_COMMAND_MODULE_CACHE_MAX_ENTRIES: usize = 16;
const PLUGIN_COMMAND_FUEL: u64 = 10_000_000;
const PLUGIN_COMMAND_MEMORY_MAX_BYTES: usize = 16 * 1024 * 1024;
const PLUGIN_COMMAND_TABLE_MAX_ELEMENTS: usize = 4096;
const PLUGIN_COMMAND_STATUS_MAX_BYTES: usize = 16 * 1024;
const PLUGIN_COMMAND_STATUS_MAX_CHARS: usize = 240;
const PLUGIN_COMMAND_LOG_MAX_LINES: usize = 64;
const PLUGIN_COMMAND_LOG_MAX_CHARS: usize = 240;
const PLUGIN_COMMAND_PATH_MAX_BYTES: usize = 4096;
pub(crate) const MAX_PLUGIN_READ_FILE_BYTES: usize = 256 * 1024;
pub(crate) const PLUGIN_COMMAND_BUFFER_CAPTURE_MAX: usize = 8_000_000;
/// Negative return codes shared by every string-returning host function. The
/// numbering is part of the plugin ABI: each condition maps to exactly one
/// code, so a guest can branch on it regardless of which host call failed.
/// -4 is reserved exclusively for "no active buffer"; every oversize
/// condition (file, buffer text, or host-call argument) reports -7.
pub(crate) const PLUGIN_HOST_ERROR_ALLOC_FAILED: i32 = -1;
pub(crate) const PLUGIN_HOST_ERROR_NOT_FOUND: i32 = -2;
pub(crate) const PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE: i32 = -3;
pub(crate) const PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER: i32 = -4;
pub(crate) const PLUGIN_HOST_ERROR_CAPABILITY_DENIED: i32 = -5;
pub(crate) const PLUGIN_HOST_ERROR_MISC: i32 = -6;
pub(crate) const PLUGIN_HOST_ERROR_TOO_LARGE: i32 = -7;
/// Wall-clock budget for one plugin command run. Enforced at host-call
/// boundaries (`plugin_wall_clock_checkpoint`), the only points where the
/// host regains control from a running guest; compute-only plugins stay
/// bounded by fuel alone.
const PLUGIN_COMMAND_WALL_CLOCK_LIMIT: Duration = Duration::from_secs(30);
const PLUGIN_COMMAND_MEMORY_EXPORT: &str = "memory";
const PLUGIN_COMMAND_ALLOC_EXPORT: &str = "kuroya_alloc";
pub(crate) const PLUGIN_COMMAND_DEFAULT_EXPORT: &str = "kuroya_plugin_command";

static PLUGIN_COMMAND_ENGINE: OnceLock<Engine> = OnceLock::new();
static PLUGIN_COMMAND_MODULE_CACHE: OnceLock<Mutex<PluginCommandModuleCache>> = OnceLock::new();

#[cfg(test)]
static PLUGIN_COMMAND_CACHED_MODULE_COMPILES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Immutable copy of the buffer that was active when the plugin command run
/// started. Captured on the UI thread before the blocking wasm run so guest
/// reads see a consistent snapshot, and so a staged write can be attributed
/// to a concrete buffer path after the run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ActiveBufferSnapshot {
    pub(crate) path: PathBuf,
    pub(crate) text: String,
    pub(crate) too_large: bool,
}

impl ActiveBufferSnapshot {
    /// Returns `None` for untitled buffers, which have no workspace path a
    /// plugin could address. Buffers over `PLUGIN_COMMAND_BUFFER_CAPTURE_MAX`
    /// bytes keep only their path; `buffer_get_text` then reports
    /// `PLUGIN_HOST_ERROR_TOO_LARGE` instead of returning text.
    pub(crate) fn capture(buffer: &TextBuffer) -> Option<Self> {
        let path = buffer.path()?.to_path_buf();
        let snapshot = buffer.text_snapshot();
        let too_large = snapshot.len_bytes() > PLUGIN_COMMAND_BUFFER_CAPTURE_MAX;
        let text = if too_large {
            String::new()
        } else {
            snapshot.text()
        };
        Some(Self {
            path,
            text,
            too_large,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PluginHostContext {
    pub(crate) plugin_id: String,
    pub(crate) workspace_root: PathBuf,
    pub(crate) active_buffer_path: Option<PathBuf>,
    pub(crate) active_buffer: Option<ActiveBufferSnapshot>,
    pub(crate) capabilities: PluginCapabilities,
    pub(crate) events: Sender<UiEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PluginCommandExecution {
    pub(crate) exit_code: i32,
    pub(crate) status: Option<String>,
    pub(crate) used_default_export: bool,
    pub(crate) logs: Vec<String>,
    pub(crate) pending_buffer_text: Option<(PathBuf, String)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PluginCommandModuleCacheStats {
    pub(crate) entries: usize,
    pub(crate) capacity: usize,
}

#[derive(Debug)]
struct PluginCommandHostState {
    status: Option<String>,
    logs: Vec<String>,
    host: PluginHostContext,
    limits: StoreLimits,
    pending_buffer_text: Option<(PathBuf, String)>,
    /// Wall-clock instant the run began, captured when the store is created.
    /// Every host call compares its elapsed time against
    /// `PLUGIN_COMMAND_WALL_CLOCK_LIMIT`.
    started_at: Instant,
    /// Set by `plugin_wall_clock_checkpoint` once it exhausted the store's
    /// fuel because the run passed its wall-clock budget; lets the error path
    /// report the budget instead of a raw out-of-fuel trap.
    budget_exhausted: bool,
}

impl PluginCommandHostState {
    fn new(host: PluginHostContext) -> Self {
        Self {
            status: None,
            logs: Vec::new(),
            host,
            limits: plugin_command_store_limits(),
            pending_buffer_text: None,
            started_at: Instant::now(),
            budget_exhausted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginCommandModuleCacheKey {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Debug, Clone)]
struct PluginCommandModuleCacheEntry {
    key: PluginCommandModuleCacheKey,
    module: Module,
}

#[derive(Debug, Default)]
struct PluginCommandModuleCache {
    entries: VecDeque<PluginCommandModuleCacheEntry>,
}

impl PluginCommandModuleCache {
    fn get(&mut self, key: &PluginCommandModuleCacheKey) -> Option<Module> {
        let index = self.entries.iter().position(|entry| &entry.key == key)?;
        let entry = self.entries.remove(index)?;
        let module = entry.module.clone();
        self.entries.push_back(entry);
        Some(module)
    }

    fn insert(&mut self, key: PluginCommandModuleCacheKey, module: Module) {
        self.entries.retain(|entry| entry.key.path != key.path);
        while self.entries.len() >= PLUGIN_COMMAND_MODULE_CACHE_MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries
            .push_back(PluginCommandModuleCacheEntry { key, module });
    }
}

pub(crate) fn execute_plugin_command(
    runtime: &PluginRuntimeRegistration,
    command_id: &str,
    host: &PluginHostContext,
) -> anyhow::Result<PluginCommandExecution> {
    validate_plugin_command_capabilities(&runtime.capabilities)?;
    let entry = plugin_entry_path(runtime)?;
    let module = cached_plugin_command_module(&entry)?;
    execute_plugin_command_module(&module, command_id, host)
}

fn validate_plugin_command_capabilities(capabilities: &PluginCapabilities) -> anyhow::Result<()> {
    if !capabilities.commands {
        bail!("plugin does not declare command capability");
    }

    let unsupported = unsupported_runtime_capabilities(capabilities);
    if !unsupported.is_empty() {
        bail!(
            "plugin declares unsupported runtime capabilities: {}",
            unsupported.join(", ")
        );
    }
    Ok(())
}

fn unsupported_runtime_capabilities(capabilities: &PluginCapabilities) -> Vec<&'static str> {
    let mut unsupported = Vec::new();
    // workspace_read gates the read host calls (workspace_root,
    // active_buffer_path, open_buffer, read_file, buffer_get_text) and
    // workspace_write gates the buffer write (buffer_set_text), so both are
    // supported runtime capabilities; process_spawn and network still fail
    // closed.
    if capabilities.process_spawn {
        unsupported.push("process_spawn");
    }
    if capabilities.network {
        unsupported.push("network");
    }
    unsupported
}

fn plugin_entry_path(runtime: &PluginRuntimeRegistration) -> anyhow::Result<PathBuf> {
    let Some(entry) = runtime.command_entry() else {
        bail!("plugin has no command entry");
    };
    normalize_child_path(&runtime.root, entry)
        .ok_or_else(|| anyhow!("plugin entry must stay inside the plugin root"))
}

fn cached_plugin_command_module(entry: &Path) -> anyhow::Result<Module> {
    let metadata = plugin_entry_metadata(entry)?;
    let key = plugin_command_module_cache_key(entry, &metadata);
    if let Some(module) = plugin_command_module_cache()
        .lock()
        .map_err(|_| anyhow!("plugin command module cache lock was poisoned"))?
        .get(&key)
    {
        return Ok(module);
    }

    let wasm = read_plugin_entry_bytes_with_metadata(entry, &metadata)?;
    let module = compile_plugin_command_module(&wasm)?;
    #[cfg(test)]
    PLUGIN_COMMAND_CACHED_MODULE_COMPILES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    plugin_command_module_cache()
        .lock()
        .map_err(|_| anyhow!("plugin command module cache lock was poisoned"))?
        .insert(key, module.clone());
    Ok(module)
}

fn plugin_command_module_cache_key(
    entry: &Path,
    metadata: &fs::Metadata,
) -> PluginCommandModuleCacheKey {
    PluginCommandModuleCacheKey {
        path: entry.to_path_buf(),
        len: metadata.len(),
        modified: metadata.modified().ok(),
    }
}

fn plugin_entry_metadata(entry: &Path) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::metadata(entry).map_err(plugin_entry_read_error)?;
    if !metadata.is_file() {
        bail!("plugin entry is not a regular file");
    }
    if metadata.len() > PLUGIN_COMMAND_WASM_MAX_BYTES {
        bail!(
            "plugin entry exceeds the {} byte limit",
            PLUGIN_COMMAND_WASM_MAX_BYTES
        );
    }
    Ok(metadata)
}

fn read_plugin_entry_bytes_with_metadata(
    entry: &Path,
    metadata: &fs::Metadata,
) -> anyhow::Result<Vec<u8>> {
    let file = File::open(entry).map_err(plugin_entry_read_error)?;
    let mut limited = file.take(PLUGIN_COMMAND_WASM_MAX_BYTES.saturating_add(1));
    let capacity = usize::try_from(metadata.len()).unwrap_or(0);
    let mut bytes = Vec::with_capacity(capacity);
    limited
        .read_to_end(&mut bytes)
        .map_err(plugin_entry_read_error)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > PLUGIN_COMMAND_WASM_MAX_BYTES {
        bail!(
            "plugin entry exceeds the {} byte limit",
            PLUGIN_COMMAND_WASM_MAX_BYTES
        );
    }
    Ok(bytes)
}

fn plugin_entry_read_error(error: std::io::Error) -> anyhow::Error {
    anyhow!("failed to read plugin entry: {error}")
}

fn plugin_command_engine() -> &'static Engine {
    PLUGIN_COMMAND_ENGINE.get_or_init(|| {
        let mut config = Config::default();
        config.consume_fuel(true);
        Engine::new(&config)
    })
}

fn plugin_command_module_cache() -> &'static Mutex<PluginCommandModuleCache> {
    PLUGIN_COMMAND_MODULE_CACHE.get_or_init(|| Mutex::new(PluginCommandModuleCache::default()))
}

pub(crate) fn plugin_command_module_cache_stats() -> PluginCommandModuleCacheStats {
    let entries = plugin_command_module_cache()
        .lock()
        .map(|cache| cache.entries.len())
        .unwrap_or_default();
    PluginCommandModuleCacheStats {
        entries,
        capacity: PLUGIN_COMMAND_MODULE_CACHE_MAX_ENTRIES,
    }
}

#[cfg(test)]
fn reset_plugin_command_module_cache_for_test() {
    if let Some(cache) = PLUGIN_COMMAND_MODULE_CACHE.get() {
        cache
            .lock()
            .expect("plugin command module cache should not be poisoned")
            .entries
            .clear();
    }
    PLUGIN_COMMAND_CACHED_MODULE_COMPILES.store(0, std::sync::atomic::Ordering::SeqCst);
}

#[cfg(test)]
fn plugin_command_cached_module_compiles_for_test() -> usize {
    PLUGIN_COMMAND_CACHED_MODULE_COMPILES.load(std::sync::atomic::Ordering::SeqCst)
}

fn compile_plugin_command_module(wasm: &[u8]) -> anyhow::Result<Module> {
    Module::new(plugin_command_engine(), wasm).context("failed to load plugin wasm")
}

#[cfg(test)]
fn execute_plugin_command_wasm(
    wasm: &[u8],
    command_id: &str,
    host: &PluginHostContext,
) -> anyhow::Result<PluginCommandExecution> {
    let module = compile_plugin_command_module(wasm)?;
    execute_plugin_command_module(&module, command_id, host)
}

fn execute_plugin_command_module(
    module: &Module,
    command_id: &str,
    host: &PluginHostContext,
) -> anyhow::Result<PluginCommandExecution> {
    let engine = plugin_command_engine();
    let mut store = Store::new(engine, PluginCommandHostState::new(host.clone()));
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(PLUGIN_COMMAND_FUEL)
        .context("failed to initialize plugin fuel limit")?;
    let mut linker = Linker::new(engine);
    linker
        .func_wrap("kuroya", "status", plugin_status_host_call)
        .context("failed to install plugin host API")?;
    linker
        .func_wrap("kuroya", "log", plugin_log_host_call)
        .context("failed to install plugin host API")?;
    linker
        .func_wrap("kuroya", "workspace_root", plugin_workspace_root_host_call)
        .context("failed to install plugin host API")?;
    linker
        .func_wrap(
            "kuroya",
            "active_buffer_path",
            plugin_active_buffer_path_host_call,
        )
        .context("failed to install plugin host API")?;
    linker
        .func_wrap("kuroya", "open_buffer", plugin_open_buffer_host_call)
        .context("failed to install plugin host API")?;
    linker
        .func_wrap("kuroya", "read_file", plugin_read_file_host_call)
        .context("failed to install plugin host API")?;
    linker
        .func_wrap(
            "kuroya",
            "buffer_get_text",
            plugin_buffer_get_text_host_call,
        )
        .context("failed to install plugin host API")?;
    linker
        .func_wrap(
            "kuroya",
            "buffer_set_text",
            plugin_buffer_set_text_host_call,
        )
        .context("failed to install plugin host API")?;
    let instance = linker
        .instantiate(&mut store, module)
        .context("failed to instantiate plugin wasm")?
        .start(&mut store)
        .context("failed to start plugin wasm")?;
    let (command, used_default_export) = plugin_command_func(&instance, &store, command_id)?;
    let call_result = command.call(&mut store, ());
    // Staged buffer text survives a failed run on purpose: a plugin that set
    // the buffer text and then trapped (or exited nonzero) still has its last
    // staged write applied to the captured buffer.
    let pending_buffer_text = store.data_mut().pending_buffer_text.take();
    let exit_code = call_result.map_err(|error| {
        let budget_exhausted = store.data().budget_exhausted;
        plugin_command_call_error(error, budget_exhausted, &host.plugin_id)
    })?;

    let status = store.data().status.clone();
    let logs = store.data().logs.clone();
    Ok(PluginCommandExecution {
        exit_code,
        status,
        used_default_export,
        logs,
        pending_buffer_text,
    })
}

fn plugin_command_store_limits() -> StoreLimits {
    StoreLimitsBuilder::new()
        .memory_size(PLUGIN_COMMAND_MEMORY_MAX_BYTES)
        .table_elements(PLUGIN_COMMAND_TABLE_MAX_ELEMENTS)
        .instances(1)
        .memories(1)
        .tables(4)
        .trap_on_grow_failure(true)
        .build()
}

fn plugin_command_func(
    instance: &Instance,
    store: &Store<PluginCommandHostState>,
    command_id: &str,
) -> anyhow::Result<(TypedFunc<(), i32>, bool)> {
    if let Some(func) = instance.get_func(store, command_id) {
        return func
            .typed::<(), i32>(store)
            .map(|func| (func, false))
            .map_err(|error| {
                anyhow!(
                    "plugin command export {} must have signature () -> i32: {error}",
                    plugin_command_export_fragment(command_id)
                )
            });
    }

    if let Some(func) = instance.get_func(store, PLUGIN_COMMAND_DEFAULT_EXPORT) {
        return func
            .typed::<(), i32>(store)
            .map(|func| (func, true))
            .map_err(|error| {
                anyhow!(
                    "plugin default command export {} must have signature () -> i32: {error}",
                    PLUGIN_COMMAND_DEFAULT_EXPORT
                )
            });
    }

    bail!(
        "plugin command export {} was not found",
        plugin_command_export_fragment(command_id)
    )
}

fn plugin_command_call_error(
    error: wasmi::Error,
    budget_exhausted: bool,
    plugin_id: &str,
) -> anyhow::Error {
    anyhow!(plugin_command_trap_error_text(
        &error.to_string(),
        budget_exhausted,
        plugin_id,
    ))
}

/// Renders the user-facing failure text for a trapped plugin command run. A
/// run the wall-clock checkpoint cut short reports the budget; any other fuel
/// trap keeps the generic fuel wording, and everything else reports the raw
/// trap. Fuel is matched case-insensitively in the trap text because wasmi
/// 0.46 renders `TrapCode::OutOfFuel` as "all fuel consumed by WebAssembly".
fn plugin_command_trap_error_text(error: &str, budget_exhausted: bool, plugin_id: &str) -> String {
    if budget_exhausted {
        return format!(
            "Plugin {} exceeded its {}s time budget",
            plugin_command_export_fragment(plugin_id),
            PLUGIN_COMMAND_WALL_CLOCK_LIMIT.as_secs()
        );
    }
    if error.to_ascii_lowercase().contains("fuel") {
        return "plugin command exceeded the execution fuel limit".to_owned();
    }
    format!("plugin command trapped: {error}")
}

/// Wall-clock checkpoint at a plugin host-call boundary. Host calls are the
/// only points where the host regains control from a running guest, so this
/// is the one place a wall-clock deadline can be enforced: once the run has
/// spent its budget the checkpoint zeroes the store's remaining fuel so the
/// very next guest instruction traps out of fuel and the run fails through
/// the ordinary error path with the budget message. Compute-only plugins
/// that never call into the host keep fuel (`PLUGIN_COMMAND_FUEL`) as their
/// only bound; host-calling plugins get the wall-clock guarantee at every
/// host-call boundary.
fn plugin_wall_clock_checkpoint(
    caller: &mut Caller<'_, PluginCommandHostState>,
) -> Result<(), wasmi::Error> {
    if !plugin_wall_clock_budget_exhausted(
        caller.data().started_at,
        Instant::now(),
        PLUGIN_COMMAND_WALL_CLOCK_LIMIT,
    ) {
        return Ok(());
    }
    caller.data_mut().budget_exhausted = true;
    // wasmi 0.46: `Caller::set_fuel` updates the store's remaining fuel while
    // host code runs; with zero fuel the next guest instruction traps.
    caller.set_fuel(0).map_err(|error| {
        wasmi::Error::new(format!("failed to exhaust plugin time budget: {error}"))
    })
}

/// Pure budget check behind `plugin_wall_clock_checkpoint`. The limit is a
/// parameter so tests can pin it without faking `Instant`; production passes
/// `PLUGIN_COMMAND_WALL_CLOCK_LIMIT`.
fn plugin_wall_clock_budget_exhausted(started_at: Instant, now: Instant, limit: Duration) -> bool {
    now.saturating_duration_since(started_at) >= limit
}

fn plugin_status_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    let text = match plugin_read_guest_string(&caller, ptr, len, PLUGIN_COMMAND_STATUS_MAX_BYTES) {
        Ok(text) => text,
        Err(code) => return Ok(code),
    };
    caller.data_mut().status = plugin_command_status_output(&text);
    Ok(0)
}

fn plugin_log_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    let text = match plugin_read_guest_string(&caller, ptr, len, PLUGIN_COMMAND_STATUS_MAX_BYTES) {
        Ok(text) => text,
        Err(code) => return Ok(code),
    };
    plugin_host_log(caller.data_mut(), &text);
    Ok(0)
}

fn plugin_workspace_root_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    {
        let state = caller.data_mut();
        if let Err(code) = plugin_require_workspace_read(state, "workspace_root") {
            return Ok(code);
        }
    }
    let root = caller
        .data()
        .host
        .workspace_root
        .to_string_lossy()
        .into_owned();
    plugin_send_bytes_to_guest(caller, root.as_bytes(), PLUGIN_COMMAND_STATUS_MAX_BYTES)
}

fn plugin_active_buffer_path_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    let active = {
        let state = caller.data_mut();
        if let Err(code) = plugin_require_workspace_read(state, "active_buffer_path") {
            return Ok(code);
        }
        state.host.active_buffer_path.clone()
    };
    let Some(active) = active else {
        return Ok(PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER);
    };
    let path = active.to_string_lossy().into_owned();
    plugin_send_bytes_to_guest(caller, path.as_bytes(), PLUGIN_COMMAND_STATUS_MAX_BYTES)
}

fn plugin_open_buffer_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    {
        let state = caller.data_mut();
        if let Err(code) = plugin_require_workspace_read(state, "open_buffer") {
            return Ok(code);
        }
    }
    let raw = match plugin_read_guest_string(&caller, ptr, len, PLUGIN_COMMAND_PATH_MAX_BYTES) {
        Ok(raw) => raw,
        Err(code) => return Ok(code),
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(PLUGIN_HOST_ERROR_NOT_FOUND);
    }
    let resolved = match plugin_confined_workspace_path(&caller.data().host.workspace_root, raw) {
        Ok(resolved) => resolved,
        Err(code) => return Ok(code),
    };
    let event = UiEvent::PluginOpenFileRequested {
        plugin_id: caller.data().host.plugin_id.clone(),
        path: resolved,
    };
    if send_critical_ui_event(&caller.data().host.events, event) {
        return Ok(0);
    }
    Ok(PLUGIN_HOST_ERROR_MISC)
}

fn plugin_read_file_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    {
        let state = caller.data_mut();
        if let Err(code) = plugin_require_workspace_read(state, "read_file") {
            return Ok(code);
        }
    }
    let raw = match plugin_read_guest_string(&caller, ptr, len, PLUGIN_COMMAND_PATH_MAX_BYTES) {
        Ok(raw) => raw,
        Err(code) => return Ok(code),
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(PLUGIN_HOST_ERROR_NOT_FOUND);
    }
    let root = caller.data().host.workspace_root.clone();
    let resolved = match plugin_confined_workspace_path(&root, raw) {
        Ok(resolved) => resolved,
        Err(code) => return Ok(code),
    };
    let metadata = match fs::metadata(&resolved) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_NOT_FOUND),
    };
    if !metadata.is_file() {
        return Ok(PLUGIN_HOST_ERROR_NOT_FOUND);
    }
    if metadata.len() > u64::try_from(MAX_PLUGIN_READ_FILE_BYTES).unwrap_or(u64::MAX) {
        return Ok(PLUGIN_HOST_ERROR_TOO_LARGE);
    }
    let bytes = match read_confined_workspace_file(&resolved) {
        Ok(bytes) => bytes,
        Err(code) => return Ok(code),
    };
    plugin_send_bytes_to_guest(caller, &bytes, MAX_PLUGIN_READ_FILE_BYTES)
}

fn plugin_buffer_get_text_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    let snapshot = {
        let state = caller.data_mut();
        // Reading the buffer is a read: gated on workspace_read like the
        // other read host calls; only buffer_set_text needs workspace_write.
        if let Err(code) = plugin_require_workspace_read(state, "buffer_get_text") {
            return Ok(code);
        }
        state.host.active_buffer.clone()
    };
    let Some(snapshot) = snapshot else {
        return Ok(PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER);
    };
    if !plugin_guest_path_matches_active_buffer(&mut caller, &snapshot, ptr, len)? {
        return Ok(PLUGIN_HOST_ERROR_NOT_FOUND);
    }
    if snapshot.too_large {
        return Ok(PLUGIN_HOST_ERROR_TOO_LARGE);
    }
    plugin_send_bytes_to_guest(
        caller,
        snapshot.text.as_bytes(),
        PLUGIN_COMMAND_BUFFER_CAPTURE_MAX,
    )
}

fn plugin_buffer_set_text_host_call(
    mut caller: Caller<'_, PluginCommandHostState>,
    path_ptr: i32,
    path_len: i32,
    text_ptr: i32,
    text_len: i32,
) -> Result<i32, wasmi::Error> {
    plugin_wall_clock_checkpoint(&mut caller)?;
    let snapshot = {
        let state = caller.data_mut();
        if let Err(code) = plugin_require_workspace_write(state, "buffer_set_text") {
            return Ok(code);
        }
        state.host.active_buffer.clone()
    };
    let Some(snapshot) = snapshot else {
        return Ok(PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER);
    };
    if !plugin_guest_path_matches_active_buffer(&mut caller, &snapshot, path_ptr, path_len)? {
        return Ok(PLUGIN_HOST_ERROR_NOT_FOUND);
    }
    // Same failure channel as every other host call: an oversized or
    // malformed staged text reports a negative code (never a trap).
    let bytes = match plugin_read_guest_bytes(
        &caller,
        text_ptr,
        text_len,
        PLUGIN_COMMAND_BUFFER_CAPTURE_MAX,
    ) {
        Ok(bytes) => bytes,
        Err(code) => return Ok(code),
    };
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_MISC),
    };
    // Staged, not applied: the run must finish before the UI thread touches
    // the buffer. Multiple buffer_set_text calls overwrite the staged text,
    // so the last call before the run ends wins. The snapshot path (the
    // buffer's own stored path) is staged so the UI handler can match the
    // open buffer exactly.
    let state = caller.data_mut();
    state.pending_buffer_text = Some((snapshot.path, text));
    Ok(0)
}

/// Reads a guest path and reports whether it addresses the captured active
/// buffer: non-empty, confined to the workspace root, and lexically equal to
/// the captured buffer path. A path the host cannot read at all is simply not
/// the active buffer, so every failure collapses to `false` (reported by the
/// caller as `PLUGIN_HOST_ERROR_NOT_FOUND`).
fn plugin_guest_path_matches_active_buffer(
    caller: &mut Caller<'_, PluginCommandHostState>,
    snapshot: &ActiveBufferSnapshot,
    ptr: i32,
    len: i32,
) -> Result<bool, wasmi::Error> {
    let raw = match plugin_read_guest_string(caller, ptr, len, PLUGIN_COMMAND_PATH_MAX_BYTES) {
        Ok(raw) => raw,
        Err(_) => return Ok(false),
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(false);
    }
    let root = caller.data().host.workspace_root.clone();
    match plugin_confined_workspace_path(&root, raw) {
        Ok(resolved) => Ok(paths_match_lexically(&snapshot.path, &resolved)),
        Err(_) => Ok(false),
    }
}

/// The one failure channel for guest-memory and argument reads: every
/// malformed argument or guest-memory failure becomes a negative
/// `PLUGIN_HOST_ERROR_*` code instead of a trap, so a guest always gets a
/// chance to observe and report the failure. Oversized lengths report
/// `PLUGIN_HOST_ERROR_TOO_LARGE`; everything else (negative pointer or
/// length, missing exported memory, failed read, and — for the string
/// variant — invalid UTF-8) reports `PLUGIN_HOST_ERROR_MISC`.
fn plugin_read_guest_bytes(
    caller: &Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
    max_len: usize,
) -> Result<Vec<u8>, i32> {
    if len < 0 {
        return Err(PLUGIN_HOST_ERROR_MISC);
    }
    let len = usize::try_from(len).unwrap_or(0);
    if len > max_len {
        return Err(PLUGIN_HOST_ERROR_TOO_LARGE);
    }
    if len == 0 {
        return Ok(Vec::new());
    }
    let ptr = usize::try_from(ptr).map_err(|_| PLUGIN_HOST_ERROR_MISC)?;
    let memory = plugin_guest_memory(caller).ok_or(PLUGIN_HOST_ERROR_MISC)?;
    let mut bytes = vec![0; len];
    memory
        .read(caller, ptr, &mut bytes)
        .map_err(|_| PLUGIN_HOST_ERROR_MISC)?;
    Ok(bytes)
}

fn plugin_read_guest_string(
    caller: &Caller<'_, PluginCommandHostState>,
    ptr: i32,
    len: i32,
    max_len: usize,
) -> Result<String, i32> {
    let bytes = plugin_read_guest_bytes(caller, ptr, len, max_len)?;
    String::from_utf8(bytes).map_err(|_| PLUGIN_HOST_ERROR_MISC)
}

fn plugin_guest_memory(caller: &Caller<'_, PluginCommandHostState>) -> Option<wasmi::Memory> {
    caller
        .get_export(PLUGIN_COMMAND_MEMORY_EXPORT)
        .and_then(|export| export.into_memory())
}

fn plugin_host_log(state: &mut PluginCommandHostState, message: &str) {
    let line = sanitized_display_label_cow(message, PLUGIN_COMMAND_LOG_MAX_CHARS, "");
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    if state.logs.len() >= PLUGIN_COMMAND_LOG_MAX_LINES {
        state.logs.remove(0);
    }
    state.logs.push(line.to_owned());
}

fn plugin_require_workspace_read(state: &mut PluginCommandHostState, api: &str) -> Result<(), i32> {
    if !state.host.capabilities.workspace_read || state.host.workspace_root.as_os_str().is_empty() {
        plugin_host_log(state, &format!("denied {api}: workspace_read unavailable"));
        return Err(PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
    }
    Ok(())
}

fn plugin_require_workspace_write(
    state: &mut PluginCommandHostState,
    api: &str,
) -> Result<(), i32> {
    if !state.host.capabilities.workspace_write || state.host.workspace_root.as_os_str().is_empty()
    {
        plugin_host_log(state, &format!("denied {api}: workspace_write unavailable"));
        return Err(PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
    }
    Ok(())
}

fn plugin_confined_workspace_path(root: &Path, raw: &str) -> Result<PathBuf, i32> {
    let candidate =
        normalize_child_path(root, Path::new(raw)).ok_or(PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE)?;
    let canonical_root = root.canonicalize().map_err(|_| PLUGIN_HOST_ERROR_MISC)?;
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|_| PLUGIN_HOST_ERROR_NOT_FOUND)?;
    if !canonical_candidate.starts_with(&canonical_root) {
        return Err(PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE);
    }
    Ok(canonical_candidate)
}

fn read_confined_workspace_file(resolved: &Path) -> Result<Vec<u8>, i32> {
    let file = File::open(resolved).map_err(|_| PLUGIN_HOST_ERROR_MISC)?;
    let mut limited = file.take(u64::try_from(MAX_PLUGIN_READ_FILE_BYTES).unwrap_or(u64::MAX));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .map_err(|_| PLUGIN_HOST_ERROR_MISC)?;
    if bytes.len() > MAX_PLUGIN_READ_FILE_BYTES {
        return Err(PLUGIN_HOST_ERROR_TOO_LARGE);
    }
    Ok(bytes)
}

/// Copies `bytes` into guest memory through the guest's `kuroya_alloc` and
/// returns the byte length. Like the read helper this never traps: every
/// guest-memory failure (broken allocator, missing memory, unwritable
/// buffer) becomes a negative `PLUGIN_HOST_ERROR_*` code, so a hostile or
/// buggy guest costs the call, not the whole run. An empty payload returns
/// `Ok(0)` only when the content itself is genuinely empty (an empty
/// workspace file, an empty buffer); absent contexts never reach this far
/// because their callers gate first — `workspace_root` returns
/// `PLUGIN_HOST_ERROR_CAPABILITY_DENIED` without `workspace_read` or a root,
/// and `active_buffer_path` returns `PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER`.
fn plugin_send_bytes_to_guest(
    mut caller: Caller<'_, PluginCommandHostState>,
    bytes: &[u8],
    max_bytes: usize,
) -> Result<i32, wasmi::Error> {
    if bytes.len() > max_bytes {
        return Ok(PLUGIN_HOST_ERROR_TOO_LARGE);
    }
    if bytes.is_empty() {
        return Ok(0);
    }
    let size = match i32::try_from(bytes.len()) {
        Ok(size) => size,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_MISC),
    };
    let func = match caller
        .get_export(PLUGIN_COMMAND_ALLOC_EXPORT)
        .and_then(|export| export.into_func())
    {
        Some(func) => func,
        None => return Ok(PLUGIN_HOST_ERROR_ALLOC_FAILED),
    };
    let alloc = match func.typed::<i32, i32>(&caller) {
        Ok(alloc) => alloc,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_ALLOC_FAILED),
    };
    let ptr = match alloc.call(&mut caller, size) {
        Ok(ptr) => ptr,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_ALLOC_FAILED),
    };
    if ptr < 0 {
        return Ok(PLUGIN_HOST_ERROR_ALLOC_FAILED);
    }
    let ptr = match usize::try_from(ptr) {
        Ok(ptr) => ptr,
        Err(_) => return Ok(PLUGIN_HOST_ERROR_MISC),
    };
    let memory = match plugin_guest_memory(&caller) {
        Some(memory) => memory,
        None => return Ok(PLUGIN_HOST_ERROR_MISC),
    };
    if memory.write(&mut caller, ptr, bytes).is_err() {
        return Ok(PLUGIN_HOST_ERROR_MISC);
    }
    Ok(size)
}

pub(crate) fn plugin_command_status_output(value: &str) -> Option<String> {
    let output = sanitized_display_label_cow(value, PLUGIN_COMMAND_STATUS_MAX_CHARS, "");
    let output = output.trim();
    if output.is_empty() {
        None
    } else {
        Some(output.to_owned())
    }
}

fn plugin_command_export_fragment(value: &str) -> String {
    sanitized_display_label_cow(value, 96, "command").into_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveBufferSnapshot, MAX_PLUGIN_READ_FILE_BYTES, PLUGIN_COMMAND_BUFFER_CAPTURE_MAX,
        PLUGIN_COMMAND_DEFAULT_EXPORT, PLUGIN_COMMAND_MEMORY_MAX_BYTES,
        PLUGIN_COMMAND_STATUS_MAX_BYTES, PLUGIN_COMMAND_WALL_CLOCK_LIMIT,
        PLUGIN_COMMAND_WASM_MAX_BYTES, PLUGIN_HOST_ERROR_ALLOC_FAILED,
        PLUGIN_HOST_ERROR_CAPABILITY_DENIED, PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE,
        PLUGIN_HOST_ERROR_MISC, PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER, PLUGIN_HOST_ERROR_NOT_FOUND,
        PLUGIN_HOST_ERROR_TOO_LARGE, PluginHostContext, execute_plugin_command,
        execute_plugin_command_wasm, plugin_command_cached_module_compiles_for_test,
        plugin_command_status_output, plugin_command_trap_error_text,
        plugin_wall_clock_budget_exhausted, reset_plugin_command_module_cache_for_test,
    };
    use crate::ui_events::UiEvent;
    use kuroya_core::{
        PluginActivationEvent, PluginCapabilities, PluginRuntimeRegistration, TextBuffer,
        normalize_child_path,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    static CACHE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    static TEST_PLUGIN_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn cache_test_lock() -> std::sync::MutexGuard<'static, ()> {
        CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn execute_plugin_command_runs_named_export_and_reads_status() {
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "Hello from plugin")
                    (func (export "example.sayHello") (result i32)
                        i32.const 0
                        i32.const 17
                        call $status
                        drop
                        i32.const 0
                    )
                )
                "#,
            ),
            "example.sayHello",
            &host_context(PathBuf::new()),
        )
        .expect("plugin command should run");

        assert_eq!(execution.exit_code, 0);
        assert_eq!(execution.status.as_deref(), Some("Hello from plugin"));
        assert!(!execution.used_default_export);
    }

    #[test]
    fn execute_plugin_command_uses_default_export_when_command_export_is_absent() {
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(&format!(
                r#"
                (module
                    (func (export "{PLUGIN_COMMAND_DEFAULT_EXPORT}") (result i32)
                        i32.const 0
                    )
                )
                "#
            )),
            "example.missing",
            &host_context(PathBuf::new()),
        )
        .expect("default export should run");

        assert_eq!(execution.exit_code, 0);
        assert!(execution.status.is_none());
        assert!(execution.used_default_export);
    }

    #[test]
    fn execute_plugin_command_reports_nonzero_exit_without_runtime_error() {
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (func (export "example.fail") (result i32)
                        i32.const 7
                    )
                )
                "#,
            ),
            "example.fail",
            &host_context(PathBuf::new()),
        )
        .expect("nonzero command exit is a plugin result");

        assert_eq!(execution.exit_code, 7);
    }

    #[test]
    fn execute_plugin_command_rejects_missing_export() {
        let error = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (func (export "other.command") (result i32)
                        i32.const 0
                    )
                )
                "#,
            ),
            "example.missing",
            &host_context(PathBuf::new()),
        )
        .expect_err("missing command export should fail");

        assert!(error.to_string().contains("was not found"));
    }

    #[test]
    fn execute_plugin_command_rejects_wrong_signature() {
        let error = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (func (export "example.bad") (param i32) (result i32)
                        local.get 0
                    )
                )
                "#,
            ),
            "example.bad",
            &host_context(PathBuf::new()),
        )
        .expect_err("wrong command export signature should fail");

        assert!(error.to_string().contains("must have signature"));
    }

    #[test]
    fn execute_plugin_command_stops_infinite_loop_with_fuel() {
        let error = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (func (export "example.loop") (result i32)
                        loop $again
                            br $again
                        end
                        i32.const 0
                    )
                )
                "#,
            ),
            "example.loop",
            &host_context(PathBuf::new()),
        )
        .expect_err("fuel should stop infinite loop");

        assert!(error.to_string().contains("fuel"));
    }

    #[test]
    fn execute_plugin_command_rejects_large_initial_memory() {
        let pages = PLUGIN_COMMAND_MEMORY_MAX_BYTES / 65_536 + 1;
        let error = execute_plugin_command_wasm(
            &wasm_bytes(&format!(
                r#"
                (module
                    (memory {pages})
                    (func (export "example.run") (result i32)
                        i32.const 0
                    )
                )
                "#
            )),
            "example.run",
            &host_context(PathBuf::new()),
        )
        .expect_err("initial memory should be bounded");

        assert!(error.to_string().contains("instantiate"));
    }

    #[test]
    fn execute_plugin_command_reuses_cached_module_for_repeated_entry() {
        let _guard = cache_test_lock();
        reset_plugin_command_module_cache_for_test();
        let temp = TestPluginDir::new();
        let wasm_path = temp.write_wasm(
            "plugin.wasm",
            r#"
            (module
                (func (export "example.run") (result i32)
                    i32.const 13
                )
            )
            "#,
        );
        let runtime = runtime_with_entry(temp.root(), wasm_path);

        let first = run_runtime_command(&runtime, "example.run")
            .expect("first plugin command run should compile");
        let second = run_runtime_command(&runtime, "example.run")
            .expect("second plugin command run should reuse the cached module");

        assert_eq!(first.exit_code, 13);
        assert_eq!(second.exit_code, 13);
        assert_eq!(plugin_command_cached_module_compiles_for_test(), 1);
    }

    #[test]
    fn execute_plugin_command_invalidates_cached_module_when_entry_changes() {
        let _guard = cache_test_lock();
        reset_plugin_command_module_cache_for_test();
        let temp = TestPluginDir::new();
        let wasm_path = temp.write_wasm(
            "plugin.wasm",
            r#"
            (module
                (func (export "example.run") (result i32)
                    i32.const 1
                )
            )
            "#,
        );
        let runtime = runtime_with_entry(temp.root(), wasm_path);

        let first = run_runtime_command(&runtime, "example.run")
            .expect("first plugin command run should compile");
        temp.write_wasm(
            "plugin.wasm",
            r#"
            (module
                (func (export "example.helper") (result i32)
                    i32.const 0
                )
                (func (export "example.run") (result i32)
                    i32.const 2
                )
            )
            "#,
        );
        let second = run_runtime_command(&runtime, "example.run")
            .expect("changed plugin command entry should recompile");

        assert_eq!(first.exit_code, 1);
        assert_eq!(second.exit_code, 2);
        assert_eq!(plugin_command_cached_module_compiles_for_test(), 2);
    }

    #[test]
    fn execute_plugin_command_rejects_unsupported_capabilities() {
        let temp = TestPluginDir::new();
        let wasm_path = temp.write_wasm("plugin.wasm", "(module)");
        let runtime = runtime_with_entry(temp.root(), wasm_path);
        let runtime = PluginRuntimeRegistration {
            capabilities: PluginCapabilities {
                commands: true,
                process_spawn: true,
                ..PluginCapabilities::default()
            },
            ..runtime
        };

        let error = run_runtime_command(&runtime, "example.run")
            .expect_err("unsupported capability should fail closed");

        assert!(error.to_string().contains("process_spawn"));
    }

    #[test]
    fn execute_plugin_command_accepts_workspace_write_capability() {
        let _guard = cache_test_lock();
        reset_plugin_command_module_cache_for_test();
        let temp = TestPluginDir::new();
        let wasm_path = temp.write_wasm(
            "plugin.wasm",
            r#"
            (module
                (func (export "example.run") (result i32)
                    i32.const 0
                )
            )
            "#,
        );
        let runtime = runtime_with_entry(temp.root(), wasm_path);
        let runtime = PluginRuntimeRegistration {
            capabilities: PluginCapabilities {
                commands: true,
                workspace_write: true,
                ..PluginCapabilities::default()
            },
            ..runtime
        };

        let execution = run_runtime_command(&runtime, "example.run")
            .expect("workspace_write is a supported runtime capability");

        assert_eq!(execution.exit_code, 0);
        assert_eq!(execution.pending_buffer_text, None);
    }

    #[test]
    fn execute_plugin_command_rejects_entry_outside_plugin_root() {
        let temp = TestPluginDir::new();
        let outside_temp = TestPluginDir::new();
        let outside = outside_temp.write_wasm("outside.wasm", "(module)");
        let runtime = runtime_with_entry(temp.root(), outside);

        let error =
            run_runtime_command(&runtime, "example.run").expect_err("outside entry should fail");

        assert!(error.to_string().contains("plugin root"));
    }

    #[test]
    fn execute_plugin_command_rejects_oversized_entry_without_reading_unbounded() {
        let temp = TestPluginDir::new();
        let entry = temp.root().join("large.wasm");
        fs::write(
            &entry,
            vec![0_u8; usize::try_from(PLUGIN_COMMAND_WASM_MAX_BYTES).unwrap() + 1],
        )
        .expect("write oversized plugin entry");
        let runtime = runtime_with_entry(temp.root(), entry);

        let error =
            run_runtime_command(&runtime, "example.run").expect_err("oversized entry should fail");

        assert!(error.to_string().contains("byte limit"));
    }

    #[test]
    fn plugin_command_status_output_sanitizes_and_truncates() {
        let output = plugin_command_status_output(&format!("done\n{}\u{202e}", "x".repeat(512)))
            .expect("non-empty output");

        assert!(!output.chars().any(char::is_control));
        assert!(!output.contains('\u{202e}'));
        assert!(output.contains("..."));
        assert!(output.chars().count() <= 240);
        assert!(plugin_command_status_output(" \n \u{202e}").is_none());
    }

    #[test]
    fn execute_plugin_command_captures_guest_logs() {
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "log" (func $log (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "first message")
                    (data (i32.const 32) "second message")
                    (func (export "example.run") (result i32)
                        (drop (call $log (i32.const 0) (i32.const 13)))
                        (drop (call $log (i32.const 32) (i32.const 14)))
                        i32.const 0
                    )
                )
                "#,
            ),
            "example.run",
            &host_context(PathBuf::new()),
        )
        .expect("plugin command should run");

        assert_eq!(execution.logs, vec!["first message", "second message"]);
    }

    #[test]
    fn execute_plugin_command_bounds_and_sanitizes_guest_logs() {
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "log" (func $log (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "bad\e2\80\aelog")
                    (func (export "example.run") (result i32)
                        (local $i i32)
                        (block $exit
                            (loop $again
                                (br_if $exit (i32.ge_s (local.get $i) (i32.const 70)))
                                (drop (call $log (i32.const 0) (i32.const 9)))
                                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                                br $again
                            )
                        )
                        i32.const 0
                    )
                )
                "#,
            ),
            "example.run",
            &host_context(PathBuf::new()),
        )
        .expect("plugin command should run");

        assert_eq!(execution.logs.len(), 64);
        assert!(
            execution
                .logs
                .iter()
                .all(|line| line == "badlog" && line.chars().count() <= 240)
        );
    }

    #[test]
    fn execute_plugin_command_returns_workspace_root_to_guest() {
        let temp = TestPluginDir::new();
        let expected = temp.root().to_string_lossy().into_owned();
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "workspace_root" (func $root (result i32)))
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (func $alloc (export "kuroya_alloc") (param i32) (result i32)
                        i32.const 1024
                    )
                    (func (export "example.run") (result i32)
                        (local $len i32)
                        (local.set $len (call $root))
                        (drop (call $status (call $alloc (local.get $len)) (local.get $len)))
                        local.get $len
                    )
                )
                "#,
            ),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), workspace_read_capabilities()),
        )
        .expect("plugin command should run");

        assert_eq!(execution.exit_code, i32::try_from(expected.len()).unwrap());
        assert_eq!(execution.status.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn execute_plugin_command_gates_workspace_root_behind_workspace_read() {
        let temp = TestPluginDir::new();
        let guest = r#"
                (module
                    (import "kuroya" "workspace_root" (func $root (result i32)))
                    (memory (export "memory") 1)
                    (func (export "example.run") (result i32)
                        call $root
                    )
                )
                "#;

        let denied = execute_plugin_command_wasm(
            &wasm_bytes(guest),
            "example.run",
            &host_context(temp.root().to_path_buf()),
        )
        .expect("guest should observe the capability denial");
        assert_eq!(denied.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
        assert!(
            denied
                .logs
                .iter()
                .any(|line| line.contains("denied") && line.contains("workspace_root"))
        );

        let granted_without_root = PluginHostContext {
            workspace_root: PathBuf::new(),
            ..host_context_with(temp.root().to_path_buf(), workspace_read_capabilities())
        };
        let no_root =
            execute_plugin_command_wasm(&wasm_bytes(guest), "example.run", &granted_without_root)
                .expect("guest should observe the missing workspace root denial");
        assert_eq!(no_root.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
    }

    #[test]
    fn execute_plugin_command_reports_error_codes_instead_of_trapping() {
        // Malformed status/log arguments used to trap the whole run; they now
        // report the shared negative codes and let the run continue.
        let oversize = PLUGIN_COMMAND_STATUS_MAX_BYTES + 1;
        let guest = format!(
            r#"
                (module
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (import "kuroya" "log" (func $log (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "hello")
                    (func (export "example.status.oversize") (result i32)
                        (call $status (i32.const 0) (i32.const {oversize}))
                    )
                    (func (export "example.status.negative") (result i32)
                        (call $status (i32.const 0) (i32.const -1))
                    )
                    (func (export "example.log.oversize") (result i32)
                        (call $log (i32.const 0) (i32.const {oversize}))
                    )
                )
                "#,
            oversize = oversize
        );

        let status_oversize = execute_plugin_command_wasm(
            &wasm_bytes(&guest),
            "example.status.oversize",
            &host_context(PathBuf::new()),
        )
        .expect("oversize status should return a code, not trap");
        assert_eq!(status_oversize.exit_code, PLUGIN_HOST_ERROR_TOO_LARGE);
        assert_eq!(status_oversize.status, None);

        let status_negative = execute_plugin_command_wasm(
            &wasm_bytes(&guest),
            "example.status.negative",
            &host_context(PathBuf::new()),
        )
        .expect("negative status length should return a code, not trap");
        assert_eq!(status_negative.exit_code, PLUGIN_HOST_ERROR_MISC);

        let log_oversize = execute_plugin_command_wasm(
            &wasm_bytes(&guest),
            "example.log.oversize",
            &host_context(PathBuf::new()),
        )
        .expect("oversize log should return a code, not trap");
        assert_eq!(log_oversize.exit_code, PLUGIN_HOST_ERROR_TOO_LARGE);
        assert!(log_oversize.logs.is_empty());
    }

    #[test]
    fn plugin_host_error_codes_are_unique() {
        let codes = [
            PLUGIN_HOST_ERROR_ALLOC_FAILED,
            PLUGIN_HOST_ERROR_NOT_FOUND,
            PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE,
            PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER,
            PLUGIN_HOST_ERROR_CAPABILITY_DENIED,
            PLUGIN_HOST_ERROR_MISC,
            PLUGIN_HOST_ERROR_TOO_LARGE,
        ];

        for (index, code) in codes.iter().enumerate() {
            assert!(code.is_negative(), "code {code} must be negative");
            assert!(
                !codes[..index].contains(code),
                "error code {code} is used twice"
            );
        }
    }

    #[test]
    fn execute_plugin_command_returns_active_buffer_path_when_permitted() {
        let temp = TestPluginDir::new();
        let active = temp.root().join("src/main.rs");
        let expected = active.to_string_lossy().into_owned();
        let (tx, _rx) = crate::ui_event_channel::ui_event_channel();
        let host = PluginHostContext {
            plugin_id: "example.plugin".to_owned(),
            workspace_root: temp.root().to_path_buf(),
            active_buffer_path: Some(active),
            active_buffer: None,
            capabilities: workspace_read_capabilities(),
            events: tx,
        };
        let wat = r#"
                (module
                    (import "kuroya" "active_buffer_path" (func $active (result i32)))
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (func $alloc (export "kuroya_alloc") (param i32) (result i32)
                        i32.const 1024
                    )
                    (func (export "example.run") (result i32)
                        (local $len i32)
                        (local.set $len (call $active))
                        (if (i32.lt_s (local.get $len) (i32.const 0))
                            (then (return (local.get $len)))
                        )
                        (drop (call $status (call $alloc (local.get $len)) (local.get $len)))
                        local.get $len
                    )
                )
                "#;
        let permitted = execute_plugin_command_wasm(&wasm_bytes(wat), "example.run", &host)
            .expect("permitted plugin command should run");
        assert_eq!(permitted.exit_code, i32::try_from(expected.len()).unwrap());
        assert_eq!(permitted.status.as_deref(), Some(expected.as_str()));

        let without_active = execute_plugin_command_wasm(
            &wasm_bytes(wat),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), workspace_read_capabilities()),
        )
        .expect("guest should observe the negative code");
        assert_eq!(without_active.exit_code, PLUGIN_HOST_ERROR_NO_ACTIVE_BUFFER);
    }

    #[test]
    fn execute_plugin_command_reads_workspace_files_for_capable_plugins() {
        let temp = TestPluginDir::new();
        fs::write(temp.root().join("data.txt"), b"plugin payload").expect("write workspace file");
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "read_file" (func $read (param i32 i32) (result i32)))
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "data.txt")
                    (func $alloc (export "kuroya_alloc") (param i32) (result i32)
                        i32.const 1024
                    )
                    (func (export "example.run") (result i32)
                        (local $n i32)
                        (local.set $n (call $read (i32.const 0) (i32.const 8)))
                        (drop (call $status (call $alloc (local.get $n)) (local.get $n)))
                        local.get $n
                    )
                )
                "#,
            ),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), workspace_read_capabilities()),
        )
        .expect("workspace read plugin command should run");

        assert_eq!(
            execution.exit_code,
            i32::try_from("plugin payload".len()).unwrap()
        );
        assert_eq!(execution.status.as_deref(), Some("plugin payload"));
    }

    #[test]
    fn execute_plugin_command_rejects_reads_outside_workspace_root() {
        let temp = TestPluginDir::new();
        let outside_temp = TestPluginDir::new();
        fs::write(outside_temp.root().join("outside.txt"), b"secret").expect("write outside file");
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(&format!(
                r#"
                (module
                    (import "kuroya" "read_file" (func $read (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "../{}")
                    (func (export "example.run") (result i32)
                        (call $read (i32.const 0) (i32.const {}))
                    )
                )
                "#,
                sibling_file_name(outside_temp.root()),
                "../".len()
                    + outside_temp
                        .root()
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .len()
                    + "/outside.txt".len(),
            )),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), workspace_read_capabilities()),
        )
        .expect("guest should observe the rejection code");

        assert_eq!(execution.exit_code, PLUGIN_HOST_ERROR_ESCAPES_WORKSPACE);
    }

    #[test]
    fn execute_plugin_command_rejects_oversized_workspace_reads() {
        let temp = TestPluginDir::new();
        fs::write(
            temp.root().join("big.bin"),
            vec![0_u8; MAX_PLUGIN_READ_FILE_BYTES + 1],
        )
        .expect("write oversized workspace file");
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "read_file" (func $read (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "big.bin")
                    (func (export "example.run") (result i32)
                        (call $read (i32.const 0) (i32.const 7))
                    )
                )
                "#,
            ),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), workspace_read_capabilities()),
        )
        .expect("guest should observe the oversize rejection code");

        assert_eq!(execution.exit_code, PLUGIN_HOST_ERROR_TOO_LARGE);
    }

    #[test]
    fn execute_plugin_command_denies_workspace_reads_without_capability() {
        let temp = TestPluginDir::new();
        fs::write(temp.root().join("data.txt"), b"plugin payload").expect("write workspace file");
        let guest = r#"
                (module
                    (import "kuroya" "read_file" (func $read (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "data.txt")
                    (func (export "example.run") (result i32)
                        (call $read (i32.const 0) (i32.const 8))
                    )
                )
                "#;
        let denied = execute_plugin_command_wasm(
            &wasm_bytes(guest),
            "example.run",
            &host_context(temp.root().to_path_buf()),
        )
        .expect("guest should observe the capability denial");
        assert_eq!(denied.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
        assert!(
            denied
                .logs
                .iter()
                .any(|line| line.contains("denied") && line.contains("read_file"))
        );

        let granted_without_root = PluginHostContext {
            workspace_root: PathBuf::new(),
            ..host_context(temp.root().to_path_buf())
        };
        let no_root =
            execute_plugin_command_wasm(&wasm_bytes(guest), "example.run", &granted_without_root)
                .expect("guest should observe the missing workspace root denial");
        assert_eq!(no_root.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
    }

    #[test]
    fn execute_plugin_command_gets_and_sets_active_buffer_text() {
        let temp = TestPluginDir::new();
        let path = temp.root().join("notes.md");
        fs::write(&path, b"known text").expect("write captured buffer file");
        let captured = TextBuffer::from_text(1, Some(path.clone()), "known text".to_owned());
        let snapshot = ActiveBufferSnapshot::capture(&captured).expect("buffer has a path");
        let host = host_context_full(
            temp.root().to_path_buf(),
            workspace_read_write_capabilities(),
            Some(snapshot),
        );

        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "buffer_get_text" (func $get (param i32 i32) (result i32)))
                    (import "kuroya" "buffer_set_text" (func $set (param i32 i32 i32 i32) (result i32)))
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "notes.md")
                    (data (i32.const 16) "stale staged text")
                    (data (i32.const 48) "replacement text")
                    (func $alloc (export "kuroya_alloc") (param i32) (result i32)
                        i32.const 2048
                    )
                    (func (export "example.run") (result i32)
                        (local $n i32)
                        (local.set $n (call $get (i32.const 0) (i32.const 8)))
                        (if (i32.lt_s (local.get $n) (i32.const 0))
                            (then (return (local.get $n)))
                        )
                        (drop (call $status (i32.const 2048) (local.get $n)))
                        (drop (call $set (i32.const 0) (i32.const 8) (i32.const 16) (i32.const 17)))
                        (call $set (i32.const 0) (i32.const 8) (i32.const 48) (i32.const 16))
                    )
                )
                "#,
            ),
            "example.run",
            &host,
        )
        .expect("buffer text plugin command should run");

        assert_eq!(execution.exit_code, 0);
        assert_eq!(execution.status.as_deref(), Some("known text"));
        // The last buffer_set_text call wins; the path is the buffer's own
        // stored path so the UI handler can match it exactly.
        assert_eq!(
            execution.pending_buffer_text,
            Some((path, "replacement text".to_owned()))
        );
    }

    #[test]
    fn execute_plugin_command_gates_buffer_text_by_read_and_write() {
        let temp = TestPluginDir::new();
        let path = temp.root().join("notes.md");
        fs::write(&path, b"known text").expect("write captured buffer file");
        let captured = TextBuffer::from_text(1, Some(path), "known text".to_owned());
        // workspace_read alone: buffer_get_text is a read, so it is allowed.
        let read_only = host_context_full(
            temp.root().to_path_buf(),
            workspace_read_capabilities(),
            ActiveBufferSnapshot::capture(&captured),
        );
        // workspace_write alone without workspace_read: neither call runs.
        let write_only = host_context_full(
            temp.root().to_path_buf(),
            workspace_write_capabilities(),
            ActiveBufferSnapshot::capture(&captured),
        );
        let guest = r#"
                (module
                    (import "kuroya" "buffer_get_text" (func $get (param i32 i32) (result i32)))
                    (import "kuroya" "buffer_set_text" (func $set (param i32 i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "notes.md")
                    (func $alloc (export "kuroya_alloc") (param i32) (result i32)
                        i32.const 2048
                    )
                    (func (export "example.get") (result i32)
                        (call $get (i32.const 0) (i32.const 8))
                    )
                    (func (export "example.set") (result i32)
                        (call $set (i32.const 0) (i32.const 8) (i32.const 0) (i32.const 8))
                    )
                )
                "#;

        let get = execute_plugin_command_wasm(&wasm_bytes(guest), "example.get", &read_only)
            .expect("workspace_read should allow buffer_get_text");
        assert_eq!(get.exit_code, i32::try_from("known text".len()).unwrap());

        let set = execute_plugin_command_wasm(&wasm_bytes(guest), "example.set", &read_only)
            .expect("guest should observe the write denial");
        assert_eq!(set.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
        assert_eq!(set.pending_buffer_text, None);
        assert!(
            set.logs
                .iter()
                .any(|line| line.contains("denied") && line.contains("buffer_set_text"))
        );

        let denied_get =
            execute_plugin_command_wasm(&wasm_bytes(guest), "example.get", &write_only)
                .expect("guest should observe the read denial");
        assert_eq!(denied_get.exit_code, PLUGIN_HOST_ERROR_CAPABILITY_DENIED);
        assert!(
            denied_get
                .logs
                .iter()
                .any(|line| line.contains("denied") && line.contains("buffer_get_text"))
        );
    }

    #[test]
    fn execute_plugin_command_buffer_text_rejects_mismatched_paths() {
        let temp = TestPluginDir::new();
        let captured_path = temp.root().join("notes.md");
        fs::write(&captured_path, b"known text").expect("write captured buffer file");
        fs::write(temp.root().join("other.md"), b"other").expect("write other workspace file");
        let captured = TextBuffer::from_text(1, Some(captured_path), "known text".to_owned());
        let host = host_context_full(
            temp.root().to_path_buf(),
            workspace_read_write_capabilities(),
            ActiveBufferSnapshot::capture(&captured),
        );
        let guest = r#"
                (module
                    (import "kuroya" "buffer_get_text" (func $get (param i32 i32) (result i32)))
                    (import "kuroya" "buffer_set_text" (func $set (param i32 i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "other.md")
                    (func (export "example.get") (result i32)
                        (call $get (i32.const 0) (i32.const 8))
                    )
                    (func (export "example.set") (result i32)
                        (call $set (i32.const 0) (i32.const 8) (i32.const 0) (i32.const 5))
                    )
                )
                "#;

        let get = execute_plugin_command_wasm(&wasm_bytes(guest), "example.get", &host)
            .expect("guest should observe the path mismatch");
        let set = execute_plugin_command_wasm(&wasm_bytes(guest), "example.set", &host)
            .expect("guest should observe the path mismatch");

        assert_eq!(get.exit_code, PLUGIN_HOST_ERROR_NOT_FOUND);
        assert_eq!(set.exit_code, PLUGIN_HOST_ERROR_NOT_FOUND);
        assert_eq!(set.pending_buffer_text, None);
    }

    #[test]
    fn execute_plugin_command_buffer_text_rejects_oversized_text() {
        let temp = TestPluginDir::new();
        let path = temp.root().join("notes.md");
        fs::write(&path, b"known text").expect("write captured buffer file");
        let captured = TextBuffer::from_text(1, Some(path), "known text".to_owned());
        let mut snapshot = ActiveBufferSnapshot::capture(&captured).expect("buffer has a path");
        snapshot.too_large = true;
        let host = host_context_full(
            temp.root().to_path_buf(),
            workspace_read_write_capabilities(),
            Some(snapshot),
        );
        let guest = format!(
            r#"
                (module
                    (import "kuroya" "buffer_get_text" (func $get (param i32 i32) (result i32)))
                    (import "kuroya" "buffer_set_text" (func $set (param i32 i32 i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "notes.md")
                    (func (export "example.get") (result i32)
                        (call $get (i32.const 0) (i32.const 8))
                    )
                    (func (export "example.set") (result i32)
                        (call $set (i32.const 0) (i32.const 8) (i32.const 0) (i32.const {}))
                    )
                )
                "#,
            PLUGIN_COMMAND_BUFFER_CAPTURE_MAX + 1
        );

        let get = execute_plugin_command_wasm(&wasm_bytes(&guest), "example.get", &host)
            .expect("guest should observe the oversized capture");
        let set = execute_plugin_command_wasm(&wasm_bytes(&guest), "example.set", &host)
            .expect("guest should observe the oversized staged text");

        assert_eq!(get.exit_code, PLUGIN_HOST_ERROR_TOO_LARGE);
        assert_eq!(set.exit_code, PLUGIN_HOST_ERROR_TOO_LARGE);
        assert_eq!(set.pending_buffer_text, None);
    }

    #[test]
    fn plugin_wall_clock_budget_exhausts_after_limit_and_not_before() {
        let now = Instant::now();
        assert!(plugin_wall_clock_budget_exhausted(
            now - Duration::from_secs(31),
            now,
            PLUGIN_COMMAND_WALL_CLOCK_LIMIT
        ));
        // The boundary is inclusive: a run as old as the limit is exhausted.
        assert!(plugin_wall_clock_budget_exhausted(
            now - PLUGIN_COMMAND_WALL_CLOCK_LIMIT,
            now,
            PLUGIN_COMMAND_WALL_CLOCK_LIMIT
        ));
        // A fresh run keeps its budget.
        assert!(!plugin_wall_clock_budget_exhausted(
            now,
            now,
            PLUGIN_COMMAND_WALL_CLOCK_LIMIT
        ));
        // The limit is a parameter so tests can pin it without faking Instant.
        assert!(!plugin_wall_clock_budget_exhausted(
            now - Duration::from_secs(1),
            now,
            Duration::from_secs(2)
        ));
        assert!(plugin_wall_clock_budget_exhausted(
            now - Duration::from_secs(2),
            now,
            Duration::from_secs(2)
        ));
    }

    #[test]
    fn plugin_command_trap_error_text_reports_budget_and_fuel_distinctly() {
        // A run the wall-clock checkpoint cut short gets the budget message
        // instead of the raw out-of-fuel trap text.
        assert_eq!(
            plugin_command_trap_error_text(
                "all fuel consumed by WebAssembly",
                true,
                "example.plugin"
            ),
            "Plugin example.plugin exceeded its 30s time budget"
        );
        // Plain fuel exhaustion keeps the generic fuel wording (wasmi 0.46
        // renders TrapCode::OutOfFuel as "all fuel consumed by WebAssembly").
        assert_eq!(
            plugin_command_trap_error_text(
                "all fuel consumed by WebAssembly",
                false,
                "example.plugin"
            ),
            "plugin command exceeded the execution fuel limit"
        );
        assert_eq!(
            plugin_command_trap_error_text("Trapped: Out Of Fuel", false, "example.plugin"),
            "plugin command exceeded the execution fuel limit"
        );
        // Non-fuel traps keep the raw trap text.
        assert_eq!(
            plugin_command_trap_error_text("unknown trap", false, "example.plugin"),
            "plugin command trapped: unknown trap"
        );
        // The plugin id is sanitized and bounded like other status text.
        let hostile = plugin_command_trap_error_text(
            "unknown trap",
            true,
            &format!("bad\n{}\u{202e}", "y".repeat(200)),
        );
        assert!(hostile.contains("exceeded its 30s time budget"));
        assert!(!hostile.chars().any(char::is_control));
        assert!(!hostile.contains('\u{202e}'));
        assert!(hostile.contains("..."));
    }

    #[test]
    fn execute_plugin_command_host_call_loop_completes_within_wall_clock_budget() {
        // A guest that makes many host calls must keep completing: the
        // wall-clock checkpoint installed in every host call must not
        // exhaust fuel before the 30s budget is actually spent.
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "status" (func $status (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "tick")
                    (func (export "example.run") (result i32)
                        (local $i i32)
                        (block $exit
                            (loop $again
                                (br_if $exit (i32.ge_s (local.get $i) (i32.const 50000)))
                                (drop (call $status (i32.const 0) (i32.const 4)))
                                (local.set $i (i32.add (local.get $i) (i32.const 1)))
                                br $again
                            )
                        )
                        local.get $i
                    )
                )
                "#,
            ),
            "example.run",
            &host_context(PathBuf::new()),
        )
        .expect("host-calling loop should finish well inside the wall-clock budget");

        assert_eq!(execution.exit_code, 50000);
        assert_eq!(execution.status.as_deref(), Some("tick"));
    }

    #[test]
    fn execute_plugin_command_requests_buffer_open_via_ui_event() {
        let temp = TestPluginDir::new();
        fs::write(temp.root().join("notes.md"), b"# notes").expect("write notes file");
        let (tx, rx) = crate::ui_event_channel::ui_event_channel();
        let host = PluginHostContext {
            plugin_id: "example.plugin".to_owned(),
            workspace_root: temp.root().to_path_buf(),
            active_buffer_path: None,
            active_buffer: None,
            capabilities: workspace_read_capabilities(),
            events: tx,
        };
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "open_buffer" (func $open (param i32 i32) (result i32)))
                    (memory (export "memory") 1)
                    (data (i32.const 0) "notes.md")
                    (func (export "example.run") (result i32)
                        (call $open (i32.const 0) (i32.const 8))
                    )
                )
                "#,
            ),
            "example.run",
            &host,
        )
        .expect("open_buffer plugin command should run");

        assert_eq!(execution.exit_code, 0);
        match rx
            .recv_timeout(Duration::from_secs(1))
            .expect("open request event")
        {
            UiEvent::PluginOpenFileRequested { plugin_id, path } => {
                assert_eq!(plugin_id, "example.plugin");
                assert_eq!(path, temp.root().join("notes.md").canonicalize().unwrap());
            }
            other => panic!("unexpected ui event: {other:?}"),
        }
    }

    #[test]
    fn execute_plugin_command_reports_missing_kuroya_alloc() {
        let temp = TestPluginDir::new();
        let read_caps = workspace_read_capabilities();
        let absent = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "workspace_root" (func $root (result i32)))
                    (memory (export "memory") 1)
                    (func (export "example.run") (result i32)
                        call $root
                    )
                )
                "#,
            ),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), read_caps.clone()),
        )
        .expect("guest should observe the missing alloc code");
        assert_eq!(absent.exit_code, PLUGIN_HOST_ERROR_ALLOC_FAILED);

        let failing = execute_plugin_command_wasm(
            &wasm_bytes(
                r#"
                (module
                    (import "kuroya" "workspace_root" (func $root (result i32)))
                    (memory (export "memory") 1)
                    (func (export "kuroya_alloc") (param i32) (result i32)
                        i32.const -1
                    )
                    (func (export "example.run") (result i32)
                        call $root
                    )
                )
                "#,
            ),
            "example.run",
            &host_context_with(temp.root().to_path_buf(), read_caps),
        )
        .expect("guest should observe the failed alloc code");
        assert_eq!(failing.exit_code, PLUGIN_HOST_ERROR_ALLOC_FAILED);
    }

    #[test]
    fn example_plugin_wat_module_compiles_and_runs_against_host_abi() {
        // The shipped example module is documentation as well as a fixture:
        // its status/log imports return i32, so its call sites must drop the
        // results or the module fails validation, and it must keep running
        // against the live linker unchanged.
        let wat = include_str!("../../../examples/plugin-example/plugin.wat");
        let execution = execute_plugin_command_wasm(
            &wasm_bytes(wat),
            "example.hello",
            &host_context(PathBuf::new()),
        )
        .expect("example plugin module should compile and run");

        assert_eq!(execution.exit_code, 0);
        assert_eq!(
            execution.status.as_deref(),
            Some("Hello from Kuroya example plugin")
        );
        assert_eq!(execution.logs, vec!["example plugin ran"]);
    }

    fn wasm_bytes(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).expect("test wat should compile")
    }

    fn runtime_with_entry(root: &Path, entry: PathBuf) -> PluginRuntimeRegistration {
        PluginRuntimeRegistration {
            plugin_id: "example.plugin".to_owned(),
            name: "Example".to_owned(),
            version: "0.1.0".to_owned(),
            root: root.to_path_buf(),
            entry: Some(entry),
            activation_events: vec![PluginActivationEvent::OnCommand("example.run".to_owned())],
            capabilities: PluginCapabilities {
                commands: true,
                ..PluginCapabilities::default()
            },
        }
    }

    fn host_context(workspace_root: PathBuf) -> PluginHostContext {
        host_context_with(
            workspace_root,
            PluginCapabilities {
                commands: true,
                ..PluginCapabilities::default()
            },
        )
    }

    fn host_context_with(
        workspace_root: PathBuf,
        capabilities: PluginCapabilities,
    ) -> PluginHostContext {
        host_context_full(workspace_root, capabilities, None)
    }

    fn host_context_full(
        workspace_root: PathBuf,
        capabilities: PluginCapabilities,
        active_buffer: Option<ActiveBufferSnapshot>,
    ) -> PluginHostContext {
        let (tx, _rx) = crate::ui_event_channel::ui_event_channel();
        PluginHostContext {
            plugin_id: "example.plugin".to_owned(),
            workspace_root,
            active_buffer_path: active_buffer.as_ref().map(|snapshot| snapshot.path.clone()),
            active_buffer,
            capabilities,
            events: tx,
        }
    }

    fn workspace_read_capabilities() -> PluginCapabilities {
        PluginCapabilities {
            commands: true,
            workspace_read: true,
            ..PluginCapabilities::default()
        }
    }

    fn workspace_write_capabilities() -> PluginCapabilities {
        PluginCapabilities {
            commands: true,
            workspace_write: true,
            ..PluginCapabilities::default()
        }
    }

    fn workspace_read_write_capabilities() -> PluginCapabilities {
        PluginCapabilities {
            commands: true,
            workspace_read: true,
            workspace_write: true,
            ..PluginCapabilities::default()
        }
    }

    fn run_runtime_command(
        runtime: &PluginRuntimeRegistration,
        command_id: &str,
    ) -> anyhow::Result<super::PluginCommandExecution> {
        let (tx, _rx) = crate::ui_event_channel::ui_event_channel();
        let host = PluginHostContext {
            plugin_id: runtime.plugin_id.clone(),
            workspace_root: PathBuf::new(),
            active_buffer_path: None,
            active_buffer: None,
            capabilities: runtime.capabilities.clone(),
            events: tx,
        };
        execute_plugin_command(runtime, command_id, &host)
    }

    fn sibling_file_name(root: &Path) -> String {
        root.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "outside".to_owned())
    }

    struct TestPluginDir {
        root: PathBuf,
    }

    impl TestPluginDir {
        fn new() -> Self {
            let mut root = std::env::temp_dir();
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let counter = TEST_PLUGIN_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
            root.push(format!(
                "kuroya-plugin-test-{}-{unique}-{counter}",
                std::process::id(),
            ));
            fs::create_dir_all(&root).expect("create temp plugin dir");
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }

        fn write_wasm(&self, name: &str, wat: &str) -> PathBuf {
            let relative = Path::new(name);
            let path = normalize_child_path(&self.root, relative).expect("test child path");
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create test wasm parent");
            }
            fs::write(&path, wasm_bytes(wat)).expect("write test wasm");
            path
        }
    }

    impl Drop for TestPluginDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}
