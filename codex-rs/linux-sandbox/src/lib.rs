//! Linux bubblewrap/Landlock/seccomp sandbox **is not built** in this tree.
//!
//! The intended runtime (wasm / hosted) uses host-side policy instead. The
//! `codex-linux-sandbox` binary and library remain so `codex-arg0` dispatch and
//! workspace layout stay stable, but invocation exits immediately with a clear code.

/// Entry point used when `argv[0]` is `codex-linux-sandbox` (see the `codex-arg0` crate).
#[cfg(target_os = "linux")]
pub fn run_main() -> ! {
    eprintln!(
        "codex-linux-sandbox is disabled in this build (no native bubblewrap/Landlock). Use the host runtime."
    );
    std::process::exit(2)
}

#[cfg(not(target_os = "linux"))]
pub fn run_main() -> ! {
    eprintln!("codex-linux-sandbox is only meaningful on Linux.");
    std::process::exit(2)
}
