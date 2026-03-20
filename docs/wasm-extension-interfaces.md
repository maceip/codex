# Wasm extension interfaces (core, TCP, WebSocket, app-server)

This doc ties together **planned** surfaces that go beyond the Phase 1–6 thin bridge.

## Single source of strings

| Location | Role |
|----------|------|
| [`codex-rs/core/src/wasm_extension.rs`](../codex-rs/core/src/wasm_extension.rs) | `HOST_*` import names, `SUBMIT_KEY_*`, serde **payload sketches** |
| [`codex-rs/wasm-bridge/src/extension_ids.rs`](../codex-rs/wasm-bridge/src/extension_ids.rs) | **Mirror** of those strings for the wasm crate (sync tested) |
| [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json) → `extension_interfaces` | Human + tool-consumable map |

Native tests in **`codex-wasm-bridge`** fail if core and bridge IDs diverge.

## Natural insertion points

1. **`host_call_from_request`** in [`wasm_bridge.rs`](../codex-rs/core/src/wasm_bridge.rs): extend the `STRUCTURED` table (or parallel match) so `payload.ws` / `payload.tcp` / `payload.app_server_rpc` map to `HOST_WEBSOCKET_REQUEST`, `HOST_TCP_SOCKET`, `HOST_APP_SERVER_RPC`.
2. **`codex-wasm-bridge` `host_imports`**: add matching `#[wasm_bindgen(js_name = …)]` extern fns when the host is ready.
3. **JS hosts** (`codex-cli/bin/`): add handlers next to `host-http-fetch.js` / `host-phase4-capabilities.js`.
4. **`codex-core` on wasm**: pull in only the subgraph that compiles; keep I/O behind these host calls (no `tokio::net` in wasm).

## TCP vs WebSocket vs HTTP

- **HTTP** is implemented today (`host_http_request` + streaming chunks).
- **WebSocket** is the right fit for **SSE-like** or **bidirectional** API channels; sketch: `WasmBridgeWebSocketHandshake` → host opens browser `WebSocket` or EdgeJS equivalent.
- **TCP** is for **non-TLS sockets** (MCP, custom wire protocols); often **absent in browsers** — hosts return `missing_capability` unless running under EdgeJS/Node with a capability broker.

## App-server RPC without compiling `codex-app-server` into wasm

**`host_app_server_rpc`** carries v2-shaped JSON-RPC (`method`, `params`, optional `id`).
The host forwards to a **native** `codex-app-server` (or a proxy), then delivers the JSON
result via `codex_deliver_callback`. That gives a **fuller IDE surface** from wasm **clients**
without moving Axum/net stacks into the module.

See also [`wasm-app-server-bridge.md`](wasm-app-server-bridge.md).
