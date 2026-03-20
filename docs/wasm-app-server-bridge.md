# App-server bridge strategy (wasm client → native server)

## Goal

Expose **app-server v2** JSON-RPC–style methods (`thread/read`, `config/read`, …) to logic
running inside **`codex-wasm-bridge`** (and eventually **`codex-core`** on wasm) **without**
compiling `codex-app-server` or Axum into `wasm32`.

## Mechanism (planned)

1. Wasm issues a host call **`host_app_server_rpc`** with payload matching
   [`WasmBridgeAppServerRpcEnvelope`](../codex-rs/core/src/wasm_extension.rs).
2. The host (Node, EdgeJS, IDE worker) serializes the frame onto the **real** app-server
   transport (stdio socket, HTTP, etc.).
3. The host receives the JSON response and calls **`codex_deliver_callback`** with
   `capability` appropriate to the bridge kernel (TBD: dedicated capability vs `exec`-like).

## Wire compatibility

- Use **camelCase** on the wire for new payloads (align with
  [`AGENTS.md`](../AGENTS.md) app-server v2 rules).
- Keep method paths **identical** to v2 RPC names so the same TypeScript / Rust clients
  can run in native and wasm-hosted modes.

## Related

- [`wasm-extension-interfaces.md`](wasm-extension-interfaces.md)
- [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json) → `extension_interfaces`
