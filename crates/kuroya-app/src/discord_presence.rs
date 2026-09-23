use crate::KuroyaApp;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::sync::mpsc;
use std::time::Duration;

const DISCORD_PRESENCE_TICK: Duration = Duration::from_secs(20);

pub(crate) const DISCORD_PRESENCE_SHUTDOWN_LIMIT: Duration = Duration::from_millis(500);

const DISCORD_IPC_SLOTS: usize = 10;

const DISCORD_PRESENCE_TEXT_MAX_BYTES: usize = 128;

const MAX_DISCORD_FRAME_PAYLOAD_BYTES: usize = 1024 * 1024;
const OPCODE_HANDSHAKE: u32 = 0;
const OPCODE_FRAME: u32 = 1;

const DISCORD_LARGE_IMAGE_KEY: &str = "kuroya";

pub(crate) enum DiscordPresenceCommand {
    Update {
        details: Option<String>,
        state: Option<String>,
        show_elapsed: bool,
    },
    Clear,
    Shutdown,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PresenceActivity {
    pub(crate) details: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) show_elapsed: bool,
}

pub(crate) struct DiscordPresenceRuntime {
    thread: DiscordPresenceThread,
    client_id: String,
}

impl DiscordPresenceRuntime {
    fn new(thread: DiscordPresenceThread, client_id: String) -> Self {
        Self { thread, client_id }
    }

    pub(crate) fn client_id(&self) -> &str {
        &self.client_id
    }

    pub(crate) fn send_update(
        &self,
        details: Option<String>,
        state: Option<String>,
        show_elapsed: bool,
    ) {
        self.thread.send_update(details, state, show_elapsed);
    }

    pub(crate) fn clear_and_shutdown(&mut self) {
        self.thread
            .clear_and_shutdown(DISCORD_PRESENCE_SHUTDOWN_LIMIT);
    }
}

pub(crate) struct DiscordPresenceThread {
    tx: Option<mpsc::Sender<DiscordPresenceCommand>>,
    exit_rx: mpsc::Receiver<()>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl DiscordPresenceThread {
    pub(crate) fn send_update(
        &self,
        details: Option<String>,
        state: Option<String>,
        show_elapsed: bool,
    ) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(DiscordPresenceCommand::Update {
                details,
                state,
                show_elapsed,
            });
        }
    }

    pub(crate) fn clear_and_shutdown(&mut self, limit: Duration) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(DiscordPresenceCommand::Clear);
            let _ = tx.send(DiscordPresenceCommand::Shutdown);
            drop(tx);
        }
        if let Err(mpsc::RecvTimeoutError::Disconnected) = self.exit_rx.recv_timeout(limit)
            && let Some(join) = self.join.take()
        {
            let _ = join.join();
        }
    }
}

impl Drop for DiscordPresenceThread {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.take() {
            let _ = tx.send(DiscordPresenceCommand::Shutdown);
        }
    }
}

pub(crate) fn spawn_discord_presence(
    client_id: String,
) -> (mpsc::Sender<DiscordPresenceCommand>, DiscordPresenceThread) {
    spawn_discord_presence_loop(
        SystemDiscordConnector { client_id },
        DISCORD_PRESENCE_TICK,
        Box::new(system_unix_time),
    )
}

fn spawn_discord_presence_loop<C>(
    connector: C,
    tick: Duration,
    clock: Box<dyn Fn() -> u64 + Send>,
) -> (mpsc::Sender<DiscordPresenceCommand>, DiscordPresenceThread)
where
    C: DiscordPresenceConnector + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("discord-presence".to_owned())
        .spawn(move || {
            let _exit_signal = exit_tx;
            run_presence_loop(rx, connector, tick, clock);
        });
    let thread = match spawned {
        Ok(join) => DiscordPresenceThread {
            tx: Some(tx.clone()),
            exit_rx,
            join: Some(join),
        },

        Err(_) => DiscordPresenceThread {
            tx: None,
            exit_rx,
            join: None,
        },
    };
    (tx, thread)
}

fn run_presence_loop<C>(
    rx: mpsc::Receiver<DiscordPresenceCommand>,
    mut connector: C,
    tick: Duration,
    clock: Box<dyn Fn() -> u64 + Send>,
) where
    C: DiscordPresenceConnector,
{
    let pid = std::process::id();
    let mut transport: Option<C::Transport> = None;

    let mut pending: Option<PresenceActivity> = None;

    let mut last_sent: Option<PresenceActivity> = None;

    let mut start_unix: Option<u64> = None;
    let mut nonce_counter: u64 = 0;

    loop {
        if transport.is_none()
            && let Ok(new_transport) = connector.connect()
        {
            transport = Some(new_transport);
            start_unix = None;
            let reconnecting = pending.take().or_else(|| last_sent.clone());
            if let Some(activity) = reconnecting {
                let last_state = last_sent.as_ref().and_then(|sent| sent.state.as_deref());
                if send_activity(
                    &mut transport,
                    pid,
                    &activity,
                    last_state,
                    &mut start_unix,
                    &clock,
                    next_nonce(pid, &mut nonce_counter),
                ) {
                    last_sent = Some(activity);
                }
            }
        }
        match rx.recv_timeout(tick) {
            Ok(DiscordPresenceCommand::Update {
                details,
                state,
                show_elapsed,
            }) => {
                let activity = PresenceActivity {
                    details,
                    state,
                    show_elapsed,
                };
                if transport.is_some() {
                    let last_state = last_sent.as_ref().and_then(|sent| sent.state.as_deref());
                    if send_activity(
                        &mut transport,
                        pid,
                        &activity,
                        last_state,
                        &mut start_unix,
                        &clock,
                        next_nonce(pid, &mut nonce_counter),
                    ) {
                        last_sent = Some(activity);
                    } else {
                        pending = Some(activity);
                    }
                } else {
                    pending = Some(activity);
                }
            }
            Ok(DiscordPresenceCommand::Clear) => {
                pending = None;
                last_sent = None;
                start_unix = None;
                if let Some(live) = transport.as_mut() {
                    let payload = clear_activity_payload(pid, &next_nonce(pid, &mut nonce_counter));
                    if live
                        .send_frame(OPCODE_FRAME, payload.to_string().as_bytes())
                        .is_err()
                    {
                        transport = None;
                    }
                }
            }
            Ok(DiscordPresenceCommand::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(live) = transport.as_mut()
                    && last_sent.is_some()
                {
                    let payload = clear_activity_payload(pid, &next_nonce(pid, &mut nonce_counter));
                    let _ = live.send_frame(OPCODE_FRAME, payload.to_string().as_bytes());
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if transport.is_some()
                    && let Some(activity) = last_sent.clone()
                {
                    let elapsed = activity.show_elapsed.then_some(start_unix).flatten();
                    let payload = presence_payload(
                        pid,
                        activity.details.as_deref(),
                        activity.state.as_deref(),
                        elapsed,
                        &next_nonce(pid, &mut nonce_counter),
                    );
                    if let Some(live) = transport.as_mut()
                        && live
                            .send_frame(OPCODE_FRAME, payload.to_string().as_bytes())
                            .is_err()
                    {
                        transport = None;
                    }
                }
            }
        }
    }
}

fn next_nonce(pid: u32, nonce_counter: &mut u64) -> String {
    *nonce_counter += 1;
    format!("kuroya-{pid}-{nonce_counter}")
}

fn send_activity<T: DiscordPresenceTransport>(
    transport: &mut Option<T>,
    pid: u32,
    activity: &PresenceActivity,
    last_state: Option<&str>,
    start_unix: &mut Option<u64>,
    clock: &dyn Fn() -> u64,
    nonce: String,
) -> bool {
    let state_changed = last_state != activity.state.as_deref();
    if activity.show_elapsed && (start_unix.is_none() || state_changed) {
        *start_unix = Some(clock());
    }
    let elapsed = activity.show_elapsed.then_some(*start_unix).flatten();
    let payload = presence_payload(
        pid,
        activity.details.as_deref(),
        activity.state.as_deref(),
        elapsed,
        &nonce,
    );
    let Some(live) = transport.as_mut() else {
        return false;
    };
    if live
        .send_frame(OPCODE_FRAME, payload.to_string().as_bytes())
        .is_ok()
    {
        true
    } else {
        *transport = None;
        false
    }
}

fn system_unix_time() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn presence_labels_for_frame(
    file_name: Option<&str>,
    workspace_name: Option<&str>,
    workspace_placeholder: bool,
    show_details: bool,
    show_workspace: bool,
) -> (Option<String>, Option<String>) {
    let details = show_details.then(|| match file_name {
        Some(name) => format!("Editing {name}"),
        None => "Idle".to_owned(),
    });
    let state = show_workspace.then(|| {
        if workspace_placeholder {
            "In Kuroya".to_owned()
        } else {
            format!("In {}", workspace_name.unwrap_or("Kuroya"))
        }
    });
    (details, state)
}

impl KuroyaApp {
    pub(crate) fn sync_discord_presence_runtime(&mut self) {
        if !self.settings.discord.presence_is_configurable() {
            self.shutdown_discord_presence();
            return;
        }
        let client_id = self.settings.discord.client_id.clone();
        if self
            .discord_presence
            .as_ref()
            .is_some_and(|runtime| runtime.client_id() == client_id)
        {
            return;
        }
        self.shutdown_discord_presence();
        let (_, thread) = spawn_discord_presence(client_id.clone());
        self.discord_presence = Some(DiscordPresenceRuntime::new(thread, client_id));
        self.discord_presence_sent = None;
    }

    pub(crate) fn update_discord_presence(&mut self) {
        let Some(next) = self.discord_presence_activity() else {
            return;
        };
        if self.discord_presence_sent.as_ref() == Some(&next) {
            return;
        }
        if let Some(runtime) = &self.discord_presence {
            runtime.send_update(next.details.clone(), next.state.clone(), next.show_elapsed);
            self.discord_presence_sent = Some(next);
        }
    }

    fn discord_presence_activity(&self) -> Option<PresenceActivity> {
        self.discord_presence.as_ref()?;
        let discord = &self.settings.discord;
        let file_name = self
            .active_buffer()
            .and_then(kuroya_core::TextBuffer::path)
            .and_then(|path| path.file_name())
            .and_then(OsStr::to_str);
        let workspace_name = self.workspace.root.file_name().and_then(OsStr::to_str);
        let (details, state) = presence_labels_for_frame(
            file_name,
            workspace_name,
            self.workspace_placeholder,
            discord.show_details,
            discord.show_workspace,
        );
        Some(PresenceActivity {
            details,
            state,
            show_elapsed: discord.show_elapsed,
        })
    }

    pub(crate) fn shutdown_discord_presence(&mut self) {
        if let Some(mut runtime) = self.discord_presence.take() {
            runtime.clear_and_shutdown();
        }
        self.discord_presence_sent = None;
    }
}

fn presence_payload(
    pid: u32,
    details: Option<&str>,
    state: Option<&str>,
    start_unix: Option<u64>,
    nonce: &str,
) -> Value {
    let mut activity = json!({
        "assets": {
            "large_image": DISCORD_LARGE_IMAGE_KEY,
            "large_text": "Kuroya",
        },
        "instance": false,
    });
    if let Some(details) = details {
        activity["details"] = Value::String(clamp_presence_text(details).to_owned());
    }
    if let Some(state) = state {
        activity["state"] = Value::String(clamp_presence_text(state).to_owned());
    }
    if let Some(start) = start_unix {
        activity["timestamps"] = json!({ "start": start });
    }
    json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": pid, "activity": activity },
        "nonce": nonce,
    })
}

fn clear_activity_payload(pid: u32, nonce: &str) -> Value {
    json!({
        "cmd": "SET_ACTIVITY",
        "args": { "pid": pid, "activity": Value::Null },
        "nonce": nonce,
    })
}

fn handshake_payload(client_id: &str) -> String {
    json!({ "v": 1, "client_id": client_id }).to_string()
}

fn clamp_presence_text(text: &str) -> &str {
    if text.len() <= DISCORD_PRESENCE_TEXT_MAX_BYTES {
        return text;
    }
    let mut end = DISCORD_PRESENCE_TEXT_MAX_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiscordFrame {
    pub(crate) opcode: u32,
    pub(crate) payload: Vec<u8>,
}

fn encode_frame(opcode: u32, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(8 + payload.len());
    frame.extend_from_slice(&opcode.to_le_bytes());
    frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    frame.extend_from_slice(payload);
    frame
}

fn decode_frame_header(header: &[u8; 8]) -> (u32, u32) {
    let mut opcode_bytes = [0u8; 4];
    let mut length_bytes = [0u8; 4];
    opcode_bytes.copy_from_slice(&header[..4]);
    length_bytes.copy_from_slice(&header[4..]);
    (
        u32::from_le_bytes(opcode_bytes),
        u32::from_le_bytes(length_bytes),
    )
}

fn read_frame_from(reader: &mut impl Read) -> io::Result<DiscordFrame> {
    let mut header = [0u8; 8];
    reader.read_exact(&mut header)?;
    let (opcode, length) = decode_frame_header(&header);
    let Ok(length) = usize::try_from(length) else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "discord frame length is not representable",
        ));
    };
    if length > MAX_DISCORD_FRAME_PAYLOAD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "discord frame payload is unreasonably large",
        ));
    }
    let mut payload = vec![0u8; length];
    reader.read_exact(&mut payload)?;
    Ok(DiscordFrame { opcode, payload })
}

pub(crate) trait DiscordPresenceTransport {
    fn send_frame(&mut self, opcode: u32, payload: &[u8]) -> io::Result<()>;
    fn read_frame(&mut self) -> io::Result<DiscordFrame>;
}

trait DiscordPresenceConnector {
    type Transport: DiscordPresenceTransport;
    fn connect(&mut self) -> io::Result<Self::Transport>;
}

struct SystemDiscordConnector {
    client_id: String,
}

impl DiscordPresenceConnector for SystemDiscordConnector {
    type Transport = SystemIpcStream;

    fn connect(&mut self) -> io::Result<SystemIpcStream> {
        let mut stream = open_discord_ipc()?;
        let payload = handshake_payload(&self.client_id);
        stream.send_frame(OPCODE_HANDSHAKE, payload.as_bytes())?;

        let _ready = stream.read_frame()?;
        Ok(stream)
    }
}

enum SystemIpcStream {
    #[cfg(windows)]
    Pipe(std::fs::File),
    #[cfg(unix)]
    Socket(std::os::unix::net::UnixStream),
}

impl DiscordPresenceTransport for SystemIpcStream {
    fn send_frame(&mut self, opcode: u32, payload: &[u8]) -> io::Result<()> {
        self.write_all(&encode_frame(opcode, payload))
    }

    fn read_frame(&mut self) -> io::Result<DiscordFrame> {
        read_frame_from(self)
    }
}

impl Read for SystemIpcStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.read(buf),
            #[cfg(unix)]
            Self::Socket(socket) => socket.read(buf),
        }
    }
}

impl Write for SystemIpcStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.write(buf),
            #[cfg(unix)]
            Self::Socket(socket) => socket.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            #[cfg(windows)]
            Self::Pipe(pipe) => pipe.flush(),
            #[cfg(unix)]
            Self::Socket(socket) => socket.flush(),
        }
    }
}

#[cfg(windows)]
fn open_discord_ipc() -> io::Result<SystemIpcStream> {
    for slot in 0..DISCORD_IPC_SLOTS {
        for prefix in ["\\\\.\\pipe\\", "\\\\?\\pipe\\"] {
            let path = format!("{prefix}discord-ipc-{slot}");
            if let Ok(pipe) = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            {
                return Ok(SystemIpcStream::Pipe(pipe));
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no discord-ipc pipe is open",
    ))
}

#[cfg(unix)]
fn open_discord_ipc() -> io::Result<SystemIpcStream> {
    for path in discord_ipc_socket_candidates() {
        if let Ok(socket) = std::os::unix::net::UnixStream::connect(&path) {
            return Ok(SystemIpcStream::Socket(socket));
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no discord-ipc socket is listening",
    ))
}

#[cfg(unix)]
fn discord_ipc_socket_candidates() -> Vec<std::path::PathBuf> {
    let mut bases: Vec<std::path::PathBuf> = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(std::path::PathBuf::from)
        .collect();
    bases.sort();
    bases.dedup();
    bases.push(std::path::PathBuf::from("/run/user").join(current_user_uid()));

    let mut candidates = Vec::new();
    for base in bases {
        for directory in [
            "",
            "app/com.discordapp.Discord/",
            ".var/app/com.discordapp.Discord/.cache/",
        ] {
            for slot in 0..DISCORD_IPC_SLOTS {
                candidates.push(base.join(format!("{directory}discord-ipc-{slot}")));
            }
        }
    }
    candidates
}

#[cfg(unix)]
fn current_user_uid() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("Uid:")
                && let Some(uid) = rest.split_whitespace().next()
            {
                return uid.to_owned();
            }
        }
    }
    "1000".to_owned()
}

#[cfg(test)]
mod tests {
    use super::{
        DISCORD_PRESENCE_SHUTDOWN_LIMIT, DISCORD_PRESENCE_TEXT_MAX_BYTES, DiscordFrame,
        DiscordPresenceCommand, DiscordPresenceRuntime, DiscordPresenceThread,
        DiscordPresenceTransport, OPCODE_FRAME, OPCODE_HANDSHAKE, clamp_presence_text,
        clear_activity_payload, decode_frame_header, encode_frame, handshake_payload,
        presence_labels_for_frame, presence_payload, read_frame_from, run_presence_loop,
        spawn_discord_presence_loop,
    };
    use crate::{
        KuroyaApp, app_startup_context::AppStartupContext, terminal::TerminalPane,
        ui_event_channel::ui_event_channel,
    };
    use kuroya_core::{EditorSettings, TextBuffer, Workspace};
    use serde_json::{Value, json};
    use std::{
        io::{self, Cursor},
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc,
        },
        time::{Duration, Instant},
    };
    use tokio::runtime::Runtime;

    #[test]
    fn frame_round_trips_opcode_and_payload_through_the_le_header() {
        let payload = br#"{"cmd":"SET_ACTIVITY"}"#.as_slice();
        let frame = encode_frame(OPCODE_FRAME, payload);

        assert_eq!(frame[..4], OPCODE_FRAME.to_le_bytes());
        assert_eq!(frame[4..8], (payload.len() as u32).to_le_bytes());

        let decoded = read_frame_from(&mut Cursor::new(frame)).unwrap();
        assert_eq!(decoded.opcode, OPCODE_FRAME);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn frame_header_decodes_little_endian_opcode_and_length() {
        let header = [0x01, 0x00, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00];

        assert_eq!(decode_frame_header(&header), (1, 42));
    }

    #[test]
    fn frame_read_rejects_oversized_payload_lengths() {
        let mut header = [0u8; 8];
        header[..4].copy_from_slice(&OPCODE_FRAME.to_le_bytes());
        header[4..].copy_from_slice(&u32::MAX.to_le_bytes());

        let error = read_frame_from(&mut Cursor::new(header)).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn frame_read_fails_on_truncated_payload() {
        let frame = encode_frame(OPCODE_HANDSHAKE, b"{\"v\":1}");

        assert!(read_frame_from(&mut Cursor::new(&frame[..frame.len() - 1])).is_err());
    }

    #[test]
    fn handshake_payload_uses_protocol_version_one_and_the_client_id() {
        assert_eq!(
            handshake_payload("1234567890123456789"),
            json!({"v": 1, "client_id": "1234567890123456789"}).to_string()
        );
    }

    #[test]
    fn presence_payload_contains_only_labels_and_the_kuroya_asset() {
        let payload = presence_payload(
            4242,
            Some("main.rs"),
            Some("kuroya"),
            Some(1_700_000_000),
            "n1",
        );

        assert_eq!(payload["cmd"], "SET_ACTIVITY");
        assert_eq!(payload["args"]["pid"], 4242);
        assert_eq!(payload["nonce"], "n1");
        assert_eq!(payload["args"]["activity"]["details"], "main.rs");
        assert_eq!(payload["args"]["activity"]["state"], "kuroya");
        assert_eq!(
            payload["args"]["activity"]["assets"]["large_image"],
            "kuroya"
        );
        assert_eq!(payload["args"]["activity"]["instance"], false);
        assert_eq!(
            payload["args"]["activity"]["timestamps"]["start"],
            1_700_000_000
        );
    }

    #[test]
    fn presence_payload_omits_the_timestamp_block_without_an_anchor() {
        let payload = presence_payload(4242, Some("main.rs"), Some("kuroya"), None, "n2");

        assert!(payload["args"]["activity"].get("timestamps").is_none());
    }

    #[test]
    fn presence_payload_never_gains_path_separators_from_labels() {
        let payload = presence_payload(1, Some("main.rs"), Some("kuroya"), Some(1), "n3");
        let serialized = payload.to_string();

        assert!(!serialized.contains('/'));
        assert!(!serialized.contains('\\'));
    }

    #[test]
    fn clear_payload_sets_the_activity_to_null() {
        let payload = clear_activity_payload(4242, "n4");

        assert_eq!(payload["cmd"], "SET_ACTIVITY");
        assert_eq!(payload["args"]["pid"], 4242);
        assert_eq!(payload["args"]["activity"], Value::Null);
    }

    #[test]
    fn presence_text_is_clamped_to_the_discord_byte_limit_on_char_boundaries() {
        let ascii = "a".repeat(500);
        assert_eq!(
            clamp_presence_text(&ascii).len(),
            DISCORD_PRESENCE_TEXT_MAX_BYTES
        );

        let multibyte = "あ".repeat(200);
        let clamped = clamp_presence_text(&multibyte);
        assert!(clamped.len() <= DISCORD_PRESENCE_TEXT_MAX_BYTES);
        assert!(clamped.chars().all(|character| character == 'あ'));
    }

    #[test]
    fn presence_labels_use_only_the_file_and_workspace_names() {
        assert_eq!(
            presence_labels_for_frame(Some("main.rs"), Some("kuroya"), false, true, true),
            (
                Some("Editing main.rs".to_owned()),
                Some("In kuroya".to_owned())
            )
        );
        assert_eq!(
            presence_labels_for_frame(None, Some("kuroya"), false, true, true),
            (Some("Idle".to_owned()), Some("In kuroya".to_owned()))
        );
        assert_eq!(
            presence_labels_for_frame(None, None, true, true, true),
            (Some("Idle".to_owned()), Some("In Kuroya".to_owned()))
        );
    }

    #[test]
    fn presence_labels_omit_the_lines_the_user_hid() {
        assert_eq!(
            presence_labels_for_frame(Some("main.rs"), Some("kuroya"), false, false, true),
            (None, Some("In kuroya".to_owned()))
        );
        assert_eq!(
            presence_labels_for_frame(Some("main.rs"), Some("kuroya"), false, true, false),
            (Some("Editing main.rs".to_owned()), None)
        );
        assert_eq!(
            presence_labels_for_frame(Some("main.rs"), Some("kuroya"), false, false, false),
            (None, None)
        );
    }

    #[test]
    fn presence_payload_omits_hidden_fields_instead_of_sending_empty_strings() {
        let payload = presence_payload(4242, None, None, None, "n5");
        let activity = &payload["args"]["activity"];

        assert!(activity.get("details").is_none());
        assert!(activity.get("state").is_none());
        assert_eq!(activity["assets"]["large_image"], "kuroya");

        let payload = presence_payload(4242, Some("main.rs"), None, None, "n6");
        assert_eq!(payload["args"]["activity"]["details"], "main.rs");
        assert!(payload["args"]["activity"].get("state").is_none());
    }

    struct RecordedTransport {
        sent: Arc<Mutex<Vec<(u32, Value)>>>,

        fail_writes_from: Option<usize>,
        writes: usize,
        ready_sent: bool,
    }

    impl RecordedTransport {
        fn new(sent: &Arc<Mutex<Vec<(u32, Value)>>>, fail_writes_from: Option<usize>) -> Self {
            Self {
                sent: Arc::clone(sent),
                fail_writes_from,
                writes: 0,
                ready_sent: false,
            }
        }
    }

    impl super::DiscordPresenceTransport for RecordedTransport {
        fn send_frame(&mut self, opcode: u32, payload: &[u8]) -> io::Result<()> {
            if let Some(from) = self.fail_writes_from
                && self.writes >= from
            {
                return Err(io::Error::other("mock pipe closed"));
            }
            self.writes += 1;
            let value = serde_json::from_slice(payload).unwrap_or(Value::Null);
            self.sent.lock().unwrap().push((opcode, value));
            Ok(())
        }

        fn read_frame(&mut self) -> io::Result<DiscordFrame> {
            if self.ready_sent {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "no more mock frames",
                ));
            }
            self.ready_sent = true;
            Ok(DiscordFrame {
                opcode: OPCODE_FRAME,
                payload: br#"{"evt":"READY"}"#.to_vec(),
            })
        }
    }

    struct MockConnector {
        pending: Vec<RecordedTransport>,
        fail_first_connects: usize,

        gate: Option<Arc<AtomicBool>>,
        attempts: Arc<AtomicU64>,
    }

    impl MockConnector {
        fn with_transports(transports: Vec<RecordedTransport>) -> Self {
            Self {
                pending: transports,
                fail_first_connects: 0,
                gate: None,
                attempts: Arc::new(AtomicU64::new(0)),
            }
        }

        fn gated(transports: Vec<RecordedTransport>, gate: Arc<AtomicBool>) -> Self {
            Self {
                pending: transports,
                fail_first_connects: 0,
                gate: Some(gate),
                attempts: Arc::new(AtomicU64::new(0)),
            }
        }
    }

    impl super::DiscordPresenceConnector for MockConnector {
        type Transport = RecordedTransport;

        fn connect(&mut self) -> io::Result<RecordedTransport> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if let Some(gate) = &self.gate
                && !gate.load(Ordering::SeqCst)
            {
                return Err(io::Error::other("mock discord is gated off"));
            }
            if self.fail_first_connects > 0 {
                self.fail_first_connects -= 1;
                return Err(io::Error::other("mock discord is absent"));
            }
            let mut transport = self
                .pending
                .pop()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "mock discord is absent"))?;

            transport.send_frame(
                super::OPCODE_HANDSHAKE,
                handshake_payload("mock-client-id").as_bytes(),
            )?;
            let _ready = transport.read_frame()?;
            Ok(transport)
        }
    }

    const IDLE_TEST_TICK: Duration = Duration::from_secs(600);
    const RECONNECT_TEST_TICK: Duration = Duration::from_millis(20);

    fn stepped_clock() -> (Arc<AtomicU64>, Box<dyn Fn() -> u64 + Send>) {
        let counter = Arc::new(AtomicU64::new(1_000_000));
        let clock_counter = Arc::clone(&counter);
        let clock = Box::new(move || clock_counter.fetch_add(10, Ordering::SeqCst) + 10);
        (counter, clock)
    }

    fn spawn_loop_for_test(
        connector: MockConnector,
        tick: Duration,
        clock: Box<dyn Fn() -> u64 + Send>,
    ) -> (mpsc::Sender<DiscordPresenceCommand>, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _exit_signal = exit_tx;
            run_presence_loop(rx, connector, tick, clock);
        });
        (tx, exit_rx)
    }

    fn recorded(sent: &Arc<Mutex<Vec<(u32, Value)>>>) -> Vec<(u32, Value)> {
        sent.lock().unwrap().clone()
    }

    fn activities(sent: &Arc<Mutex<Vec<(u32, Value)>>>) -> Vec<Value> {
        recorded(sent)
            .into_iter()
            .filter(|(opcode, payload)| *opcode == OPCODE_FRAME && payload["cmd"] == "SET_ACTIVITY")
            .map(|(_, payload)| payload["args"]["activity"].clone())
            .collect()
    }

    fn update_command(details: &str, state: &str) -> DiscordPresenceCommand {
        DiscordPresenceCommand::Update {
            details: Some(details.to_owned()),
            state: Some(state.to_owned()),
            show_elapsed: true,
        }
    }

    fn wait_for_activity_count(sent: &Arc<Mutex<Vec<(u32, Value)>>>, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if activities(sent).len() >= expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!(
            "timed out waiting for {expected} activity frames; saw {:?}",
            activities(sent)
        );
    }

    fn wait_until(condition: impl Fn() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("timed out waiting for a mock condition");
    }

    fn wait_for_exit(exit_rx: &mpsc::Receiver<()>) -> bool {
        matches!(
            exit_rx.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected)
        )
    }

    #[test]
    fn update_while_disconnected_is_queued_until_reconnect_and_sends_only_the_newest() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let gate = Arc::new(AtomicBool::new(false));
        let connector =
            MockConnector::gated(vec![RecordedTransport::new(&sent, None)], gate.clone());
        let attempts = connector.attempts.clone();
        let (tx, exit_rx) = spawn_loop_for_test(connector, RECONNECT_TEST_TICK, stepped_clock().1);

        tx.send(update_command("first.rs", "kuroya")).unwrap();
        wait_until(|| attempts.load(Ordering::SeqCst) >= 2);
        tx.send(update_command("second.rs", "kuroya")).unwrap();
        wait_until(|| attempts.load(Ordering::SeqCst) >= 3);
        gate.store(true, Ordering::SeqCst);
        wait_for_activity_count(&sent, 1);
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();
        assert!(wait_for_exit(&exit_rx));

        let sent_activities = activities(&sent);
        assert_eq!(
            sent_activities.len(),
            2,
            "the reconnect update plus the shutdown clear"
        );
        assert_eq!(
            sent_activities[0]["details"], "second.rs",
            "only the newest queued update may reach Discord"
        );
    }

    #[test]
    fn clear_sends_a_null_activity_frame() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "kuroya")).unwrap();
        wait_for_activity_count(&sent, 1);
        tx.send(DiscordPresenceCommand::Clear).unwrap();
        wait_until(|| activities(&sent).last() == Some(&Value::Null));
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();
        assert!(wait_for_exit(&exit_rx));

        let sent_activities = activities(&sent);
        assert_eq!(
            sent_activities
                .iter()
                .filter(|activity| activity.is_null())
                .count(),
            1,
            "the clear is the only null activity; the shutdown must not re-clear"
        );
        assert_eq!(sent_activities[0]["details"], "main.rs");
    }

    #[test]
    fn state_change_reanchors_the_start_timestamp_but_a_repeat_does_not() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "workspace-a")).unwrap();
        wait_for_activity_count(&sent, 1);
        tx.send(update_command("lib.rs", "workspace-a")).unwrap();
        wait_for_activity_count(&sent, 2);
        tx.send(update_command("lib.rs", "workspace-b")).unwrap();
        wait_for_activity_count(&sent, 3);
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();
        assert!(wait_for_exit(&exit_rx));

        let first_seen = first_seen_starts(&activities(&sent));
        let same_workspace = first_seen[&("main.rs".to_owned(), "workspace-a".to_owned())];
        let same_workspace_again = first_seen[&("lib.rs".to_owned(), "workspace-a".to_owned())];
        let switched = first_seen[&("lib.rs".to_owned(), "workspace-b".to_owned())];
        assert_eq!(
            same_workspace_again, same_workspace,
            "a new file in the same workspace keeps the anchor"
        );
        assert!(
            switched > same_workspace,
            "a workspace switch re-anchors the elapsed timer"
        );
    }

    #[test]
    fn tick_timeout_resends_the_last_activity_to_keep_the_presence_alive() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, RECONNECT_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "kuroya")).unwrap();
        wait_for_activity_count(&sent, 1);

        wait_for_activity_count(&sent, 2);
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();
        assert!(wait_for_exit(&exit_rx));

        let sent_activities = activities(&sent);
        assert_eq!(
            sent_activities[0], sent_activities[1],
            "the keep-alive repeats the same activity unchanged"
        );
    }

    #[test]
    fn write_error_marks_the_connection_dead_and_recovers_on_the_next_connect() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let first = RecordedTransport::new(&sent, Some(2));
        let second = RecordedTransport::new(&sent, None);
        let connector = MockConnector::with_transports(vec![second, first]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "kuroya")).unwrap();
        wait_for_activity_count(&sent, 1);

        tx.send(update_command("other.rs", "kuroya")).unwrap();

        wait_until(|| recorded(&sent).len() >= 4);
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();
        assert!(wait_for_exit(&exit_rx));

        let frames = recorded(&sent);
        assert_eq!(frames[0].0, OPCODE_HANDSHAKE);
        assert_eq!(frames[1].1["args"]["activity"]["details"], "main.rs");
        assert_eq!(frames[2].0, OPCODE_HANDSHAKE, "the loop reconnected");
        assert_eq!(
            frames[3].1["args"]["activity"]["details"], "other.rs",
            "the reconnect announces the newest activity"
        );
        assert_eq!(
            activities(&sent).last(),
            Some(&Value::Null),
            "shutdown still clears the recovered connection"
        );
    }

    #[test]
    fn dropping_the_sender_clears_the_presence_and_exits_the_thread() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "kuroya")).unwrap();
        wait_for_activity_count(&sent, 1);
        drop(tx);

        assert!(wait_for_exit(&exit_rx));
        assert_eq!(activities(&sent).last(), Some(&Value::Null));
    }

    #[test]
    fn shutdown_command_clears_the_presence_and_exits_promptly() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(update_command("main.rs", "kuroya")).unwrap();
        wait_for_activity_count(&sent, 1);
        tx.send(DiscordPresenceCommand::Shutdown).unwrap();

        assert!(wait_for_exit(&exit_rx));
        assert_eq!(activities(&sent).last(), Some(&Value::Null));
    }

    #[test]
    fn shutdown_without_a_sent_activity_does_not_send_a_clear() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, exit_rx) = spawn_loop_for_test(connector, IDLE_TEST_TICK, stepped_clock().1);

        tx.send(DiscordPresenceCommand::Shutdown).unwrap();

        assert!(wait_for_exit(&exit_rx));
        assert!(activities(&sent).is_empty());

        let frames = recorded(&sent);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].0, OPCODE_HANDSHAKE);
    }

    #[test]
    fn clear_and_shutdown_waits_for_the_thread_within_the_limit() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (tx, rx) = mpsc::channel();
        let (exit_tx, exit_rx) = mpsc::channel();
        let joined = Arc::new(AtomicU64::new(0));
        let joined_flag = Arc::clone(&joined);
        let (_, clock) = stepped_clock();
        std::thread::spawn(move || {
            let _exit_signal = exit_tx;
            run_presence_loop(rx, connector, IDLE_TEST_TICK, clock);
            joined_flag.store(1, Ordering::SeqCst);
        });
        let mut thread = DiscordPresenceThread {
            tx: Some(tx),
            exit_rx,
            join: None,
        };

        thread.clear_and_shutdown(Duration::from_secs(5));

        assert_eq!(joined.load(Ordering::SeqCst), 1);
        assert!(DISCORD_PRESENCE_SHUTDOWN_LIMIT <= Duration::from_millis(500));
        assert_eq!(activities(&sent).last(), Some(&Value::Null));
    }

    #[test]
    fn sync_discord_presence_stays_inert_when_disabled_or_missing_client_id() {
        let root = discord_temp_root("inert");
        let mut app = app_for_discord_test(root.clone());

        app.sync_discord_presence_runtime();
        assert!(
            app.discord_presence.is_none(),
            "default settings must spawn nothing"
        );

        app.settings.discord.presence_enabled = true;
        app.sync_discord_presence_runtime();
        assert!(
            app.discord_presence.is_none(),
            "enabled without a client id must spawn nothing"
        );

        app.settings.discord.client_id = "1234567890123456789".to_owned();
        app.sync_discord_presence_runtime();
        assert!(app.discord_presence.is_some());
        app.shutdown_discord_presence();
        assert!(app.discord_presence.is_none());

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn frame_updates_send_only_on_change_and_shutdown_clears_the_presence() {
        let root = discord_temp_root("frames");
        let mut app = app_for_discord_test(root.clone());
        let (runtime, sent) = discord_runtime_for_test();
        app.discord_presence = Some(runtime);
        app.buffers.push(TextBuffer::from_text(
            1,
            Some(root.join("src/main.rs")),
            "fn main() {}".to_owned(),
        ));
        app.active = Some(1);

        app.update_discord_presence();
        app.update_discord_presence();
        app.buffer_mut(1).unwrap().set_path(root.join("src/lib.rs"));
        app.update_discord_presence();

        app.shutdown_discord_presence();
        assert!(app.discord_presence.is_none());

        let sent_activities = activities(&sent);
        let shown: Vec<&Value> = sent_activities
            .iter()
            .filter(|activity| !activity.is_null())
            .collect();
        assert_eq!(
            shown.len(),
            2,
            "one update per label change, none for the unchanged frame: {sent_activities:?}"
        );
        assert_eq!(shown[0]["details"], "Editing main.rs");
        assert_eq!(
            shown[0]["state"],
            format!("In {}", root.file_name().unwrap().to_string_lossy())
        );
        assert_eq!(shown[1]["details"], "Editing lib.rs");
        assert_eq!(sent_activities.last(), Some(&Value::Null));
    }

    type TestRuntime = (DiscordPresenceRuntime, Arc<Mutex<Vec<(u32, Value)>>>);

    fn discord_runtime_for_test() -> TestRuntime {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let connector = MockConnector::with_transports(vec![RecordedTransport::new(&sent, None)]);
        let (_, clock) = stepped_clock();
        let (_, thread) = spawn_discord_presence_loop(connector, IDLE_TEST_TICK, clock);
        (
            DiscordPresenceRuntime::new(thread, "test-client-id".to_owned()),
            sent,
        )
    }

    fn app_for_discord_test(root: PathBuf) -> KuroyaApp {
        let (tx, rx) = ui_event_channel();
        let settings = EditorSettings::default();
        KuroyaApp::from_startup_context(AppStartupContext {
            runtime: Runtime::new().expect("test runtime"),
            tx,
            rx,
            workspace: Workspace::new(root.clone()),
            settings: settings.clone(),
            settings_panel_draft: settings,
            settings_editor_font_path: String::new(),
            settings_ui_font_path: String::new(),
            theme_picker_selected: 0,
            saved_session: None,
            terminal: TerminalPane::new(root.clone(), 100, 12.0, 1.2),
            watcher: None,
            recent_projects: Vec::new(),
            trusted_workspaces: vec![root.clone()],
            now: Instant::now(),
            startup_timings: Vec::new(),
        })
    }

    fn discord_temp_root(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "kuroya-discord-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn first_seen_starts(activities: &[Value]) -> std::collections::HashMap<(String, String), u64> {
        let mut first_seen = std::collections::HashMap::new();
        for activity in activities {
            let key = (
                activity["details"].as_str().unwrap_or_default().to_owned(),
                activity["state"].as_str().unwrap_or_default().to_owned(),
            );
            let start = activity["timestamps"]["start"].as_u64().unwrap_or_default();
            first_seen.entry(key).or_insert(start);
        }
        first_seen
    }
}
