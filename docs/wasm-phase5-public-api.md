# Phase 5: Public wasm entry points

The supported **host↔wasm** surface is the `wasm-bindgen` module **`codex_wasm_bridge`** (crate `codex-wasm-bridge`). Native `codex-core` / `codex-app-server` are **not** yet built for `wasm32`; the bridge is the product-facing ABI for this milestone.

## Wasm exports (host calls into Rust)

| Method                                | Role                                                                                                                   |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `codex_init(config_json)`             | Initialize; `config_json` is JSON (e.g. `{ "version": "0.1.0" }`). Returns JSON `{ ok: true }` or `error`.             |
| `codex_submit(request_json)`          | Submit work; body is `{ correlation_id, kind, payload }`. Returns `{ correlation_id, status: "accepted" }` or `error`. |
| `codex_deliver_callback(result_json)` | Host delivers async results (exec chunks, HTTP, FS, secrets, …).                                                       |
| `codex_cancel(cancel_json)`           | Best-effort cancel by `correlation_id`.                                                                                |
| `codex_shutdown()`                    | Teardown; no further submits.                                                                                          |

Typed reference: [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json).

## Host imports (wasm calls into JS)

The glue expects globals: `host_exec_request`, `host_http_request`, `host_fs_read`, `host_fs_write`, `host_fs_list`, `host_fs_stat`, `host_fs_remove`, `host_secret_get`, `host_secret_set`, `host_sandbox_apply`, and **`host_secret_delete`** where the embedding supports it.

Production implementations live under **`codex-cli/bin/`**:

- `host-http-fetch.js` — HTTP / streaming.
- `host-phase4-capabilities.js` — filesystem, secrets, sandbox stub.

`codex-cli/bin/codex.js` (`--wasm` / `CODEX_WASM=1`) wires these with the native **exec** spawner.

## Structured `payload` keys (shortcut `codex_submit`)

Tools may send structured payloads instead of raw `host_call`: `http`, `fs_read`, `fs_write`, `fs_list`, `fs_stat`, `fs_remove`, `secret_get`, `secret_set`, `sandbox_apply`, and `exec` (maps to `host_exec_request`).

## Node smoke

```bash
node tools/auto-web/e2e-phase5-lifecycle.mjs
```

From repo root: `pnpm run e2e:wasm-phase5-lifecycle`.

## Browser

This repo’s checked-in flow uses **`wasm-bindgen --target no-modules`** for real browser tests (global host imports). The authoritative browser bundle for CI/local E2E is **Puppeteer + Chrome-for-Testing (`chrome@stable`)** — see [`wasm-browser-smoke.md`](wasm-browser-smoke.md).
