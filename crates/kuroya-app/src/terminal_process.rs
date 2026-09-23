use crate::path_display::{display_error_label_cow, sanitized_display_label_cow};
#[cfg(test)]
use crossbeam_channel::TrySendError;
use crossbeam_channel::{Receiver, Sender, unbounded};
use egui::Context;
use portable_pty::{ExitStatus, PtySize, native_pty_system};
use std::{
    borrow::Cow,
    collections::BTreeMap,
    fmt::Display,
    fmt::Write as _,
    io::{Read, Write},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub(crate) use shell::TerminalShellProfile;
pub(crate) use shell::default_shell_label;
pub(crate) use shell::detected_shell_profiles;
pub(crate) use shell::terminal_shell_label;
use shell::{configured_process, configured_shell};

mod shell;

const TERMINAL_EVENT_OUTPUT_CHUNK_BYTES: usize = 4096;
const TERMINAL_PROCESS_INPUT_MAX_BYTES: usize = 1024 * 1024;
const TERMINAL_FAILURE_LABEL_MAX_CHARS: usize = 48;

pub(crate) enum TerminalCommand {
    Input(String),
    Resize(PtySize),
    Close,
}

/// Command stream for the writer thread only: input bytes and the close
/// request. Resizes are deliberately absent - see
/// [`run_terminal_command_demux`] for why they take a separate path.
enum TerminalWriterCommand {
    Input(String),
    Close,
}

pub(crate) enum TerminalLaunch {
    Shell {
        shell_path: Option<String>,
        shell_args: Vec<String>,
    },
    Process {
        program: String,
        args: Vec<String>,
        env: BTreeMap<String, String>,
    },
}

pub(crate) enum TerminalEvent {
    Output(Vec<u8>),
    Finished {
        message: Option<String>,
        process_exit_code: Option<i32>,
        reason: TerminalFinishReason,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TerminalFinishReason {
    ProcessExit,
    TerminalError,
}

#[cfg(test)]
pub(crate) fn send_terminal_event(tx: &Sender<TerminalEvent>, event: TerminalEvent) -> bool {
    send_terminal_event_with_repaint(tx, event, None)
}

#[cfg(test)]
pub(crate) fn send_terminal_event_with_repaint(
    tx: &Sender<TerminalEvent>,
    event: TerminalEvent,
    repaint_context: Option<&Context>,
) -> bool {
    if let TerminalEvent::Output(output) = event {
        return send_terminal_output_chunks_with_repaint(tx, output, repaint_context);
    }
    send_terminal_event_single_with_repaint(tx, event, repaint_context)
}

#[cfg(test)]
fn send_terminal_event_single_with_repaint(
    tx: &Sender<TerminalEvent>,
    event: TerminalEvent,
    repaint_context: Option<&Context>,
) -> bool {
    match tx.try_send(event) {
        Ok(()) => {
            request_terminal_repaint(repaint_context);
            true
        }
        Err(TrySendError::Full(_)) => {
            request_terminal_repaint(repaint_context);
            false
        }
        Err(TrySendError::Disconnected(_)) => false,
    }
}

#[cfg(test)]
fn send_terminal_output_chunks_with_repaint(
    tx: &Sender<TerminalEvent>,
    output: Vec<u8>,
    repaint_context: Option<&Context>,
) -> bool {
    if output.len() <= TERMINAL_EVENT_OUTPUT_CHUNK_BYTES {
        return send_terminal_event_single_with_repaint(
            tx,
            TerminalEvent::Output(output),
            repaint_context,
        );
    }

    request_terminal_repaint(repaint_context);
    for chunk in output.chunks(TERMINAL_EVENT_OUTPUT_CHUNK_BYTES) {
        if !send_terminal_event_single(tx, TerminalEvent::Output(chunk.to_vec())) {
            return false;
        }
    }
    request_terminal_repaint(repaint_context);
    true
}

#[cfg(test)]
fn send_terminal_event_single(tx: &Sender<TerminalEvent>, event: TerminalEvent) -> bool {
    match tx.try_send(event) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) => false,
        Err(TrySendError::Disconnected(_)) => false,
    }
}

pub(crate) fn send_terminal_event_blocking_with_repaint(
    tx: &Sender<TerminalEvent>,
    event: TerminalEvent,
    repaint_context: Option<&Context>,
) -> bool {
    if let TerminalEvent::Output(output) = event {
        return send_terminal_output_chunks_blocking_with_repaint(tx, output, repaint_context);
    }
    send_terminal_event_single_blocking_with_repaint(tx, event, repaint_context)
}

fn send_terminal_event_single_blocking_with_repaint(
    tx: &Sender<TerminalEvent>,
    event: TerminalEvent,
    repaint_context: Option<&Context>,
) -> bool {
    request_terminal_repaint(repaint_context);
    match tx.send(event) {
        Ok(()) => {
            request_terminal_repaint(repaint_context);
            true
        }
        Err(_) => false,
    }
}

fn send_terminal_output_chunks_blocking_with_repaint(
    tx: &Sender<TerminalEvent>,
    output: Vec<u8>,
    repaint_context: Option<&Context>,
) -> bool {
    if output.len() <= TERMINAL_EVENT_OUTPUT_CHUNK_BYTES {
        return send_terminal_event_single_blocking_with_repaint(
            tx,
            TerminalEvent::Output(output),
            repaint_context,
        );
    }

    request_terminal_repaint(repaint_context);
    for chunk in output.chunks(TERMINAL_EVENT_OUTPUT_CHUNK_BYTES) {
        if !send_terminal_event_single_blocking(tx, TerminalEvent::Output(chunk.to_vec())) {
            return false;
        }
    }
    request_terminal_repaint(repaint_context);
    true
}

fn send_terminal_event_single_blocking(tx: &Sender<TerminalEvent>, event: TerminalEvent) -> bool {
    tx.send(event).is_ok()
}

fn request_terminal_repaint(repaint_context: Option<&Context>) {
    if let Some(ctx) = repaint_context {
        ctx.request_repaint();
    }
}

/// Windows Job Object wrapper that owns the spawned terminal process tree.
///
/// portable-pty's Windows killer only terminates the direct shell child, so
/// grandchildren (e.g. the node server started by `npm run dev`) would
/// outlive the tab and keep ports and handles open. The job is created with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and the shell is assigned immediately
/// after spawn, so closing the stored handle - by dropping this guard, or by
/// this process exiting for any reason (including `std::process::exit`) -
/// tears down the entire tree. Descendants spawned by the shell inherit job
/// membership automatically.
#[cfg(windows)]
mod process_job {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// Kill-on-close job guard kept alive for as long as the PTY session runs.
    pub(crate) struct TerminalProcessJob {
        handle: HANDLE,
    }

    impl TerminalProcessJob {
        /// Creates the job and assigns the freshly spawned child to it.
        ///
        /// Returns `None` when a job API fails; the terminal then keeps the
        /// previous direct-child-only kill behavior as a graceful fallback.
        pub(crate) fn attach(child_pid: Option<u32>) -> Option<Self> {
            let child_pid = child_pid?;
            // SAFETY: no security attributes and no name are required.
            let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if handle.is_null() {
                return None;
            }
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: `handle` is a valid job and the buffer matches the info class.
            let configured = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION
                        as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                // SAFETY: `handle` is a valid job that is no longer needed.
                unsafe { CloseHandle(handle) };
                return None;
            }
            // portable-pty does not expose the child handle, so reopen the
            // process by pid with the rights AssignProcessToJobObject requires.
            // SAFETY: probing an OS-supplied pid; the handle is closed below.
            let child_handle =
                unsafe { OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, child_pid) };
            if child_handle.is_null() {
                // SAFETY: `handle` is a valid job that is no longer needed.
                unsafe { CloseHandle(handle) };
                return None;
            }
            // SAFETY: both handles are valid and open with sufficient rights.
            let assigned = unsafe { AssignProcessToJobObject(handle, child_handle) };
            // SAFETY: `child_handle` is a valid process handle.
            unsafe { CloseHandle(child_handle) };
            if assigned == 0 {
                // SAFETY: `handle` is a valid job that is no longer needed.
                unsafe { CloseHandle(handle) };
                return None;
            }
            Some(Self { handle })
        }

        /// Backstop that kills every process still assigned to the job.
        pub(crate) fn terminate(&self) {
            // SAFETY: `handle` is a valid job for the guard's lifetime.
            unsafe { TerminateJobObject(self.handle, 1) };
        }
    }

    impl Drop for TerminalProcessJob {
        fn drop(&mut self) {
            // Closing the last job handle fires JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            // which also covers crash and std::process::exit teardown paths.
            // SAFETY: `handle` is a valid job owned exclusively by this guard.
            unsafe { CloseHandle(self.handle) };
        }
    }

    /// Reports whether `pid` still maps to a live process.
    #[cfg(test)]
    pub(crate) fn process_is_alive(pid: u32) -> bool {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::{
            PROCESS_QUERY_LIMITED_INFORMATION, WaitForSingleObject,
        };

        // Waiting on the handle requires SYNCHRONIZE on top of query access.
        const SYNCHRONIZE: u32 = 0x0010_0000;

        // SAFETY: probing an OS-supplied pid; the handle is closed below.
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE, 0, pid) };
        if handle.is_null() {
            return false;
        }
        // SAFETY: `handle` is a valid process handle just opened above.
        let state = unsafe { WaitForSingleObject(handle, 0) };
        // SAFETY: `handle` is a valid process handle.
        unsafe { CloseHandle(handle) };
        state != WAIT_OBJECT_0
    }
}

pub(crate) fn run_pty(
    cwd: PathBuf,
    initial_size: PtySize,
    launch: TerminalLaunch,
    show_exit_alert: bool,
    rx_command: Receiver<TerminalCommand>,
    rx_close: Receiver<()>,
    close_requested: Arc<AtomicBool>,
    tx_output: Sender<TerminalEvent>,
    repaint_context: Option<Context>,
) -> anyhow::Result<()> {
    let pty_system = native_pty_system();
    let pair = pty_system.openpty(initial_size)?;

    let is_process_launch = matches!(launch, TerminalLaunch::Process { .. });
    let mut cmd = match launch {
        TerminalLaunch::Shell {
            shell_path,
            shell_args,
        } => configured_shell(shell_path.as_deref(), &shell_args),
        TerminalLaunch::Process { program, args, env } => configured_process(&program, &args, &env),
    };
    cmd.cwd(cwd);
    let mut child = pair.slave.spawn_command(cmd)?;
    #[cfg(windows)]
    let terminal_job = process_job::TerminalProcessJob::attach(child.process_id());
    let mut child_killer = child.clone_killer();
    let mut close_killer = child.clone_killer();
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader()?;
    let mut writer = pair.master.take_writer()?;
    let master = pair.master;

    let terminal_finished = Arc::new(AtomicBool::new(false));
    let reader_tx = tx_output.clone();
    let reader_close_requested = Arc::clone(&close_requested);
    let reader_terminal_finished = Arc::clone(&terminal_finished);
    let reader_repaint_context = repaint_context.clone();
    let reader_handle = thread::spawn(move || {
        let mut buf = [0_u8; TERMINAL_EVENT_OUTPUT_CHUNK_BYTES];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if !send_terminal_output_if_unfinished(
                        &reader_tx,
                        buf[..n].to_vec(),
                        &reader_terminal_finished,
                        reader_repaint_context.as_ref(),
                    ) {
                        break;
                    }
                }
                Err(error) => {
                    send_terminal_read_error_output(
                        &reader_tx,
                        error,
                        &reader_close_requested,
                        &reader_terminal_finished,
                        reader_repaint_context.as_ref(),
                    );
                    break;
                }
            }
        }
    });

    let close_signal_requested = Arc::clone(&close_requested);
    thread::spawn(move || {
        if rx_close.recv().is_ok() {
            close_signal_requested.store(true, Ordering::SeqCst);
            let _ = close_killer.kill();
        }
    });

    let (tx_writer_command, rx_writer_command) = unbounded();
    let (tx_resize, rx_resize) = unbounded();
    let demux_close_requested = Arc::clone(&close_requested);
    let demux_terminal_finished = Arc::clone(&terminal_finished);
    thread::spawn(move || {
        run_terminal_command_demux(
            rx_command,
            tx_writer_command,
            tx_resize,
            &demux_close_requested,
            &demux_terminal_finished,
        );
    });

    let writer_close_requested = Arc::clone(&close_requested);
    let writer_terminal_finished = Arc::clone(&terminal_finished);
    thread::spawn(move || {
        while let Ok(command) = rx_writer_command.recv() {
            if terminal_command_writer_should_stop(
                &writer_close_requested,
                &writer_terminal_finished,
            ) {
                break;
            }
            match command {
                TerminalWriterCommand::Input(input) => {
                    let input_result = write_terminal_input_if_running(
                        &mut writer,
                        &input,
                        &writer_close_requested,
                        &writer_terminal_finished,
                    );
                    match input_result {
                        Ok(true) => {}
                        Ok(false) | Err(_) => break,
                    }
                }
                TerminalWriterCommand::Close => {
                    writer_close_requested.store(true, Ordering::SeqCst);
                    let _ = child_killer.kill();
                    break;
                }
            }
        }
        if !writer_close_requested.load(Ordering::SeqCst) {
            writer_close_requested.store(true, Ordering::SeqCst);
            let _ = child_killer.kill();
        }
    });

    // Owns the pty master so resizes apply even while the writer thread is
    // wedged in a blocking input write (see run_terminal_command_demux). The
    // channel disconnecting wakes this loop, so the master is dropped when
    // the demux ends instead of lingering behind a blocked write.
    let resize_close_requested = Arc::clone(&close_requested);
    let resize_terminal_finished = Arc::clone(&terminal_finished);
    thread::spawn(move || {
        while let Ok(size) = rx_resize.recv() {
            if terminal_command_writer_should_stop(
                &resize_close_requested,
                &resize_terminal_finished,
            ) {
                break;
            }
            let _ = master.resize(size);
        }
    });

    let status = child.wait();
    #[cfg(windows)]
    if let Some(terminal_job) = &terminal_job {
        // The graceful ConPTY/killer path only ever reaches the direct child.
        // Terminate the job so anything the shell left behind (e.g. `npm run
        // dev` grandchildren still holding the console) dies now instead of
        // wedging the reader below or outliving the closed session.
        terminal_job.terminate();
    }
    let _ = reader_handle.join();
    let status = status?;
    let close_requested = close_requested.load(Ordering::SeqCst);
    let _ = send_terminal_finished_once(
        &tx_output,
        TerminalEvent::Finished {
            message: terminal_exit_alert_message(&status, show_exit_alert, close_requested),
            process_exit_code: terminal_process_exit_code(
                &status,
                is_process_launch,
                close_requested,
            ),
            reason: TerminalFinishReason::ProcessExit,
        },
        &terminal_finished,
        repaint_context.as_ref(),
    );
    Ok(())
}

fn send_terminal_output_if_unfinished(
    tx: &Sender<TerminalEvent>,
    output: Vec<u8>,
    terminal_finished: &AtomicBool,
    repaint_context: Option<&Context>,
) -> bool {
    if terminal_finished.load(Ordering::SeqCst) {
        return false;
    }
    send_terminal_event_blocking_with_repaint(tx, TerminalEvent::Output(output), repaint_context)
}

/// Reports a pty reader failure as plain output instead of a finished event.
///
/// Closing ConPTY can race into a read error, so a read failure must not claim
/// the once-only finished signal: the error text is streamed as an `Output`
/// event and the normal `Finished` event from `child.wait()` below still
/// carries the real exit code and terminal state.
fn send_terminal_read_error_output(
    tx: &Sender<TerminalEvent>,
    error: impl Display,
    close_requested: &AtomicBool,
    terminal_finished: &AtomicBool,
    repaint_context: Option<&Context>,
) -> bool {
    if close_requested.load(Ordering::SeqCst) {
        return false;
    }
    send_terminal_output_if_unfinished(
        tx,
        terminal_failure_message("terminal read error", error).into_bytes(),
        terminal_finished,
        repaint_context,
    )
}

fn send_terminal_finished_once(
    tx: &Sender<TerminalEvent>,
    event: TerminalEvent,
    terminal_finished: &AtomicBool,
    repaint_context: Option<&Context>,
) -> bool {
    if terminal_finished.swap(true, Ordering::SeqCst) {
        return false;
    }
    send_terminal_event_blocking_with_repaint(tx, event, repaint_context)
}

fn terminal_command_writer_should_stop(
    close_requested: &AtomicBool,
    terminal_finished: &AtomicBool,
) -> bool {
    close_requested.load(Ordering::SeqCst) || terminal_finished.load(Ordering::SeqCst)
}

/// Demultiplexes terminal commands so resizes never queue behind input writes.
///
/// The writer thread can block indefinitely inside `write_all` on the ConPTY
/// input pipe when the child never drains stdin (e.g. a large paste into a
/// program that reads nothing). If resizes shared the writer queue, every
/// resize would stall behind the blocked write and the pty size would diverge
/// from the parser grid permanently. This loop therefore forwards input and
/// close to the writer channel and resizes to a dedicated channel whose
/// receiver owns the pty master; both channels are unbounded, so this demux
/// never blocks on a wedged writer. Dropping `tx_resize` (when this loop ends)
/// also disconnects the resize loop, so the master cannot linger behind a
/// blocked input write during teardown.
fn run_terminal_command_demux(
    rx_command: Receiver<TerminalCommand>,
    tx_writer: Sender<TerminalWriterCommand>,
    tx_resize: Sender<PtySize>,
    close_requested: &AtomicBool,
    terminal_finished: &AtomicBool,
) {
    while let Ok(command) = rx_command.recv() {
        if terminal_command_writer_should_stop(close_requested, terminal_finished) {
            break;
        }
        match command {
            TerminalCommand::Input(input) => {
                if tx_writer.send(TerminalWriterCommand::Input(input)).is_err() {
                    break;
                }
            }
            TerminalCommand::Resize(size) => {
                if tx_resize.send(size).is_err() {
                    break;
                }
            }
            TerminalCommand::Close => {
                let _ = tx_writer.send(TerminalWriterCommand::Close);
                break;
            }
        }
    }
}

fn write_terminal_input_if_running(
    writer: &mut impl Write,
    input: &str,
    close_requested: &AtomicBool,
    terminal_finished: &AtomicBool,
) -> std::io::Result<bool> {
    if terminal_command_writer_should_stop(close_requested, terminal_finished) {
        return Ok(false);
    }
    write_terminal_input(writer, input)?;
    Ok(true)
}

fn write_terminal_input(writer: &mut impl Write, input: &str) -> std::io::Result<()> {
    let input = bounded_terminal_process_input(input, TERMINAL_PROCESS_INPUT_MAX_BYTES);
    if input.is_empty() {
        return Ok(());
    }
    writer.write_all(input.as_bytes())?;
    writer.flush()
}

fn bounded_terminal_process_input(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }

    let mut truncate_at = max_bytes;
    while truncate_at > 0 && !input.is_char_boundary(truncate_at) {
        truncate_at -= 1;
    }
    &input[..truncate_at]
}

fn terminal_process_exit_code(
    status: &ExitStatus,
    is_process_launch: bool,
    close_requested: bool,
) -> Option<i32> {
    if !is_process_launch || close_requested {
        return None;
    }
    // Windows reports unsigned codes (0xFFFFFFFF for a -1 exit), so the cast
    // must wrap instead of saturating to keep the signed value meaningful.
    Some(status.exit_code() as i32)
}

pub(crate) fn terminal_exit_alert_message(
    status: &ExitStatus,
    show_exit_alert: bool,
    close_requested: bool,
) -> Option<String> {
    (show_exit_alert && !close_requested && !status.success())
        .then(|| terminal_failure_message("process exited", status))
}

fn terminal_failure_label_cow(label: &str) -> Cow<'_, str> {
    sanitized_display_label_cow(label, TERMINAL_FAILURE_LABEL_MAX_CHARS, "terminal error")
}

pub(crate) fn terminal_failure_message(label: &str, detail: impl Display) -> String {
    let label = terminal_failure_label_cow(label);
    let detail = detail.to_string();
    let detail = display_error_label_cow(&detail);
    let mut message = String::with_capacity(label.len() + detail.len() + 6);
    let _ = write!(&mut message, "\r\n{label}: {detail}\r\n");
    message
}

#[cfg(test)]
fn terminal_event_output_chunk_bytes_for_test() -> usize {
    TERMINAL_EVENT_OUTPUT_CHUNK_BYTES
}

#[cfg(test)]
mod tests {
    use super::{
        TerminalCommand, TerminalEvent, TerminalFinishReason, TerminalWriterCommand,
        run_terminal_command_demux, send_terminal_event, send_terminal_event_blocking_with_repaint,
        send_terminal_event_with_repaint, send_terminal_read_error_output,
        terminal_event_output_chunk_bytes_for_test, terminal_exit_alert_message,
        terminal_failure_label_cow, terminal_failure_message, terminal_process_exit_code,
        write_terminal_input, write_terminal_input_if_running,
    };
    use crate::path_display::DISPLAY_ERROR_LABEL_MAX_CHARS;
    use crossbeam_channel::{RecvTimeoutError, TryRecvError, bounded, unbounded};
    use egui::Context;
    use portable_pty::{ExitStatus, PtySize};
    use std::{
        borrow::Cow,
        io::{self, Write},
        sync::atomic::{AtomicBool, Ordering},
        thread,
    };

    #[test]
    fn terminal_exit_alert_message_only_shows_non_zero_when_enabled() {
        assert_eq!(
            terminal_exit_alert_message(&ExitStatus::with_exit_code(0), true, false),
            None
        );
        assert_eq!(
            terminal_exit_alert_message(&ExitStatus::with_exit_code(1), false, false),
            None
        );
        assert_eq!(
            terminal_exit_alert_message(&ExitStatus::with_exit_code(1), true, false),
            Some("\r\nprocess exited: Exited with code 1\r\n".to_owned())
        );
    }

    #[test]
    fn terminal_exit_alert_message_suppresses_intentional_close() {
        assert_eq!(
            terminal_exit_alert_message(&ExitStatus::with_exit_code(1), true, true),
            None
        );
    }

    #[test]
    fn terminal_input_write_reports_flush_failures() {
        let mut writer = FlushFailWriter::default();

        let error =
            write_terminal_input(&mut writer, "queued input").expect_err("flush should fail");

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(writer.bytes, b"queued input");
        assert_eq!(writer.flushes, 1);
    }

    #[test]
    fn terminal_input_write_preserves_raw_bytes() {
        let mut writer = FlushFailWriter::default();
        let input = "\0\x1b[31mraw\r\n\u{7}";

        let error = write_terminal_input(&mut writer, input).expect_err("flush should fail");

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(writer.bytes, input.as_bytes());
        assert_eq!(writer.flushes, 1);
    }

    #[test]
    fn terminal_input_write_caps_oversized_input_at_utf8_boundary() {
        let mut writer = FlushFailWriter::default();
        let input = format!(
            "{}\u{e9}",
            "a".repeat(super::TERMINAL_PROCESS_INPUT_MAX_BYTES)
        );

        let error = write_terminal_input(&mut writer, &input).expect_err("flush should fail");

        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(writer.bytes.len(), super::TERMINAL_PROCESS_INPUT_MAX_BYTES);
        assert!(std::str::from_utf8(&writer.bytes).is_ok());
        assert!(writer.bytes.ends_with(b"a"));
        assert_eq!(writer.flushes, 1);
    }

    #[test]
    fn terminal_input_write_skips_finished_processes() {
        let mut writer = FlushFailWriter::default();
        let close_requested = AtomicBool::new(false);
        let terminal_finished = AtomicBool::new(true);

        let wrote = write_terminal_input_if_running(
            &mut writer,
            "late input",
            &close_requested,
            &terminal_finished,
        )
        .expect("finished process should skip input without writer errors");

        assert!(!wrote);
        assert!(writer.bytes.is_empty());
        assert_eq!(writer.flushes, 0);
    }

    #[test]
    fn terminal_process_exit_code_tracks_process_launches_only() {
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(0), true, false),
            Some(0)
        );
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(17), true, false),
            Some(17)
        );
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(17), true, true),
            None
        );
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(17), false, false),
            None
        );
    }

    #[test]
    fn terminal_process_exit_code_wraps_windows_unsigned_codes_to_signed() {
        // Windows tools exit with 0xFFFFFFFF (u32::MAX) to report -1; the
        // wrapping cast must surface -1 instead of saturating at i32::MAX.
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(u32::MAX), true, false),
            Some(-1)
        );
        assert_eq!(
            terminal_process_exit_code(&ExitStatus::with_exit_code(0x8000_0000), true, false),
            Some(i32::MIN)
        );
    }

    fn terminal_test_pty_size(rows: u16, cols: u16) -> PtySize {
        PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        }
    }

    #[test]
    fn terminal_command_demux_delivers_resize_while_input_queue_is_blocked() {
        let (tx_command, rx_command) = unbounded();
        let (tx_writer, rx_writer) = unbounded();
        let (tx_resize, rx_resize) = unbounded();
        let close_requested = AtomicBool::new(false);
        let terminal_finished = AtomicBool::new(false);

        // Queue a paste-sized input first, exactly like the wedged-writer
        // scenario: with the old single-queue writer thread, resizes behind
        // this input would never run while write_all was blocked.
        tx_command
            .send(TerminalCommand::Input(
                "x".repeat(super::TERMINAL_PROCESS_INPUT_MAX_BYTES),
            ))
            .expect("command queue should accept input");
        tx_command
            .send(TerminalCommand::Resize(terminal_test_pty_size(30, 100)))
            .expect("command queue should accept resize");
        tx_command
            .send(TerminalCommand::Resize(terminal_test_pty_size(40, 120)))
            .expect("command queue should accept resize");

        let demux = thread::spawn(move || {
            run_terminal_command_demux(
                rx_command,
                tx_writer,
                tx_resize,
                &close_requested,
                &terminal_finished,
            );
        });

        let first = rx_resize
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("first resize must bypass the unconsumed input queue");
        assert_eq!((first.rows, first.cols), (30, 100));
        let second = rx_resize
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("second resize must also bypass the input queue");
        assert_eq!((second.rows, second.cols), (40, 120));

        // The input stays untouched at the head of the writer queue in order,
        // and resizes never enter the writer stream.
        match rx_writer.try_recv() {
            Ok(TerminalWriterCommand::Input(input)) => {
                assert_eq!(input.len(), super::TERMINAL_PROCESS_INPUT_MAX_BYTES);
            }
            Ok(TerminalWriterCommand::Close) => panic!("close should not be queued"),
            Err(TryRecvError::Empty) => panic!("input should be queued for the writer"),
            Err(TryRecvError::Disconnected) => panic!("writer queue disconnected"),
        }
        assert!(matches!(rx_writer.try_recv(), Err(TryRecvError::Empty)));

        // Dropping the command sender ends the demux, which disconnects the
        // resize channel exactly like session teardown does.
        drop(tx_command);
        demux
            .join()
            .expect("demux thread should stop when commands disconnect");
        assert!(matches!(
            rx_resize.recv_timeout(std::time::Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn terminal_command_demux_forwards_close_and_disconnects_resize_channel() {
        let (tx_command, rx_command) = unbounded();
        let (tx_writer, rx_writer) = unbounded();
        let (tx_resize, rx_resize) = unbounded();
        let close_requested = AtomicBool::new(false);
        let terminal_finished = AtomicBool::new(false);

        tx_command
            .send(TerminalCommand::Close)
            .expect("command queue should accept close");
        let demux = thread::spawn(move || {
            run_terminal_command_demux(
                rx_command,
                tx_writer,
                tx_resize,
                &close_requested,
                &terminal_finished,
            );
        });

        demux
            .join()
            .expect("demux thread should stop after forwarding close");
        match rx_writer.try_recv() {
            Ok(TerminalWriterCommand::Close) => {}
            Ok(TerminalWriterCommand::Input(_)) => panic!("expected close command"),
            Err(TryRecvError::Empty) => panic!("close should be queued for the writer"),
            Err(TryRecvError::Disconnected) => panic!("writer queue disconnected"),
        }
        // The resize channel drops with the demux so the pty master is not
        // held behind the close request.
        assert!(matches!(
            rx_resize.recv_timeout(std::time::Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        ));
    }

    #[test]
    fn terminal_failure_label_cow_borrows_clean_ascii_and_unicode() {
        assert!(matches!(
            terminal_failure_label_cow("terminal read error"),
            Cow::Borrowed("terminal read error")
        ));

        let unicode = "terminal \u{03bb} error";
        match terminal_failure_label_cow(unicode) {
            Cow::Borrowed(label) => assert_eq!(label, unicode),
            Cow::Owned(label) => panic!("expected borrowed label, got {label:?}"),
        }
    }

    #[test]
    fn terminal_failure_label_cow_owns_dirty_truncated_and_fallback_labels() {
        let dirty = terminal_failure_label_cow("terminal\nread");
        assert_eq!(dirty.as_ref(), "terminal read");
        assert!(matches!(dirty, Cow::Owned(_)));

        let long = format!("{}tail", "a".repeat(80));
        let truncated = terminal_failure_label_cow(&long);
        assert!(truncated.contains("..."));
        assert!(truncated.chars().count() <= super::TERMINAL_FAILURE_LABEL_MAX_CHARS);
        assert!(matches!(truncated, Cow::Owned(_)));

        let fallback = terminal_failure_label_cow("   ");
        assert_eq!(fallback.as_ref(), "terminal error");
        assert!(matches!(fallback, Cow::Owned(_)));
    }

    #[test]
    fn terminal_failure_message_preserves_formatting_bounds_and_detail_sanitization() {
        assert_eq!(
            terminal_failure_message("process exited", ExitStatus::with_exit_code(17)),
            "\r\nprocess exited: Exited with code 17\r\n"
        );

        let message = terminal_failure_message(
            &format!("read error {}", "x".repeat(96)),
            "first line\r\nsecond line \u{202e}detail",
        );
        let body = message
            .strip_prefix("\r\n")
            .expect("terminal message should start with CRLF")
            .strip_suffix("\r\n")
            .expect("terminal message should end with CRLF");
        let (label, detail) = body
            .split_once(": ")
            .expect("terminal message should separate label and detail");

        assert_eq!(message.matches("\r\n").count(), 2);
        assert!(label.contains("..."));
        assert!(label.chars().count() <= super::TERMINAL_FAILURE_LABEL_MAX_CHARS);
        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\r'));
        assert!(!detail.contains('\u{202e}'));
        assert!(detail.contains("first line second line detail"));
        assert!(detail.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    #[test]
    fn terminal_failure_message_sanitizes_and_bounds_error_details() {
        let message = terminal_failure_message(
            "terminal error",
            format!("first line\nsecond line \u{202e}{}", "x".repeat(512)),
        );
        let detail = terminal_message_detail(&message, "terminal error");

        assert_eq!(message.matches("\r\n").count(), 2);
        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\r'));
        assert!(!detail.contains('\u{202e}'));
        assert!(detail.contains("first line second line"));
        assert!(detail.contains("..."));
        assert!(detail.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    #[test]
    fn terminal_exit_alert_message_sanitizes_and_bounds_signal_details() {
        let status = ExitStatus::with_signal(&format!(
            "signal line\nnext line \u{202e}{}",
            "x".repeat(512)
        ));
        let message = terminal_exit_alert_message(&status, true, false)
            .expect("nonzero signal exit should show alert");
        let detail = terminal_message_detail(&message, "process exited");

        assert!(!detail.contains('\n'));
        assert!(!detail.contains('\r'));
        assert!(!detail.contains('\u{202e}'));
        assert!(detail.contains("Terminated by signal line next line"));
        assert!(detail.contains("..."));
        assert!(detail.chars().count() <= DISPLAY_ERROR_LABEL_MAX_CHARS);
    }

    fn terminal_message_detail<'a>(message: &'a str, label: &str) -> &'a str {
        let prefix = format!("\r\n{label}: ");
        message
            .strip_prefix(&prefix)
            .expect("terminal message should start with label")
            .strip_suffix("\r\n")
            .expect("terminal message should end with CRLF")
    }

    #[derive(Default)]
    struct FlushFailWriter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl Write for FlushFailWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes = self.flushes.saturating_add(1);
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "flush failed"))
        }
    }

    #[test]
    fn send_terminal_event_returns_false_when_output_queue_is_full() {
        let (tx, _rx) = bounded(1);

        assert!(send_terminal_event(
            &tx,
            TerminalEvent::Output(b"first".to_vec())
        ));
        assert!(!send_terminal_event(
            &tx,
            TerminalEvent::Output(b"overflow".to_vec())
        ));
    }

    #[test]
    fn send_terminal_event_enqueues_when_capacity_is_available() {
        let (tx, rx) = bounded(1);

        assert!(send_terminal_event(
            &tx,
            TerminalEvent::Finished {
                message: Some("done".to_owned()),
                process_exit_code: Some(0),
                reason: TerminalFinishReason::ProcessExit,
            }
        ));

        match rx.try_recv() {
            Ok(TerminalEvent::Finished {
                message: Some(message),
                process_exit_code,
                reason,
            }) => {
                assert_eq!(message, "done");
                assert_eq!(process_exit_code, Some(0));
                assert_eq!(reason, TerminalFinishReason::ProcessExit);
            }
            Ok(TerminalEvent::Finished { message: None, .. }) => {
                panic!("expected finished message")
            }
            Ok(TerminalEvent::Output(_)) => panic!("expected finished event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal event"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
    }

    #[test]
    fn send_terminal_event_splits_oversized_output_chunks() {
        let chunk_size = terminal_event_output_chunk_bytes_for_test();
        let (tx, rx) = bounded(2);

        assert!(send_terminal_event(
            &tx,
            TerminalEvent::Output(vec![b'x'; chunk_size + 3])
        ));

        match rx.try_recv() {
            Ok(TerminalEvent::Output(output)) => assert_eq!(output.len(), chunk_size),
            Ok(TerminalEvent::Finished { .. }) => panic!("expected output event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal output"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        match rx.try_recv() {
            Ok(TerminalEvent::Output(output)) => assert_eq!(output.len(), 3),
            Ok(TerminalEvent::Finished { .. }) => panic!("expected output event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal output"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
    }

    #[test]
    fn send_terminal_event_blocking_waits_for_capacity_and_enqueues() {
        let (tx, rx) = bounded(1);
        assert!(send_terminal_event(
            &tx,
            TerminalEvent::Output(b"first".to_vec())
        ));

        let blocking_tx = tx.clone();
        let handle = thread::spawn(move || {
            send_terminal_event_blocking_with_repaint(
                &blocking_tx,
                TerminalEvent::Finished {
                    message: Some("done".to_owned()),
                    process_exit_code: Some(17),
                    reason: TerminalFinishReason::ProcessExit,
                },
                None,
            )
        });

        match rx.recv() {
            Ok(TerminalEvent::Output(output)) => assert_eq!(output, b"first"),
            Ok(TerminalEvent::Finished { .. }) => panic!("expected first output event"),
            Err(_) => panic!("terminal event queue disconnected"),
        }
        assert!(
            handle
                .join()
                .expect("blocking enqueue thread should finish")
        );
        match rx.try_recv() {
            Ok(TerminalEvent::Finished {
                message: Some(message),
                process_exit_code,
                reason,
            }) => {
                assert_eq!(message, "done");
                assert_eq!(process_exit_code, Some(17));
                assert_eq!(reason, TerminalFinishReason::ProcessExit);
            }
            Ok(TerminalEvent::Finished { message: None, .. }) => {
                panic!("expected finished message")
            }
            Ok(TerminalEvent::Output(_)) => panic!("expected finished event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal event"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
    }

    #[test]
    fn send_terminal_event_with_repaint_requests_repaint_when_enqueued() {
        let (tx, _rx) = bounded(1);
        let ctx = Context::default();

        assert!(send_terminal_event_with_repaint(
            &tx,
            TerminalEvent::Output(b"output".to_vec()),
            Some(&ctx)
        ));

        assert!(ctx.has_requested_repaint());
    }

    #[test]
    fn send_terminal_event_with_repaint_requests_repaint_when_queue_is_full() {
        let (tx, _rx) = bounded(1);
        assert!(send_terminal_event(
            &tx,
            TerminalEvent::Output(b"first".to_vec())
        ));
        let ctx = Context::default();

        assert!(!send_terminal_event_with_repaint(
            &tx,
            TerminalEvent::Output(b"overflow".to_vec()),
            Some(&ctx)
        ));

        assert!(ctx.has_requested_repaint());
    }

    #[test]
    fn send_terminal_event_returns_false_when_output_queue_is_disconnected() {
        let (tx, rx) = bounded(1);
        drop(rx);

        assert!(!send_terminal_event(
            &tx,
            TerminalEvent::Output(b"output".to_vec())
        ));
        assert!(!send_terminal_event_blocking_with_repaint(
            &tx,
            TerminalEvent::Finished {
                message: None,
                process_exit_code: None,
                reason: TerminalFinishReason::ProcessExit,
            },
            None,
        ));
    }

    #[test]
    fn terminal_read_error_is_reported_as_output_and_exit_code_still_finishes() {
        let (tx, rx) = bounded(4);
        let close_requested = AtomicBool::new(false);
        let terminal_finished = AtomicBool::new(false);

        assert!(send_terminal_read_error_output(
            &tx,
            io::Error::new(io::ErrorKind::BrokenPipe, "conpty closed"),
            &close_requested,
            &terminal_finished,
            None,
        ));
        // The read failure must not claim the once-only finished signal.
        assert!(!terminal_finished.load(Ordering::SeqCst));

        assert!(super::send_terminal_finished_once(
            &tx,
            TerminalEvent::Finished {
                message: None,
                process_exit_code: Some(7),
                reason: TerminalFinishReason::ProcessExit,
            },
            &terminal_finished,
            None,
        ));

        match rx.try_recv() {
            Ok(TerminalEvent::Output(output)) => {
                let text = String::from_utf8_lossy(&output);
                assert!(text.contains("terminal read error"));
                assert!(text.contains("conpty closed"));
            }
            Ok(TerminalEvent::Finished { .. }) => panic!("expected read-error output event"),
            Err(TryRecvError::Empty) => panic!("expected queued read-error output"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        match rx.try_recv() {
            Ok(TerminalEvent::Finished {
                process_exit_code,
                reason,
                ..
            }) => {
                assert_eq!(process_exit_code, Some(7));
                assert_eq!(reason, TerminalFinishReason::ProcessExit);
            }
            Ok(TerminalEvent::Output(_)) => panic!("expected finished event"),
            Err(TryRecvError::Empty) => panic!("expected queued finished event"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn terminal_read_error_output_is_suppressed_after_close_request() {
        let (tx, rx) = bounded(2);
        let close_requested = AtomicBool::new(true);
        let terminal_finished = AtomicBool::new(false);

        assert!(!send_terminal_read_error_output(
            &tx,
            io::Error::new(io::ErrorKind::BrokenPipe, "conpty closed"),
            &close_requested,
            &terminal_finished,
            None,
        ));
        assert!(!terminal_finished.load(Ordering::SeqCst));
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn terminal_finished_guard_suppresses_duplicate_finished_events() {
        let (tx, rx) = bounded(2);
        let terminal_finished = AtomicBool::new(false);

        assert!(super::send_terminal_finished_once(
            &tx,
            TerminalEvent::Finished {
                message: Some("first".to_owned()),
                process_exit_code: Some(0),
                reason: TerminalFinishReason::ProcessExit,
            },
            &terminal_finished,
            None,
        ));
        assert!(!super::send_terminal_finished_once(
            &tx,
            TerminalEvent::Finished {
                message: Some("second".to_owned()),
                process_exit_code: Some(1),
                reason: TerminalFinishReason::TerminalError,
            },
            &terminal_finished,
            None,
        ));

        match rx.try_recv() {
            Ok(TerminalEvent::Finished {
                message,
                process_exit_code,
                reason,
            }) => {
                assert_eq!(message.as_deref(), Some("first"));
                assert_eq!(process_exit_code, Some(0));
                assert_eq!(reason, TerminalFinishReason::ProcessExit);
            }
            Ok(TerminalEvent::Output(_)) => panic!("expected finished event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal event"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn terminal_finished_guard_suppresses_output_after_finished() {
        let (tx, rx) = bounded(3);
        let terminal_finished = AtomicBool::new(false);

        assert!(super::send_terminal_output_if_unfinished(
            &tx,
            b"before".to_vec(),
            &terminal_finished,
            None,
        ));
        assert!(super::send_terminal_finished_once(
            &tx,
            TerminalEvent::Finished {
                message: None,
                process_exit_code: Some(0),
                reason: TerminalFinishReason::ProcessExit,
            },
            &terminal_finished,
            None,
        ));
        assert!(!super::send_terminal_output_if_unfinished(
            &tx,
            b"after".to_vec(),
            &terminal_finished,
            None,
        ));

        match rx.try_recv() {
            Ok(TerminalEvent::Output(output)) => assert_eq!(output, b"before"),
            Ok(TerminalEvent::Finished { .. }) => panic!("expected output event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal output"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        match rx.try_recv() {
            Ok(TerminalEvent::Finished {
                process_exit_code, ..
            }) => {
                assert_eq!(process_exit_code, Some(0));
            }
            Ok(TerminalEvent::Output(_)) => panic!("expected output event"),
            Err(TryRecvError::Empty) => panic!("expected queued terminal output"),
            Err(TryRecvError::Disconnected) => panic!("terminal event queue disconnected"),
        }
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[cfg(windows)]
    static WINDOWS_PROCESS_TREE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Serializes the process-tree tests so their `waitfor.exe` grandchildren
    /// cannot be confused with each other by the `tasklist` scan.
    #[cfg(windows)]
    fn windows_process_tree_test_lock() -> std::sync::MutexGuard<'static, ()> {
        WINDOWS_PROCESS_TREE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[cfg(windows)]
    fn poll_windows_process_until(
        timeout: std::time::Duration,
        mut probe: impl FnMut() -> bool,
    ) -> bool {
        let started = std::time::Instant::now();
        loop {
            if probe() {
                return true;
            }
            if started.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    #[cfg(windows)]
    fn poll_for_waitfor_grandchild_pid(timeout: std::time::Duration) -> Option<u32> {
        let started = std::time::Instant::now();
        loop {
            if let Some(pid) = find_waitfor_grandchild_pid() {
                return Some(pid);
            }
            if started.elapsed() >= timeout {
                return None;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// Locates the `waitfor.exe` grandchild started by the test command line.
    #[cfg(windows)]
    fn find_waitfor_grandchild_pid() -> Option<u32> {
        let output = std::process::Command::new("tasklist.exe")
            .args(["/FI", "IMAGENAME eq waitfor.exe", "/FO", "CSV", "/NH"])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let mut fields = line.split("\",\"");
            let image = fields.next().unwrap_or_default().trim_start_matches('"');
            let pid = fields.next().and_then(|pid| pid.parse::<u32>().ok());
            if image.eq_ignore_ascii_case("waitfor.exe") {
                return pid;
            }
        }
        None
    }

    #[cfg(windows)]
    #[test]
    fn terminal_job_object_kills_pty_grandchild_when_session_is_killed() {
        use super::process_job::{TerminalProcessJob, process_is_alive};
        use portable_pty::{CommandBuilder, PtySize, native_pty_system};

        let _test_lock = windows_process_tree_test_lock();

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("openpty should succeed");
        let mut command = CommandBuilder::new("cmd.exe");
        command.args(["/C", "start /b waitfor kuroyaterminaljob /t 30"]);
        command.cwd(std::env::temp_dir());
        let mut child = pair
            .slave
            .spawn_command(command)
            .expect("cmd should spawn in the pty");
        let shell_pid = child.process_id().expect("spawned cmd should expose a pid");
        drop(pair.slave);

        let job = TerminalProcessJob::attach(child.process_id())
            .expect("job object should attach to the spawned shell");
        // Some sandboxed environments cannot initialize ConPTY children at all
        // (the shell dies with STATUS_DLL_INIT_FAILED). Without a grandchild
        // there is no orphan scenario to assert, so skip instead of failing;
        // the std::process-based job tests still cover the tree kill there.
        let Some(grandchild_pid) =
            poll_for_waitfor_grandchild_pid(std::time::Duration::from_secs(15))
        else {
            let _ = child.clone_killer().kill();
            let _ = child.wait();
            eprintln!("skipping: pty shell did not spawn a waitfor grandchild in this environment");
            return;
        };
        assert_ne!(grandchild_pid, shell_pid);

        // Same path the tab-close thread takes: kill the direct child only.
        let mut killer = child.clone_killer();
        let _ = killer.kill();
        let _ = child.wait();

        // Same backstop run_pty performs once the shell exits.
        job.terminate();

        assert!(
            poll_windows_process_until(std::time::Duration::from_secs(10), || {
                !process_is_alive(grandchild_pid)
            }),
            "waitfor grandchild should be killed with the session"
        );
        assert!(
            poll_windows_process_until(std::time::Duration::from_secs(10), || {
                !process_is_alive(shell_pid)
            }),
            "shell pid should be gone after the session is killed"
        );
    }

    #[cfg(windows)]
    #[test]
    fn terminal_job_object_kills_assigned_sleeper_when_job_handle_closes() {
        use super::process_job::{TerminalProcessJob, process_is_alive};
        use std::os::windows::process::CommandExt;

        let _test_lock = windows_process_tree_test_lock();

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = std::process::Command::new("cmd.exe");
        command.args(["/C", "waitfor kuroyaterminaljob2 /t 30"]);
        command.creation_flags(CREATE_NO_WINDOW);
        let mut sleeper = command.spawn().expect("cmd sleeper should spawn");
        let sleeper_pid = sleeper.id();

        let job = TerminalProcessJob::attach(Some(sleeper_pid))
            .expect("job object should attach to the sleeper");
        assert!(
            process_is_alive(sleeper_pid),
            "sleeper should be alive while the job handle is open"
        );

        // No explicit terminate: dropping the guard closes the last job handle
        // and JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE must finish the tree. This is
        // the path that also covers crash and std::process::exit teardown.
        drop(job);

        assert!(
            poll_windows_process_until(std::time::Duration::from_secs(10), || {
                !process_is_alive(sleeper_pid)
            }),
            "sleeper should die when the job handle closes"
        );
        let _ = sleeper.wait();
    }

    #[cfg(windows)]
    #[test]
    fn terminal_job_object_terminates_grandchild_tree_on_job_terminate() {
        use super::process_job::{TerminalProcessJob, process_is_alive};
        use std::os::windows::process::CommandExt;

        let _test_lock = windows_process_tree_test_lock();

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = std::process::Command::new("cmd.exe");
        command.args(["/C", "start /b waitfor kuroyaterminaljob3 /t 30"]);
        command.creation_flags(CREATE_NO_WINDOW);
        let mut shell = command.spawn().expect("cmd shell should spawn");
        let shell_pid = shell.id();

        let job = TerminalProcessJob::attach(Some(shell_pid))
            .expect("job object should attach to the shell");
        assert!(
            process_is_alive(shell_pid),
            "shell should be alive while the job handle is open"
        );
        // `start /b` leaves a grandchild behind once the direct cmd child
        // exits; descendants inherit job membership, so the job still owns it.
        let grandchild_pid = poll_for_waitfor_grandchild_pid(std::time::Duration::from_secs(15))
            .expect("waitfor grandchild should appear");
        assert_ne!(grandchild_pid, shell_pid);

        // Same backstop run_pty performs once the direct child is gone.
        job.terminate();

        assert!(
            poll_windows_process_until(std::time::Duration::from_secs(10), || {
                !process_is_alive(grandchild_pid)
            }),
            "waitfor grandchild should be killed by the job terminate"
        );
        assert!(
            poll_windows_process_until(std::time::Duration::from_secs(10), || {
                !process_is_alive(shell_pid)
            }),
            "shell should be killed by the job terminate"
        );
        let _ = shell.wait();
    }
}
