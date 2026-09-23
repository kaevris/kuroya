use std::{io, process::ExitStatus, time::Duration};
use tokio::{process::Child, sync::mpsc, time::timeout};

pub(crate) const LSP_SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

pub(super) const LSP_CHILD_DEATH_REAP_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LspChildControl {
    ExitGracefully,
}

pub(super) struct LspChildWatchdog {
    control_tx: mpsc::Sender<LspChildControl>,
    pub(super) death_rx: mpsc::Receiver<io::Result<ExitStatus>>,
}

impl LspChildWatchdog {
    pub(super) async fn request_graceful_exit(&mut self) -> Option<io::Result<ExitStatus>> {
        if self
            .control_tx
            .send(LspChildControl::ExitGracefully)
            .await
            .is_err()
        {
            return None;
        }
        timeout(
            LSP_SHUTDOWN_GRACE + LSP_CHILD_DEATH_REAP_TIMEOUT,
            self.death_rx.recv(),
        )
        .await
        .ok()
        .flatten()
    }
}

pub(super) fn spawn_lsp_child_watchdog(child: Child) -> LspChildWatchdog {
    spawn_lsp_child_watchdog_with_grace(child, LSP_SHUTDOWN_GRACE)
}

fn spawn_lsp_child_watchdog_with_grace(child: Child, grace: Duration) -> LspChildWatchdog {
    let (control_tx, control_rx) = mpsc::channel(1);
    let (death_tx, death_rx) = mpsc::channel(1);
    tokio::spawn(run_lsp_child_watchdog(child, control_rx, death_tx, grace));
    LspChildWatchdog {
        control_tx,
        death_rx,
    }
}

async fn run_lsp_child_watchdog(
    mut child: Child,
    mut control_rx: mpsc::Receiver<LspChildControl>,
    death_tx: mpsc::Sender<io::Result<ExitStatus>>,
    grace: Duration,
) {
    tokio::select! {
        biased;
        control = control_rx.recv() => {
            match control {
                Some(LspChildControl::ExitGracefully) => {
                    let status = match timeout(grace, child.wait()).await {
                        Ok(status) => status,
                        Err(_) => {
                            let _ = child.kill().await;
                            child.wait().await
                        }
                    };
                    let _ = death_tx.send(status).await;
                }
                None => {

                    let _ = child.kill().await;
                }
            }
        }
        status = child.wait() => {
            let _ = death_tx.send(status).await;
        }
    }
}

pub(super) fn lsp_exit_status_label(status: &ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit code {code}"),
        None => status.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{LSP_SHUTDOWN_GRACE, lsp_exit_status_label, spawn_lsp_child_watchdog_with_grace};
    use std::{process::Stdio, time::Duration};
    use tokio::process::Command;

    fn long_lived_child() -> Command {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "ping -n 30 127.0.0.1 > NUL"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "sleep 30"]);
            command
        };

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        command.kill_on_drop(true);
        command
    }

    fn exiting_child(exit_code: u8) -> Command {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", &format!("exit {exit_code}")]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", &format!("exit {exit_code}")]);
            command
        };

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::null());
        command.kill_on_drop(true);
        command
    }

    #[test]
    fn shutdown_grace_is_five_seconds() {
        assert_eq!(LSP_SHUTDOWN_GRACE, Duration::from_secs(5));
    }

    #[tokio::test]
    async fn watchdog_reports_natural_child_exit_status() {
        let mut command = exiting_child(3);
        let child = command.spawn().expect("spawn exiting child");
        let mut watchdog = spawn_lsp_child_watchdog_with_grace(child, LSP_SHUTDOWN_GRACE);

        let status = watchdog
            .death_rx
            .recv()
            .await
            .expect("watchdog should report child death")
            .expect("child exit should be observed");

        assert_eq!(status.code(), Some(3));
        assert!(watchdog.death_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn watchdog_kills_child_after_grace_period_elapses() {
        let mut command = long_lived_child();
        let child = command.spawn().expect("spawn long-lived child");
        let mut watchdog = spawn_lsp_child_watchdog_with_grace(child, Duration::from_millis(50));

        let status = watchdog.request_graceful_exit().await;

        assert!(
            status.is_some_and(|status| status.is_ok()),
            "graceful exit should reap the killed child"
        );
        assert!(
            watchdog.death_rx.recv().await.is_none(),
            "watchdog should report death exactly once"
        );
    }

    #[test]
    fn exit_status_labels_report_exit_codes() {
        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(7)
        };

        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            std::process::ExitStatus::from_raw(7 << 8)
        };

        assert_eq!(lsp_exit_status_label(&status), "exit code 7");
    }
}
