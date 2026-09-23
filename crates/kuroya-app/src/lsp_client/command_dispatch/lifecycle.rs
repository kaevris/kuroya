use super::super::wire::write_message;
use kuroya_core::LspWireMessage;
use tokio::process::ChildStdin;

/// Writes the LSP `shutdown` request and `exit` notification. The child
/// watchdog grants the server a bounded grace period to exit on its own and
/// kills it afterwards, so no immediate kill happens here.
pub(super) async fn handle_shutdown_messages(writer: &mut ChildStdin) {
    let _ = write_message(writer, &LspWireMessage::shutdown(2).to_json()).await;
    let _ = write_message(writer, &LspWireMessage::exit().to_json()).await;
}

#[cfg(test)]
mod tests {
    use super::handle_shutdown_messages;
    use std::process::Stdio;
    use tokio::process::{Child, ChildStdin, Command};

    async fn stdin_echo_child() -> (Child, ChildStdin) {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "more"]);
            command
        };

        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "cat"]);
            command
        };

        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn stdin echo child process");
        let stdin = child.stdin.take().expect("child stdin is piped");
        (child, stdin)
    }

    #[tokio::test]
    async fn shutdown_messages_write_shutdown_request_then_exit_notification() {
        let (child, mut writer) = stdin_echo_child().await;

        handle_shutdown_messages(&mut writer).await;
        drop(writer);

        let output = child
            .wait_with_output()
            .await
            .expect("stdin echo child exits cleanly");
        let output = String::from_utf8(output.stdout).expect("stdout is utf8");

        assert!(output.contains("\"method\":\"shutdown\""));
        assert!(output.contains("\"id\":2"));
        assert!(output.contains("\"method\":\"exit\""));
        assert!(
            output
                .find("\"method\":\"shutdown\"")
                .expect("shutdown first")
                < output.find("\"method\":\"exit\"").expect("exit second")
        );
    }
}
