# MaceIP WASM Port Roadmap

This roadmap targets the technical outcome described in `WORK.md`: get the repo compiling to `wasm32` with minimal source divergence from upstream, then provide the missing native capabilities through narrow host shims.

It also carries a smaller secondary thread for goal 2 from `WORK.md`: make the wasm bridge and host ABI structured enough that they can later become the foundation of `auto-web`, rather than a one-off port.

## Target outcome

- Native Codex builds continue to work.
- This fork stubs native `codex-linux-sandbox` (no bubblewrap/libcap); see [`maceip-no-native-sandbox.md`](maceip-no-native-sandbox.md).
- A new `codex-rs/wasm-bridge` crate becomes the only wasm-specific entry point.
- Native-only crates such as sandbox, PTY, and terminal UI stay out of the wasm execution path.
- Host runtimes provide exec, networking, filesystem, secrets, and streaming through a narrow ABI.
- **Milestone A (current):** the shipping wasm artifact is **`codex-wasm-bridge`** + host JS; **`codex-core`** / **`codex-app-server`** remain native-only until Milestone B. See [`wasm-support-policy.md`](wasm-support-policy.md).
- **Milestone B (planned):** deepen **`codex-core`** on `wasm32` and widen IDE / RPC coverage via host forwarding — extension IDs and payload sketches live in **`codex_core::wasm_extension`**, [`wasm-extension-interfaces.md`](wasm-extension-interfaces.md), [`wasm-app-server-bridge.md`](wasm-app-server-bridge.md), and `host-contract.json` → `extension_interfaces`. **maceip/v9** checklist: [`wasm-maceip-v9-bridge.md`](wasm-maceip-v9-bridge.md).
- The bridge contracts are explicit enough to be reused later by automated source-conversion tooling.

## Working rules

- Keep upstream changes additive and tightly scoped behind `#[cfg(target_arch = "wasm32")]`.
- Prefer one new bridge crate plus small seam patches over a broad fork.
- Add validation before implementation in every phase.
- Define the host or repo boundary at the start of each phase so `cursor` and `cloud` can work in parallel.
- Treat automation-friendliness as a non-blocking secondary requirement: no extra phase count, but every major seam should stay machine-discoverable and machine-callable.

## Roles

- `cursor`: Rust workspace gating, wasm compileability, native-regression safety, bridge-side contracts, repo docs.
- `cloud`: JS or host runtime, loader glue, fetch or stream adapters, MEMFS, command broker, browser or Node smoke tests.

## Mandatory validation matrix

These checks are the minimum bar and should be introduced as early as the required infrastructure exists.

- `cargo check --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm`
- `cargo check -p codex-core -p codex-app-server -p codex-cli`
- ABI contract tests for bridge request and response payloads
- Shim unit tests for exec, networking, filesystem, secrets, and sandbox stubs
- Host smoke test: instantiate wasm, issue one request, satisfy one host callback, return one result
- CI split between native and wasm so one target cannot silently regress the other

## Phase 0: Validation harness and scope freeze

### Objective

Freeze the actual scope before code churn starts and establish failing checks that define success precisely.

### Cursor checklist

- Inventory workspace crates into `portable`, `gated`, and `excluded` buckets.
- List first-order blockers in `core`, `app-server`, `cli`, `exec`, `keyring-store`, `linux-sandbox`, `windows-sandbox-rs`, `utils/pty`, and `tui`.
- Add a documented wasm build command to the roadmap or repo docs.
- Add native compile guards for `codex-core`, `codex-app-server`, and `codex-cli`.
- Identify where direct process, network, filesystem, and secret access enters the Rust side.

### Cloud checklist

- Define the draft host ABI for exec, HTTP, streaming, filesystem, and secret storage.
- Create a tiny host harness that can load a dummy wasm module and verify exported symbols.
- Define error-shape expectations for missing host services.
- Specify how callbacks, cancellation, and correlation IDs work across the boundary.

### Validation added first

- Failing `cargo check --target wasm32-unknown-unknown ...` for the future bridge crate.
- Failing host smoke test expecting wasm exports.
- Passing native `cargo check` on `codex-core`, `codex-app-server`, and `codex-cli`.

### Goal 2 thread

- Capture the crate inventory and ABI draft in structured tables so later automation can consume them.

### Exit criteria

- Crates are classified.
- Blocking seams are named.
- Host ABI v0 exists.
- Failing wasm-first checks are reproducible.

## Phase 1: Workspace gating and bridge skeleton

### Objective

Make the workspace wasm-aware without dragging native-only crates into the wasm target.

### Cursor checklist

- Add `codex-rs/wasm-bridge` to the workspace.
- Add target-specific dependency gates in workspace and crate manifests where needed.
- Introduce the smallest required `#[cfg(target_arch = "wasm32")]` seams in upstream crates.
- Keep native default builds unchanged.
- Document which crates are intentionally not part of the wasm build graph.

### Cloud checklist

- Create the `wasm-bridge` crate skeleton with `wasm-bindgen` exports.
- Define placeholder Rust and JS types for init, request submission, callback delivery, and shutdown.
- Add a mock host implementation that satisfies the ABI without real side effects.
- Ensure the mock host can load the skeleton and call the exported methods.

### Validation added first

- Test that exported wasm symbols exist.
- Test that native-only crates are excluded from the wasm target.
- Passing native `cargo check` proving the new gating does not perturb default builds.

### Goal 2 thread

- Keep exported method names and payloads regular enough to become machine-generated later.

### Exit criteria

- `codex-wasm-bridge` exists.
- The skeleton builds for `wasm32`.
- Native builds still compile.
- The ABI is fixed enough for independent shim work.

## Phase 2: Execution and terminal abstraction

### Objective

Remove direct process and PTY assumptions from the wasm path and replace them with a host-driven executor contract.

### Cursor checklist

- Trace actual spawn and PTY entry points in `core`, `exec`, `shell-command`, and related crates.
- Introduce a narrow execution interface for wasm paths.
- Gate direct child-process and PTY calls behind native-only code paths.
- Ensure wasm errors are explicit when a host executor is absent.
- Record any terminal-specific behaviors that must be dropped or emulated.

### Cloud checklist

- Implement host-side `ExecRequest` and `ExecResponse` handling.
- Support stdout and stderr chunk delivery, exit reporting, cancellation, and correlation IDs.
- Provide a deterministic fake executor for tests.
- Define whether interactive terminal behavior is unsupported, partially supported, or translated.

### Validation added first

- Contract tests for `ExecRequest`, `ExecChunk`, `ExecExit`, and cancellation payloads.
- Wasm-side tests for missing-host and cancelled-command behavior.
- Native compile guard proving the existing executor path still builds.

### Goal 2 thread

- Make the execution contract generic enough to later drive automated source-conversion tools, not just Codex shell commands.

### Exit criteria

- No wasm path directly invokes PTY or process APIs.
- One mocked command round-trip works end to end.

## Phase 3: Networking and async runtime adaptation

### Objective

Move HTTP and streaming work off native transport assumptions and onto a wasm-safe transport model.

### Cursor checklist

- Identify direct `reqwest`, SSE, WebSocket, and socket assumptions in `core`, `codex-api`, `app-server`, and related crates.
- Add a transport abstraction where wasm currently depends on native networking.
- Gate Tokio usage so wasm does not require multithreaded runtime or native I/O reactors.
- Replace unsupported async spawn points with wasm-safe scheduling where necessary.
- Document which streaming modes are required for parity and which can be deferred.

### Cloud checklist

- Implement fetch-based HTTP.
- Implement streamed response delivery for SSE or equivalent chunk events.
- Translate timeout, cancellation, and transport errors into the Rust contract.
- Provide a browser or Node adapter that matches the Rust-side transport abstraction.

### Validation added first

- Fixture-driven tests for standard HTTP request or response flows.
- Fixture-driven tests for streaming chunk delivery and termination.
- Compile gate that fails if native-only `tokio::net` or equivalent APIs leak into the wasm target.

### Goal 2 thread

- Normalize transport payloads so they can later be replayed by automated tooling and regression harnesses.

### Exit criteria

- Wasm requests and streamed responses work through the host bridge.
- The runtime model is explicit and wasm-safe.

See [`wasm-phase3-networking.md`](wasm-phase3-networking.md) for the reqwest/SSE/WebSocket inventory, **`stream_response` / `http_stream_chunk`** semantics, validation commands, and the intended wasm-safe async model.

Phases 4–6 docs: [`wasm-phase4-host-capabilities.md`](wasm-phase4-host-capabilities.md), [`wasm-phase5-public-api.md`](wasm-phase5-public-api.md), [`wasm-browser-smoke.md`](wasm-browser-smoke.md), [`wasm-support-policy.md`](wasm-support-policy.md). Extension / Milestone B: [`wasm-extension-interfaces.md`](wasm-extension-interfaces.md), [`codex-rs/docs/wasm-port.md`](../codex-rs/docs/wasm-port.md).

## Phase 4: Filesystem, secrets, and sandbox replacement

### Objective

Replace native filesystem, keyring, and sandbox dependencies with host services or stubs.

### Cursor checklist

- Map every wasm-relevant filesystem read, write, list, and metadata call path.
- Map every secret retrieval or storage path that matters in wasm mode.
- Introduce portable filesystem and secret access seams where direct native calls remain.
- Stub sandbox and process-hardening behavior for wasm in a deliberate, documented way.
- Mark unsupported native security behaviors explicitly instead of silently bypassing them.

### Cloud checklist

- Implement MEMFS or proxied filesystem calls with deterministic path semantics.
- Implement JS-backed secret storage.
- Return explicit stubbed responses for sandbox-related calls.
- Define persistence expectations for in-memory versus durable storage.

### Validation added first

- Round-trip tests for filesystem read, write, list, and missing-file behavior.
- Secret present and absent tests.
- Stubbed sandbox tests that assert explicit outcomes.

### Goal 2 thread

- Keep filesystem and secret operations in a host-call form that future automation can intercept or simulate.

### Exit criteria

- The wasm path no longer depends directly on native filesystem or credential APIs.
- Native-only sandbox crates are fully outside the wasm execution path.

## Phase 5: Public entry points and host loader conversion

### Objective

Turn Codex from a spawned native binary into a wasm library that a host can load and drive.

### Cursor checklist

- Expose stable `wasm-bindgen` entry points for init, request submission, callback result delivery, and shutdown.
- Choose the supported wasm-facing API shape between `core` and `app-server` and document it.
- Ensure request and response types crossing the boundary are stable and versioned.
- Keep unsupported surfaces such as the native TUI out of the public wasm contract.

### Cloud checklist

- Replace native process-spawn logic in `codex-cli/bin/codex.js` with a wasm loader.
- Wire imports and exports for fs, network, exec, and secrets.
- Make the host drive the full request lifecycle.
- Add one Node-oriented smoke flow and one browser-oriented smoke flow if the browser target is still in scope.

### Validation added first

- Host smoke tests for init, one request, one host callback, one callback result, and shutdown.
- ABI tests locking exported method names and payload shapes.
- Regression check that native CLI behavior remains on the native path.

### Goal 2 thread

- Design the exported entry points as reusable machine-callable operations, not CLI-specific side effects.

### Exit criteria

- A host can instantiate the wasm module and drive a full request lifecycle without spawning the native binary.

## Phase 6: Product surface triage, docs, and hardening

### Objective

Make the supported wasm product shape explicit and keep the port sustainable through upstream syncs.

### Cursor checklist

- Document supported and unsupported wasm surfaces in `docs/` and `codex-rs/docs/`.
- Add CI jobs for native and wasm tracks.
- Add guardrails so unsupported features fail clearly instead of crashing.
- Document the expected merge surface for upstream syncs.
- Capture deferred work items that are intentionally not part of the first wasm milestone.

### Cloud checklist

- Add end-to-end smoke coverage in the host environment.
- Measure basic initialization and round-trip latency.
- Add compatibility checks to catch ABI drift quickly.
- Document host prerequisites and runtime assumptions.

### Validation added first

- CI jobs for native and wasm.
- Unsupported-feature tests asserting clear user-visible errors.
- End-to-end happy-path fixture test exercising the documented supported flow.

### Goal 2 thread

- Publish the bridge and host contract as the first reusable substrate for `auto-web` experimentation.

### Exit criteria

- The repo has a documented wasm support policy.
- CI enforces the policy.
- The merge surface stays small.
- The automation foundation is explicit, even if only minimally exercised.

## Definition of done

- `codex-wasm-bridge` builds for `wasm32-unknown-unknown` from the main workspace.
- Native `codex-core`, `codex-app-server`, and `codex-cli` still build.
- Exec, network, filesystem, and secret work cross the wasm boundary through explicit contracts.
- A host can drive one real request lifecycle.
- Unsupported surfaces such as native TUI and native sandboxing are excluded deliberately and documented clearly.
- The bridge contracts are structured enough to serve as a base for later `auto-web` work.
