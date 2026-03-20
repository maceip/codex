use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;

use crate::process::SpawnedProcess;
use crate::process::TerminalSize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStdinPolicy {
    Piped,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessTransport<'a> {
    Pty {
        size: TerminalSize,
        inherited_fds: &'a [i32],
    },
    Pipe {
        stdin: ProcessStdinPolicy,
        inherited_fds: &'a [i32],
    },
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessSpawnRequest<'a> {
    pub program: &'a str,
    pub args: &'a [String],
    pub cwd: &'a Path,
    pub env: &'a HashMap<String, String>,
    pub arg0: &'a Option<String>,
    pub transport: ProcessTransport<'a>,
}

impl<'a> ProcessSpawnRequest<'a> {
    pub fn pty(
        program: &'a str,
        args: &'a [String],
        cwd: &'a Path,
        env: &'a HashMap<String, String>,
        arg0: &'a Option<String>,
        size: TerminalSize,
    ) -> Self {
        Self::pty_with_inherited_fds(program, args, cwd, env, arg0, size, &[])
    }

    pub fn pty_with_inherited_fds(
        program: &'a str,
        args: &'a [String],
        cwd: &'a Path,
        env: &'a HashMap<String, String>,
        arg0: &'a Option<String>,
        size: TerminalSize,
        inherited_fds: &'a [i32],
    ) -> Self {
        Self {
            program,
            args,
            cwd,
            env,
            arg0,
            transport: ProcessTransport::Pty {
                size,
                inherited_fds,
            },
        }
    }

    pub fn pipe(
        program: &'a str,
        args: &'a [String],
        cwd: &'a Path,
        env: &'a HashMap<String, String>,
        arg0: &'a Option<String>,
    ) -> Self {
        Self::pipe_with_inherited_fds(
            program,
            args,
            cwd,
            env,
            arg0,
            ProcessStdinPolicy::Piped,
            &[],
        )
    }

    pub fn pipe_no_stdin(
        program: &'a str,
        args: &'a [String],
        cwd: &'a Path,
        env: &'a HashMap<String, String>,
        arg0: &'a Option<String>,
    ) -> Self {
        Self::pipe_with_inherited_fds(
            program,
            args,
            cwd,
            env,
            arg0,
            ProcessStdinPolicy::Closed,
            &[],
        )
    }

    pub fn pipe_with_inherited_fds(
        program: &'a str,
        args: &'a [String],
        cwd: &'a Path,
        env: &'a HashMap<String, String>,
        arg0: &'a Option<String>,
        stdin: ProcessStdinPolicy,
        inherited_fds: &'a [i32],
    ) -> Self {
        Self {
            program,
            args,
            cwd,
            env,
            arg0,
            transport: ProcessTransport::Pipe {
                stdin,
                inherited_fds,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcessSpawner;

impl NativeProcessSpawner {
    pub async fn spawn(self, request: ProcessSpawnRequest<'_>) -> Result<SpawnedProcess> {
        match request.transport {
            ProcessTransport::Pty {
                size,
                inherited_fds,
            } => {
                crate::pty::spawn_process_with_inherited_fds(
                    request.program,
                    request.args,
                    request.cwd,
                    request.env,
                    request.arg0,
                    size,
                    inherited_fds,
                )
                .await
            }
            ProcessTransport::Pipe {
                stdin,
                inherited_fds,
            } => match stdin {
                ProcessStdinPolicy::Piped => {
                    crate::pipe::spawn_process_with_inherited_fds(
                        request.program,
                        request.args,
                        request.cwd,
                        request.env,
                        request.arg0,
                        inherited_fds,
                    )
                    .await
                }
                ProcessStdinPolicy::Closed => {
                    crate::pipe::spawn_process_no_stdin_with_inherited_fds(
                        request.program,
                        request.args,
                        request.cwd,
                        request.env,
                        request.arg0,
                        inherited_fds,
                    )
                    .await
                }
            },
        }
    }
}
