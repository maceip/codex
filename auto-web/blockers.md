# Current Blockers

## Open blockers

1. `codex-rs/wasm-bridge` does not exist yet.
   Impact: there is no concrete wasm build target or exported surface to validate.

2. The host ABI is not finalized.
   Impact: repo-side seam work and host-side adapter work can drift before they meet.

3. Tokio runtime usage is still native-oriented.
   Impact: `tokio::process`, `tokio::net`, and multithreaded runtime assumptions currently prevent a clean wasm target graph.

4. The JS launcher is still native-spawn only.
   Impact: `codex-cli/bin/codex.js` cannot participate in wasm loading or callback routing yet.

5. The milestone label is inconsistent in `WORK.md`.
   Impact: docs and packaging should avoid runtime-version-specific claims until `maceip/v8` versus `maceip/v9` is resolved.

## What is unblocked now

- Planning and machine-readable artifact generation in `auto-web/`
- Repo-side inventory and seam identification
- Validation-first definitions for compile checks, contract tests, and smoke tests
- Documentation that narrows the first supported wasm surface

## Recommended next implementation steps

1. Cursor: create `codex-rs/wasm-bridge` and wire the minimal workspace gating needed for `cargo check --target wasm32-unknown-unknown`.
2. Cloud: write the first draft of `auto-web/abi/host-contract.json` and the host harness expected by the smoke test.
3. Codex: keep `auto-web` artifacts synchronized with the actual seam and ABI decisions as code work lands.
