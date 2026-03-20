# WASM support policy (Phase 6)

This fork’s **supported** wasm shape is **`codex-wasm-bridge`** plus the host ABI in [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json). It exists so Cloud/embeddings can drive Codex logic without the native CLI binary.

## Supported

- Building `codex-wasm-bridge` for `wasm32-unknown-unknown` with `--no-default-features --features wasm`.
- Host runtimes (Node ≥22 is CI-representative) that implement the import table and load the generated glue from `wasm-bindgen`.
- Production helpers in `codex-cli/bin/host-http-fetch.js` and `host-phase4-capabilities.js`.
- `codex-cli --wasm` as the reference loader.

## Explicitly unsupported (first milestone)

- `codex-core` / `codex-app-server` / TUI as wasm32 artifacts.
- Native Linux sandbox / bubblewrap in wasm mode (see [`maceip-no-native-sandbox.md`](maceip-no-native-sandbox.md)); `host_sandbox_apply` is a **stub** only.
- Parity with every `reqwest` / `tokio::net` code path in core.

## CI

Workflow **`.github/workflows/wasm-bridge-ci.yml`** runs the wasm32 compile gate, crate tests, `wasm-bindgen` emit (Node + browser bundle), Node harnesses, **Puppeteer + Chrome-for-Testing (`chrome@stable`)** browser E2E, and export verification. If it fails, treat as release-blocking for wasm-related changes.

Browser bundle policy: see **`docs/wasm-browser-smoke.md`** (Puppeteer-managed **Chrome for Testing**).

## Upstream merges

Prefer **additive** bridge and `#[cfg(target_arch = "wasm32")]` seams; keep the wasm graph confined to `codex-wasm-bridge` and host JS until core wasm becomes a tracked goal.

## Deferred

Long-lived WebSockets, raw TCP from wasm, and multithreaded Tokio in the bridge remain **out of scope** until hosts implement the reserved imports — **interface names and payload sketches** are frozen in **`codex_core::wasm_extension`**, **`codex-wasm-bridge::extension_ids`**, and **`auto-web/abi/host-contract.json`** → `extension_interfaces`. See [`wasm-extension-interfaces.md`](wasm-extension-interfaces.md) and [`wasm-app-server-bridge.md`](wasm-app-server-bridge.md).

**maceip/v9** embedding is tracked in [`wasm-maceip-v9-bridge.md`](wasm-maceip-v9-bridge.md).
