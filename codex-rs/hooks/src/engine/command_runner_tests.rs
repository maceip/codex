use std::collections::HashMap;
#[cfg(any(unix, windows))]
use std::fs;
#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Command as StdCommand;
#[cfg(unix)]
use std::time::Duration;

use codex_protocol::protocol::HookEventName;
use codex_protocol::protocol::HookSource;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

#[cfg(unix)]
use super::CommandRunResult;
use super::CommandShell;
use super::ConfiguredHandler;
use super::run_command;

#[cfg(windows)]
#[tokio::test]
async fn cmd_shell_runs_quoted_hook_command_path() {
    let temp = tempdir().expect("create temp dir");
    let hook_dir = temp.path().join("hook with spaces");
    fs::create_dir(&hook_dir).expect("create hook dir");
    let hook_path = hook_dir.join("hook.cmd");
    fs::write(
        &hook_path,
        "@echo off\r\nif not \"%~1\"==\"notify\" exit /B 7\r\necho hook-ran\r\n",
    )
    .expect("write hook command");
    let source_path =
        AbsolutePathBuf::try_from(hook_path.clone()).expect("absolute hook command path");
    let handler = ConfiguredHandler {
        event_name: HookEventName::SessionStart,
        matcher: None,
        command: format!(r#""{}" notify"#, hook_path.display()),
        timeout_sec: 10,
        interactive: false,
        status_message: None,
        additional_context_limit: Default::default(),
        source_path,
        source: HookSource::User,
        display_order: 0,
        env: HashMap::new(),
    };
    let shells = [
        CommandShell {
            program: String::new(),
            args: Vec::new(),
        },
        CommandShell {
            program: std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string()),
            args: vec!["/c".to_string()],
        },
    ];

    for shell in shells {
        let result = run_command(
            &shell,
            &handler,
            /*configured_order*/ 0,
            "{}",
            temp.path(),
        )
        .await;

        assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr);
        assert_eq!(result.stdout.trim(), "hook-ran");
        assert!(result.error.is_none());
    }
}

#[tokio::test]
async fn fast_exiting_hook_preserves_stdout_when_stdin_is_not_consumed() {
    let temp = tempdir().expect("create temp dir");
    let source_path = AbsolutePathBuf::try_from(temp.path().join("hooks.json"))
        .expect("absolute hook configuration path");
    let handler = ConfiguredHandler {
        event_name: HookEventName::SessionStart,
        matcher: None,
        command: "echo hook-ran".to_string(),
        timeout_sec: 10,
        interactive: false,
        status_message: None,
        additional_context_limit: Default::default(),
        source_path,
        source: HookSource::User,
        display_order: 0,
        env: HashMap::new(),
    };
    let shell = CommandShell {
        program: String::new(),
        args: Vec::new(),
    };
    let input_json = format!(r#"{{"padding":"{}"}}"#, "x".repeat(1024 * 1024));

    let result = run_command(
        &shell,
        &handler,
        /*configured_order*/ 0,
        &input_json,
        temp.path(),
    )
    .await;

    assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr);
    assert_eq!(result.stdout.trim(), "hook-ran");
    assert_eq!(result.error, None);
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(interactive_terminal)]
async fn interactive_runner_requires_an_owner_before_spawn() {
    let temp = tempdir().expect("create temp dir");
    let shell = interactive_test_shell();
    let marker = temp.path().join("no-owner-spawned");
    let handler = interactive_test_handler(
        temp.path(),
        r#"printf spawned > "$MARKER_FILE""#,
        /*timeout_sec*/ 5,
        "MARKER_FILE",
        &marker,
    );

    let result = run_command(
        &shell,
        &handler,
        /*configured_order*/ 0,
        "{}",
        temp.path(),
    )
    .await;

    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("no local terminal owner")),
        "unexpected no-owner result: {result:?}"
    );
    assert!(
        !marker.exists(),
        "interactive command spawned without a terminal owner"
    );
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(interactive_terminal)]
async fn interactive_runner_waits_for_ready_and_releases_after_normal_exit() {
    let temp = tempdir().expect("create temp dir");
    let mut owner = crate::interactive_terminal::register_owner();
    let pid_file = temp.path().join("normal.pid");
    let handler = interactive_test_handler(
        temp.path(),
        r#"printf '%s\n' "$$" > "$PID_FILE"; printf normal-out; printf normal-err >&2"#,
        /*timeout_sec*/ 5,
        "PID_FILE",
        &pid_file,
    );
    let run = spawn_interactive_run(interactive_test_shell(), handler, temp.path().to_path_buf());
    let request = receive_terminal_request(&mut owner).await;
    assert!(
        !pid_file.exists(),
        "interactive command spawned before the ready acknowledgement"
    );
    let finished = request.finished;
    request
        .ready
        .send(Ok(()))
        .expect("acknowledge normal interactive hook");
    let result = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("normal interactive hook timed out")
        .expect("normal interactive runner task");
    finished.await.expect("normal interactive lease completion");
    let pid = wait_for_pid_file(&pid_file).await;
    assert_process_reaped(pid);
    assert_eq!(result.exit_code, Some(0));
    assert_eq!(result.stdout, "normal-out");
    assert_eq!(result.stderr, "normal-err");
    assert_eq!(result.error, None);
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(interactive_terminal)]
async fn interactive_runner_timeout_reaps_before_releasing_terminal() {
    let temp = tempdir().expect("create temp dir");
    let mut owner = crate::interactive_terminal::register_owner();
    let pid_file = temp.path().join("timeout.pid");
    let handler = interactive_test_handler(
        temp.path(),
        r#"printf '%s\n' "$$" > "$PID_FILE"; printf timeout-out; printf timeout-err >&2; exec sleep 30"#,
        /*timeout_sec*/ 2,
        "PID_FILE",
        &pid_file,
    );
    let run = spawn_interactive_run(interactive_test_shell(), handler, temp.path().to_path_buf());
    let request = receive_terminal_request(&mut owner).await;
    let mut finished = request.finished;
    request
        .ready
        .send(Ok(()))
        .expect("acknowledge timing-out interactive hook");
    let pid = wait_for_pid_file(&pid_file).await;
    assert!(process_exists(pid), "timeout child exited too early");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut finished)
            .await
            .is_err(),
        "terminal lease returned while timeout child was still running"
    );
    finished
        .await
        .expect("timed-out interactive lease completion");
    assert_process_reaped(pid);
    let result = tokio::time::timeout(Duration::from_secs(5), run)
        .await
        .expect("timed-out interactive runner did not finish")
        .expect("timed-out interactive runner task");
    assert_eq!(result.exit_code, None);
    assert_eq!(result.stdout, "timeout-out");
    assert_eq!(result.stderr, "timeout-err");
    assert!(
        result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("hook timed out after 2s")),
        "unexpected timeout result: {result:?}"
    );
}

#[cfg(unix)]
#[tokio::test]
#[serial_test::serial(interactive_terminal)]
async fn interactive_runner_cancellation_reaps_before_releasing_terminal() {
    let temp = tempdir().expect("create temp dir");
    let mut owner = crate::interactive_terminal::register_owner();
    let pid_file = temp.path().join("cancelled.pid");
    let handler = interactive_test_handler(
        temp.path(),
        r#"printf '%s\n' "$$" > "$PID_FILE"; printf cancelled-out; exec sleep 30"#,
        /*timeout_sec*/ 30,
        "PID_FILE",
        &pid_file,
    );
    let run = spawn_interactive_run(interactive_test_shell(), handler, temp.path().to_path_buf());
    let request = receive_terminal_request(&mut owner).await;
    let mut finished = request.finished;
    request
        .ready
        .send(Ok(()))
        .expect("acknowledge cancelled interactive hook");
    let pid = wait_for_pid_file(&pid_file).await;
    assert!(process_exists(pid), "cancellation child exited too early");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut finished)
            .await
            .is_err(),
        "terminal lease returned before caller cancellation"
    );
    run.abort();
    assert!(
        run.await
            .expect_err("cancelled runner task unexpectedly completed")
            .is_cancelled(),
        "runner task failed for a reason other than cancellation"
    );
    tokio::time::timeout(Duration::from_secs(5), finished)
        .await
        .expect("cancelled interactive lease was not returned")
        .expect("cancelled interactive lease completion");
    assert_process_reaped(pid);
}

#[cfg(unix)]
fn interactive_test_shell() -> CommandShell {
    CommandShell {
        program: "/bin/sh".to_string(),
        args: vec!["-c".to_string()],
    }
}

#[cfg(unix)]
fn interactive_test_handler(
    directory: &Path,
    command: &str,
    timeout_sec: u64,
    env_key: &str,
    env_path: &Path,
) -> ConfiguredHandler {
    ConfiguredHandler {
        event_name: HookEventName::SessionStart,
        matcher: None,
        command: command.to_string(),
        timeout_sec,
        interactive: true,
        status_message: None,
        additional_context_limit: Default::default(),
        source_path: AbsolutePathBuf::try_from(directory.join("hooks.json"))
            .expect("absolute hook configuration path"),
        source: HookSource::User,
        display_order: 0,
        env: HashMap::from([(env_key.to_string(), env_path.display().to_string())]),
    }
}

#[cfg(unix)]
fn spawn_interactive_run(
    shell: CommandShell,
    handler: ConfiguredHandler,
    cwd: PathBuf,
) -> tokio::task::JoinHandle<CommandRunResult> {
    tokio::spawn(async move {
        run_command(&shell, &handler, /*configured_order*/ 0, "{}", &cwd).await
    })
}

#[cfg(unix)]
async fn receive_terminal_request(
    owner: &mut crate::interactive_terminal::InteractiveTerminalOwner,
) -> crate::interactive_terminal::InteractiveTerminalRequest {
    tokio::time::timeout(Duration::from_secs(5), owner.recv())
        .await
        .expect("interactive terminal request timed out")
        .expect("interactive terminal owner disconnected")
}

#[cfg(unix)]
async fn wait_for_pid_file(path: &Path) -> i32 {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(contents) = fs::read_to_string(path)
                && let Ok(pid) = contents.trim().parse()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("child pid file was not written")
}

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    StdCommand::new("/bin/sh")
        .args(["-c", "kill -0 \"$1\" 2>/dev/null", "sh"])
        .arg(pid.to_string())
        .status()
        .expect("probe child process")
        .success()
}

#[cfg(unix)]
fn assert_process_reaped(pid: i32) {
    assert!(
        !process_exists(pid),
        "interactive child {pid} was still present when the terminal lease returned"
    );
}
