## Tracked integration cliff (maceip/v9)

This tree does **not** yet run automated E2E inside **[maceip/v9](https://github.com/maceip/v9)**. Contract + wasm gates live here; proving the EdgeJS embedding is **cross-repo** work — see [`docs/wasm-v9-integration-status.md`](docs/wasm-v9-integration-status.md) and [`docs/wasm-maceip-v9-bridge.md`](docs/wasm-maceip-v9-bridge.md).

---

GOAL:
the goal of this repo is twofold: 1) convert as much of it as possible so it runs in maceip/v9, while minimizing the amount of source changes we have to make). 2) Is the foundation for auto-web, A framework we need to start building with this that allows us automate as much as humanly physically the conversion of programs that are compiled in languages that support Web assembly as a back end namely Rust and potentially others like C++ Llvm, kotlin->wasm

Now as for the first goal:

we have input from two advisors on what to do :

////1
To port the Codex core to WebAssembly (wasm32), you must move from a "Process Management" architecture to a "Library Export" architecture. Even with the sandbox disabled, the primary challenge is that WebAssembly cannot natively perform many of the low-level operations that the current Rust core requires to function as a standalone binary.

Here are the core architectural and code-level changes needed to scaffold this port:

1. Cargo Workspace Reconfiguration
   WebAssembly environments (especially browsers) cannot link to many of the crates currently in the workspace. You will need to use target-specific dependencies and conditionally exclude OS-heavy crates.

Exclude OS Crates: In the root codex-rs/Cargo.toml, you must wrap crates like codex-linux-sandbox and codex-windows-sandbox in [target.'cfg(not(target_arch = "wasm32"))'.dependencies] blocks.

Dependency Swaps:

Networking: Replace the native features of reqwest and rama-\* with their Wasm equivalents (which use the browser's fetch API).

Async Runtime: The "full" features of tokio (multi-threading, native sockets) do not work in standard Wasm. You will likely need to switch to a single-threaded runtime or use a crate like wasm-bindgen-futures.

2. Stubbing or Replacing System Crates
   Even if you turn off the sandbox logic, the types and traits from those crates are likely referenced elsewhere in the "brain" of the agent.

linux-sandbox / windows-sandbox-rs: Since you are disabling these, you must provide a "Stub" crate that implements the same trait interface (e.g., apply_sandbox_policy) but simply returns Ok(()) without calling native primitives like libc::prctl.

utils/pty: This crate handles Pseudo-Terminals and process forking. This must be completely replaced. In a Wasm environment, instead of spawning a local shell, the agent would need to emit "command requests" that your Node.js or browser host executes and returns results for.

keyring-store: Native credential storage must be replaced with a JS-backed storage provider (e.g., localStorage or a secure cookie handler).

3. Transition to wasm-bindgen Entry Points
   The current architecture is built around a standard native main() function. For WebAssembly, the Rust core needs to become a library.

Export Logic: You will need to add wasm-bindgen to the codex-core crate.

Message Loop: Instead of the Rust core managing its own event loop and I/O, you should export a function like pub fn process_message(input: String) -> Promise<String>.

Serialization: The current project relies heavily on serde_json. This is Wasm-compatible, but you will need to ensure all protocol structures (V1 and V2) are correctly decorated for JS interop.

4. Replacing the Node.js Launcher
   The current JavaScript entry point in codex-cli/bin/codex.js is designed to detect an OS and spawn a child process.

Loader Shift: You must rewrite codex.js to use WebAssembly.instantiateStreaming.

System Call Mocking: Since Wasm cannot directly access the filesystem, the JS side must provide a "Virtual File System" or proxy the Rust core's fs calls back to the Node.js fs module using wasm imports.

5. Managing Async and Streams
   Codex uses Server-Sent Events (SSE) and WebSockets for real-time updates.

Event Pipeline: The codex-api and app-server-protocol crates define complex JSON-RPC and SSE message flows. In Wasm, these will need to be re-routed through JS EventSource or WebSocket objects rather than using native Rust TCP streams.

Summary Checklist for your Scaffold:
[ ] Add [target.'cfg(target_arch = "wasm32")'] to Cargo.toml.

[ ] Create a "No-Op" version of linux-sandbox.

[ ] Replace tokio's rt-multi-thread with wasm-bindgen-futures.

[ ] Replace pty utilities with a JS-interop command executor.

[ ] Change codex-cli from a spawn launcher to a WASM module loader.
/////1

and the second advice:

/////2
Codex CLI → WASM: Minimal Fork Strategy
Architecture
Codex is a Rust workspace with 73 crates + a thin Node.js/TypeScript wrapper (codex-cli/bin/codex.js) that spawns the native binary. The actual logic lives in codex-rs/ with these key layers:

Layer Crates WASM-compatible?
Core logic (prompt assembly, tool dispatch, conversation state) core/, proto/ Yes, mostly pure Rust + serde
Sandbox (bubblewrap, landlock, seccomp, Windows sandbox) linux-sandbox/, macos-sandbox/, windows-sandbox/ Drop entirely — your stated intent
Exec/PTY (process spawning, terminal) exec/, pty-utils/ Replace with our shims
Networking (reqwest + rustls) via core/ deps Replace with browser fetch
TUI (ratatui + crossterm) tui/ Replace with our terminal emitter
App-server (JSON-RPC for IDEs) app-server/ Good candidate for WASM entry point
Strategy: Thin Shim Layer, Not a Fork
The goal is minimal diff from upstream so you can pull updates. That means:

1. Don't modify upstream crates — add a new codex-rs/wasm-bridge/ crate

codex-rs/
wasm-bridge/ ← NEW: the only crate we add
Cargo.toml ← depends on codex-core with feature flags
src/lib.rs ← wasm_bindgen entry points
src/exec_shim.rs ← replaces PTY/process spawn with message passing
src/net_shim.rs ← replaces reqwest with browser fetch via js-sys
src/fs_shim.rs ← replaces std::fs with our MEMFS bridge

2. Use Cargo feature flags to gate platform code

The key insight: upstream already uses #[cfg(target_os = "linux")] etc. for sandbox code. We add #[cfg(target_arch = "wasm32")] guards in the minimal places where the core crate calls into platform APIs. This is ~5-10 surgical #[cfg] additions to upstream crates — the smallest possible diff.

The critical swap points:

Process execution — exec crate's spawn() → our postMessage bridge to the browser
Network I/O — reqwest::Client → wasm-bindgen-futures + browser fetch()
Filesystem — std::fs → calls into our JS MEMFS via wasm_bindgen
Tokio runtime — tokio::runtime::Runtime → wasm-bindgen-futures::spawn_local (tokio has a wasm32 feature but no I/O reactor; only timers + task spawn work) 3. Patch list (upstream crate changes — keep minimal)

Upstream crate Change Size
core/Cargo.toml Add wasm feature flag 2 lines
core/src/exec.rs (or equivalent) #[cfg(not(target_arch = "wasm32"))] around spawn + #[cfg(wasm32)] calling our shim ~10 lines per call site
core/src/http_client.rs Same pattern for reqwest calls ~10 lines
Workspace Cargo.toml Add wasm-bridge member 1 line
.cargo/config.toml Add [target.wasm32-unknown-unknown] section 3 lines
Total upstream diff: ~40-60 lines, all behind #[cfg(wasm32)] so they're invisible to native builds.

4. What we DON'T compile for WASM

Exclude from the wasm target entirely (via workspace feature flags or #[cfg]):

linux-sandbox, macos-sandbox, windows-sandbox — your stated intent
tui/ — ratatui/crossterm need a real terminal
exec/ — relies on PTY + fork/execve
pty-utils/ — POSIX PTY calls
cli/ — the Clap binary entry point (we use wasm-bridge instead)
Any crate pulling in libc, nix, landlock, seccompiler, bubblewrap 5. Build command

cargo build --target wasm32-unknown-unknown \
 -p codex-wasm-bridge \
 --features wasm \
 --no-default-features

Then wasm-bindgen to generate JS glue, and our existing napi-bridge + EdgeJS runtime hosts it.

Sync Strategy
upstream/main ──────────────────────────────────►
\ \ \
 merge merge merge
↓ ↓ ↓
our-fork/main ──[+wasm-bridge]──────────────────►

Merges from upstream: trivial because our changes are either (a) additive (new wasm-bridge/ crate) or (b) behind #[cfg(wasm32)] which upstream won't touch
Conflict surface: only the ~40 lines of #[cfg] patches + Cargo.toml member list
Automation: a CI job can git merge upstream/main and run cargo check --target wasm32-unknown-unknown to catch breakage immediately
Biggest Risk
tokio on wasm32. Tokio's reactor doesn't work in WASM — only spawn_local and timers. If codex-core uses tokio::spawn (multi-threaded) or tokio::net, those won't compile. The fix is either:

Swap to wasm-bindgen-futures::spawn_local behind #[cfg(wasm32)]
Or use tokio = { features = ["rt", "macros"] } (no rt-multi-thread, no net, no io-util) for the wasm target
This is the single largest source of #[cfg] patches but it's well-trodden ground — many Rust projects do this.
////2

# TASK 01

Synthesize to advisory sections create all planned phase doc that has two employees listed We're going to call them cursor and cloud ideally the work that they is independent and they don't block and the document you create is a for all the work that needs to get done to enable this repo to run in maceip/v8 -- or put another way, compile to wasm + minimal shims.

- your phases should include mandatory validation tests ahead of time
  the phases must be descriptive with as much detail as possible
  no overly long and verbose: we dont want 20+ phases as an exmaple
