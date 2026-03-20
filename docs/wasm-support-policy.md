# WASM support policy (Phase 6)

This fork’s **supported** wasm shape is **`codex-wasm-bridge`** plus the host ABI in [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json). It exists so Cloud/embeddings can drive Codex logic without the native CLI binary.

## Supported

- Building `codex-wasm-bridge` for `wasm32-unknown-unknown` with `--no-default-features --features wasm`.
- Host runtimes (Node ≥22 is CI-representative) that implement the import table and load the generated glue from `wasm-bindgen`.
- Production helpers in `codex-cli/bin/host-http-fetch.js` and `host-phase4-capabilities.js`.
- `codex-cli --wasm` as the reference loader.

## Milestone A vs B (single truth)

- **Milestone A (current):** the supported wasm **artifact** is **`codex-wasm-bridge`** + host JS. **`codex-core`** / **`codex-app-server`** / TUI are **not** wasm32 products in this milestone — only the bridge kernel and host ABI are.
- **Kernel vs host:** structured submit keys `ws` / `tcp` / `app_server_rpc` are **routed in the kernel** to wasm imports; hosts may still respond with **`missing_capability`** until real brokers exist. That is **in scope for Milestone A** as contract + dispatch, not as full TCP/WebSocket/app-server-in-wasm parity.
- **Milestone B (planned):** deeper **`codex-core`** on `wasm32` and richer IDE/RPC — see [`maceip-wasm-roadmap.md`](maceip-wasm-roadmap.md).

## Explicitly unsupported in Milestone A

- Shipping **`codex-core`** / **`codex-app-server`** / **TUI** as wasm32 artifacts (bridge-only milestone).
- Native Linux sandbox / bubblewrap in wasm mode (see [`maceip-no-native-sandbox.md`](maceip-no-native-sandbox.md)); `host_sandbox_apply` is a **stub** only.
- Parity with every native `reqwest` / `tokio::net` code path inside wasm.

## CI

Workflow **`.github/workflows/wasm-bridge-ci.yml`** runs the wasm32 compile gate, crate tests, `wasm-bindgen` emit (Node + browser bundle), Node harnesses, **Puppeteer + Chrome-for-Testing (`chrome@stable`)** browser E2E, and export verification. If it fails, treat as release-blocking for wasm-related changes.

Browser bundle policy: see **`docs/wasm-browser-smoke.md`** (Puppeteer-managed **Chrome for Testing**).

## Upstream merges

Prefer **additive** bridge and `#[cfg(target_arch = "wasm32")]` seams; keep the wasm graph confined to `codex-wasm-bridge` and host JS until core wasm becomes a tracked goal.

## Deferred (beyond Milestone A)

- **Full** long-lived WebSocket/TCP **implementations** inside the wasm module (kernel routes to host; real I/O is host-dependent).
- **Multithreaded Tokio** and other native runtime assumptions in the bridge graph.
- **v9 runtime proof** as an automated job in this repo — see [`wasm-v9-integration-status.md`](wasm-v9-integration-status.md).

Payload sketches and import names stay frozen in **`codex_core::wasm_extension`**, **`codex-wasm-bridge::extension_ids`**, and **`auto-web/abi/host-contract.json`** → `extension_interfaces`. See [`wasm-extension-interfaces.md`](wasm-extension-interfaces.md) and [`wasm-app-server-bridge.md`](wasm-app-server-bridge.md).

**maceip/v9** checklist: [`wasm-maceip-v9-bridge.md`](wasm-maceip-v9-bridge.md).
