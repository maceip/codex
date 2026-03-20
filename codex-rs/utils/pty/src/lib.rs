pub mod pipe;
mod process;
pub mod process_group;
pub mod pty;
pub mod spawn;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod win;

pub const DEFAULT_OUTPUT_BYTES_CAP: usize = 1024 * 1024;

/// Spawn a non-interactive process using regular pipes, but close stdin immediately.
/// Combine stdout/stderr receivers into a single broadcast receiver.
pub use process::combine_output_receivers;
/// Handle for interacting with a spawned process (PTY or pipe).
pub use process::ProcessHandle;
/// Bundle of process handles plus split output and exit receivers returned by spawn helpers.
pub use process::SpawnedProcess;
/// Terminal size in character cells used for PTY spawn and resize operations.
pub use process::TerminalSize;
/// Spawn a non-interactive process using regular pipes for stdin/stdout/stderr.
pub use spawn::NativeProcessSpawner;
pub use spawn::ProcessSpawnRequest;
pub use spawn::ProcessStdinPolicy;
pub use spawn::ProcessTransport;
/// Backwards-compatible alias for ProcessHandle.
pub type ExecCommandSession = ProcessHandle;
/// Backwards-compatible alias for SpawnedProcess.
pub type SpawnedPty = SpawnedProcess;
/// Report whether ConPTY is available on this platform (Windows only).
pub use pty::conpty_supported;
#[cfg(windows)]
pub use win::conpty::RawConPty;

/// Spawn a non-interactive process using regular pipes for stdin/stdout/stderr.
pub async fn spawn_pipe_process(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pipe(program, args, cwd, env, arg0))
        .await
}

/// Spawn a non-interactive process using regular pipes, preserving selected
/// inherited file descriptors across exec on Unix.
pub async fn spawn_pipe_process_with_inherited_fds(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
    inherited_fds: &[i32],
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pipe_with_inherited_fds(
            program,
            args,
            cwd,
            env,
            arg0,
            ProcessStdinPolicy::Piped,
            inherited_fds,
        ))
        .await
}

/// Spawn a non-interactive process using regular pipes, but close stdin immediately.
pub async fn spawn_pipe_process_no_stdin(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pipe_no_stdin(
            program, args, cwd, env, arg0,
        ))
        .await
}

/// Spawn a non-interactive process using regular pipes, close stdin
/// immediately, and preserve selected inherited file descriptors across exec on
/// Unix.
pub async fn spawn_pipe_process_no_stdin_with_inherited_fds(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
    inherited_fds: &[i32],
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pipe_with_inherited_fds(
            program,
            args,
            cwd,
            env,
            arg0,
            ProcessStdinPolicy::Closed,
            inherited_fds,
        ))
        .await
}

/// Spawn a process attached to a PTY for interactive use.
pub async fn spawn_pty_process(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
    size: TerminalSize,
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pty(
            program, args, cwd, env, arg0, size,
        ))
        .await
}

/// Spawn a process attached to a PTY for interactive use while preserving
/// selected inherited file descriptors across exec on Unix.
pub async fn spawn_pty_process_with_inherited_fds(
    program: &str,
    args: &[String],
    cwd: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
    arg0: &Option<String>,
    size: TerminalSize,
    inherited_fds: &[i32],
) -> anyhow::Result<SpawnedProcess> {
    NativeProcessSpawner
        .spawn(ProcessSpawnRequest::pty_with_inherited_fds(
            program,
            args,
            cwd,
            env,
            arg0,
            size,
            inherited_fds,
        ))
        .await
}
