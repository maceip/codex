# Codex WASM Port Plan

This is the first checked-in `auto-web` planning bundle for the Codex repo.
It translates the roadmap in `docs/maceip-wasm-roadmap.md` into reusable artifacts that future automation can consume.

## Scope

- Primary goal: compile the Rust workspace to `wasm32-unknown-unknown` with minimal upstream divergence and host shims for native-only capabilities.
- Secondary goal: keep the seams, validations, and handoffs structured enough to become the first reusable `auto-web` harness pilot.
- Supported first milestone: repo-side bridge plus one host-driven request lifecycle.
- Explicitly out of first milestone: native TUI parity, native sandbox behavior, broad browser-only productization, and full autonomous source translation.

## Current assumptions

- The repo should stay close to upstream by adding `codex-rs/wasm-bridge` and limiting upstream changes to narrow `#[cfg(target_arch = "wasm32")]` seams.
- `core` and `app-server` are the intended wasm-facing surfaces; `tui`, PTY-heavy execution, and native sandbox crates are not.
- Host capabilities will cover exec, network, filesystem, secrets, streaming, and explicit sandbox stubs.
- `WORK.md` currently mentions both `maceip/v8` and `maceip/v9`; this plan stays version-neutral until that target is clarified.

## Work tracks

### Cursor

- Own repo-side compile gating, workspace membership, bridge exports, and native-regression safety.
- Create the `codex-rs/wasm-bridge` crate and the wasm-specific seams in `core`, `app-server`, `cli`, `codex-api`, `secrets`, and related crates.
- Keep native `cargo check` and later crate-level tests green.

### Cloud

- Own host runtime loading, adapter contracts, smoke tests, and eventual `codex-cli/bin/codex.js` loader conversion.
- Implement the host contracts for exec, network, filesystem, secrets, and streaming.
- Prove one end-to-end wasm round-trip in the target environment.

### Codex

- Own planning artifacts, phase structure, validation requirements, coordination hygiene, and conflict arbitration.
- Keep `auto-web` outputs machine-readable so later automation can replay or regenerate the same plan.
- Fill any unclaimed documentation or repo-inventory gaps that unblock Cursor or Cloud without colliding with their file claims.

## Phase sequence

1. Validation harness and scope freeze
   Add failing wasm-first checks, a draft host ABI boundary, and a crate inventory before implementation churn begins.
2. Workspace gating and bridge skeleton
   Add the `wasm-bridge` crate, target-specific dependency gating, and exported placeholder entrypoints.
3. Execution and terminal abstraction
   Move process and PTY behavior behind an explicit wasm-safe host executor contract.
4. Networking and async runtime adaptation
   Replace native HTTP and streaming assumptions with host transport contracts and wasm-safe task scheduling.
5. Filesystem, secrets, and sandbox replacement
   Route storage and secret access through host services and make sandbox behavior explicit stubs on wasm.
6. Public entry points and host loader conversion
   Make the wasm library loadable by the host and replace native process spawning in the JS launcher path.
7. Product surface triage, docs, and hardening
   Document the supported surface, add CI separation, and lock down unsupported behaviors with explicit failures.

## Mandatory validation

- `cargo check --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm`
- `cargo check -p codex-core -p codex-app-server -p codex-cli`
- ABI contract tests for bridge request and response payloads
- Shim tests for exec, network, filesystem, secrets, and sandbox stubs
- Host smoke test that loads the wasm module, completes one request, services one callback, and returns one result
- CI split so native and wasm regressions are isolated and visible

## First artifacts to keep updated

- `auto-web/repo-port-plan.json`
- `auto-web/phase-map.json`
- `auto-web/coordination.yaml`
- `auto-web/logbook.yaml`

## Immediate blockers

- The host ABI remains a draft; extend it as new structured submit kinds solidify.
- The first host target is still open: Node-only is the simplest first milestone, but browser support is still listed as a live question.
- This fork stubs `codex-linux-sandbox` (no libcap/bubblewrap); core tests should not require those build inputs.
