use std::io::ErrorKind;
use std::path::Path;
use std::process::ExitStatus;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;

use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::process::Child;
use tokio::process::ChildStderr;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::Span;

use super::CommandShell;
use super::ConfiguredHandler;
use super::dispatcher::hook_event_name_label;
use super::dispatcher::hook_execution_mode_label;
use super::dispatcher::hook_handler_type_label;
use super::dispatcher::hook_scope_label;
use super::dispatcher::hook_source_label;
use super::dispatcher::scope_for_event;
use codex_protocol::protocol::HookExecutionMode;
use codex_protocol::protocol::HookHandlerType;

#[derive(Debug)]
pub(crate) struct CommandRunResult {
    pub started_at: i64,
    pub completed_at: i64,
    pub duration_ms: i64,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

#[tracing::instrument(
    name = "codex.hooks.command",
    level = "trace",
    skip_all,
    fields(
        hook.event_name = hook_event_name_label(handler.event_name),
        hook.handler_type = hook_handler_type_label(HookHandlerType::Command),
        hook.execution_mode = hook_execution_mode_label(HookExecutionMode::Sync),
        hook.scope = hook_scope_label(scope_for_event(handler.event_name)),
        hook.source = hook_source_label(handler.source),
        hook.display_order = handler.display_order,
        hook.configured_order = configured_order,
        hook.timeout_sec = handler.timeout_sec,
        hook.command_outcome = tracing::field::Empty,
    )
)]
pub(crate) async fn run_command(
    shell: &CommandShell,
    handler: &ConfiguredHandler,
    configured_order: usize,
    input_json: &str,
    cwd: &Path,
) -> CommandRunResult {
    let started_at = chrono::Utc::now().timestamp();
    let started = Instant::now();

    if handler.interactive {
        return run_interactive_command(shell, handler, input_json, cwd, started_at, started).await;
    }

    let mut command = build_command(shell, handler);
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return finish_command_run(
                started_at,
                started,
                CommandRunCompletion {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(err.to_string()),
                    outcome: "spawn_error",
                },
            );
        }
    };

    if let Some(mut stdin) = child.stdin.take()
        && let Err(err) = stdin.write_all(input_json.as_bytes()).await
        && err.kind() != ErrorKind::BrokenPipe
    {
        let _ = child.kill().await;
        return finish_command_run(
            started_at,
            started,
            CommandRunCompletion {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(format!("failed to write hook stdin: {err}")),
                outcome: "stdin_error",
            },
        );
    }

    let timeout_duration = Duration::from_secs(handler.timeout_sec);
    match timeout(timeout_duration, child.wait_with_output()).await {
        Ok(Ok(output)) => finish_command_run(
            started_at,
            started,
            CommandRunCompletion {
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                error: None,
                outcome: "completed",
            },
        ),
        Ok(Err(err)) => finish_command_run(
            started_at,
            started,
            CommandRunCompletion {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(err.to_string()),
                outcome: "wait_error",
            },
        ),
        Err(_) => finish_command_run(
            started_at,
            started,
            CommandRunCompletion {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                error: Some(format!("hook timed out after {}s", handler.timeout_sec)),
                outcome: "timeout",
            },
        ),
    }
}

async fn run_interactive_command(
    shell: &CommandShell,
    handler: &ConfiguredHandler,
    input_json: &str,
    cwd: &Path,
    started_at: i64,
    started: Instant,
) -> CommandRunResult {
    let terminal_lease = match crate::interactive_terminal::acquire().await {
        Ok(lease) => lease,
        Err(error) => {
            return finish_command_run(
                started_at,
                started,
                CommandRunCompletion {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(error.to_string()),
                    outcome: "terminal_lease_error",
                },
            );
        }
    };

    let mut command = build_command(shell, handler);
    command
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("CODEX_HOOK_INTERACTIVE", "1")
        .kill_on_drop(true);
    #[cfg(unix)]
    command.env("CODEX_HOOK_TTY_PATH", "/dev/tty");

    // Keep the child in Codex's foreground process group. Moving it to a new group without a
    // matching tcsetpgrp handoff would make `/dev/tty` reads stop with SIGTTIN. Interactive hook
    // commands must therefore `exec` their terminal program and must not leave background TTY
    // users behind.
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return finish_command_run(
                started_at,
                started,
                CommandRunCompletion {
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    error: Some(error.to_string()),
                    outcome: "spawn_error",
                },
            );
        }
    };

    let deadline = tokio::time::Instant::now() + Duration::from_secs(handler.timeout_sec);
    let (caller_alive_tx, caller_gone_rx) = oneshot::channel();
    let caller_alive = CallerAlive(Some(caller_alive_tx));
    // The detached supervisor owns both Child and lease before this function reaches another
    // await. If the hook future is cancelled, CallerAlive closes and the supervisor kills, waits,
    // drains, and only then releases the terminal back to Codex.
    let supervisor = tokio::spawn(supervise_interactive_child(
        child,
        terminal_lease,
        input_json.to_owned(),
        deadline,
        handler.timeout_sec,
        caller_gone_rx,
    ));

    let completion = match supervisor.await {
        Ok(completion) => completion,
        Err(error) => CommandRunCompletion {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: Some(format!("interactive hook supervisor failed: {error}")),
            outcome: "supervisor_error",
        },
    };
    caller_alive.finish();
    finish_command_run(started_at, started, completion)
}

struct CallerAlive(Option<oneshot::Sender<()>>);

impl CallerAlive {
    fn finish(mut self) {
        // The supervisor has already returned, so closing its liveness channel is now inert.
        drop(self.0.take());
    }
}

enum InteractiveWaitOutcome {
    Exited(std::io::Result<ExitStatus>),
    TimedOut,
    Cancelled,
}

async fn supervise_interactive_child(
    mut child: Child,
    _terminal_lease: crate::interactive_terminal::InteractiveTerminalLease,
    input_json: String,
    deadline: tokio::time::Instant,
    timeout_sec: u64,
    mut caller_gone: oneshot::Receiver<()>,
) -> CommandRunCompletion {
    let mut output_readers = OutputReaders::new(child.stdout.take(), child.stderr.take());
    let write_input = write_hook_input(child.stdin.take(), input_json);
    tokio::pin!(write_input);
    let input_result = tokio::select! {
        result = &mut write_input => Some(result),
        _ = tokio::time::sleep_until(deadline) => None,
        _ = &mut caller_gone => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            return CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error("interactive hook was cancelled", capture_error)),
                outcome: "cancelled",
            };
        }
    };

    match input_result {
        Some(Ok(())) => {}
        Some(Err(error)) => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            return CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error(
                    &format!("failed to write hook stdin: {error}"),
                    capture_error,
                )),
                outcome: "stdin_error",
            };
        }
        None => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            return CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error(
                    &format!("hook timed out after {timeout_sec}s"),
                    capture_error,
                )),
                outcome: "timeout",
            };
        }
    }

    let wait_outcome = tokio::select! {
        result = child.wait() => InteractiveWaitOutcome::Exited(result),
        _ = tokio::time::sleep_until(deadline) => InteractiveWaitOutcome::TimedOut,
        _ = &mut caller_gone => InteractiveWaitOutcome::Cancelled,
    };
    match wait_outcome {
        InteractiveWaitOutcome::Exited(Ok(status)) => {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let (stdout, stderr, capture_error) = output_readers.drain_for(remaining).await;
            CommandRunCompletion {
                exit_code: status.code(),
                stdout,
                stderr,
                error: capture_error,
                outcome: "completed",
            }
        }
        InteractiveWaitOutcome::Exited(Err(error)) => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error(&error.to_string(), capture_error)),
                outcome: "wait_error",
            }
        }
        InteractiveWaitOutcome::TimedOut => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error(
                    &format!("hook timed out after {timeout_sec}s"),
                    capture_error,
                )),
                outcome: "timeout",
            }
        }
        InteractiveWaitOutcome::Cancelled => {
            terminate_and_reap(&mut child).await;
            let (stdout, stderr, capture_error) = output_readers.drain().await;
            CommandRunCompletion {
                exit_code: None,
                stdout,
                stderr,
                error: Some(append_error(
                    "interactive hook was cancelled",
                    capture_error,
                )),
                outcome: "cancelled",
            }
        }
    }
}

async fn write_hook_input(
    stdin: Option<tokio::process::ChildStdin>,
    input_json: String,
) -> std::io::Result<()> {
    let Some(mut stdin) = stdin else {
        return Err(std::io::Error::other("hook stdin pipe was not available"));
    };
    match stdin.write_all(input_json.as_bytes()).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error),
    }
}

async fn terminate_and_reap(child: &mut Child) {
    if let Err(error) = child.start_kill()
        && error.kind() != ErrorKind::InvalidInput
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to start killing interactive hook");
    }
    if let Err(error) = child.wait().await
        && error.kind() != ErrorKind::InvalidInput
        && error.kind() != ErrorKind::NotFound
    {
        tracing::warn!(%error, "failed to reap interactive hook");
    }
}

const INTERACTIVE_OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

struct OutputReaders {
    stdout: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
    stderr: Option<JoinHandle<std::io::Result<Vec<u8>>>>,
}

impl OutputReaders {
    fn new(stdout: Option<ChildStdout>, stderr: Option<ChildStderr>) -> Self {
        Self {
            stdout: stdout.map(spawn_output_reader),
            stderr: stderr.map(spawn_output_reader),
        }
    }

    async fn drain(&mut self) -> (String, String, Option<String>) {
        self.drain_for(INTERACTIVE_OUTPUT_DRAIN_TIMEOUT).await
    }

    async fn drain_for(&mut self, duration: Duration) -> (String, String, Option<String>) {
        if duration.is_zero() {
            return (
                String::new(),
                String::new(),
                Some("timed out while draining interactive hook output".to_string()),
            );
        }
        match timeout(duration, self.collect()).await {
            Ok((stdout, stderr)) => {
                let (stdout, stdout_error) = decode_reader_result("stdout", stdout);
                let (stderr, stderr_error) = decode_reader_result("stderr", stderr);
                (
                    stdout,
                    stderr,
                    combine_optional_errors(stdout_error, stderr_error),
                )
            }
            Err(_) => (
                String::new(),
                String::new(),
                Some("timed out while draining interactive hook output".to_string()),
            ),
        }
    }

    async fn collect(
        &mut self,
    ) -> (
        Option<Result<std::io::Result<Vec<u8>>, tokio::task::JoinError>>,
        Option<Result<std::io::Result<Vec<u8>>, tokio::task::JoinError>>,
    ) {
        let stdout = async {
            match self.stdout.as_mut() {
                Some(reader) => Some(reader.await),
                None => None,
            }
        };
        let stderr = async {
            match self.stderr.as_mut() {
                Some(reader) => Some(reader.await),
                None => None,
            }
        };
        tokio::join!(stdout, stderr)
    }
}

impl Drop for OutputReaders {
    fn drop(&mut self) {
        if let Some(stdout) = &self.stdout {
            stdout.abort();
        }
        if let Some(stderr) = &self.stderr {
            stderr.abort();
        }
    }
}

fn spawn_output_reader<R>(mut reader: R) -> JoinHandle<std::io::Result<Vec<u8>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut output = Vec::new();
        reader.read_to_end(&mut output).await?;
        Ok(output)
    })
}

fn decode_reader_result(
    stream: &str,
    result: Option<Result<std::io::Result<Vec<u8>>, tokio::task::JoinError>>,
) -> (String, Option<String>) {
    match result {
        Some(Ok(Ok(output))) => (String::from_utf8_lossy(&output).to_string(), None),
        Some(Ok(Err(error))) => (
            String::new(),
            Some(format!("failed to read interactive hook {stream}: {error}")),
        ),
        Some(Err(error)) => (
            String::new(),
            Some(format!("interactive hook {stream} reader failed: {error}")),
        ),
        None => (
            String::new(),
            Some(format!("interactive hook {stream} pipe was not available")),
        ),
    }
}

fn combine_optional_errors(first: Option<String>, second: Option<String>) -> Option<String> {
    match (first, second) {
        (Some(first), Some(second)) => Some(format!("{first}; {second}")),
        (Some(error), None) | (None, Some(error)) => Some(error),
        (None, None) => None,
    }
}

fn append_error(primary: &str, secondary: Option<String>) -> String {
    match secondary {
        Some(secondary) => format!("{primary}; {secondary}"),
        None => primary.to_string(),
    }
}

struct CommandRunCompletion {
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    error: Option<String>,
    outcome: &'static str,
}

fn finish_command_run(
    started_at: i64,
    started: Instant,
    completion: CommandRunCompletion,
) -> CommandRunResult {
    Span::current().record("hook.command_outcome", completion.outcome);
    CommandRunResult {
        started_at,
        completed_at: chrono::Utc::now().timestamp(),
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(i64::MAX),
        exit_code: completion.exit_code,
        stdout: completion.stdout,
        stderr: completion.stderr,
        error: completion.error,
    }
}

fn build_command(shell: &CommandShell, handler: &ConfiguredHandler) -> Command {
    let mut command = if shell.program.is_empty() {
        default_shell_command()
    } else {
        Command::new(&shell.program)
    };
    if shell.program.is_empty() {
        #[cfg(windows)]
        command.raw_arg(format!(r#""{}""#, handler.command));

        #[cfg(not(windows))]
        command.arg(&handler.command);
    } else {
        command.args(&shell.args);

        #[cfg(windows)]
        if shell.args.iter().any(|arg| arg.eq_ignore_ascii_case("/c")) {
            command.raw_arg(format!(r#""{}""#, handler.command));
        } else {
            command.arg(&handler.command);
        }

        #[cfg(not(windows))]
        command.arg(&handler.command);
    }
    command.envs(&handler.env);
    command
}

fn default_shell_command() -> Command {
    #[cfg(windows)]
    {
        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
        let mut command = Command::new(comspec);
        command.arg("/C");
        command
    }

    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut command = Command::new(shell);
        command.arg("-lc");
        command
    }
}

#[cfg(test)]
#[path = "command_runner_tests.rs"]
mod tests;
