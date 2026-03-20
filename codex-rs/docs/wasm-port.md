# WebAssembly port notes (`codex-core`)

This crate is **not** built for `wasm32` today. The **`codex-wasm-bridge`** crate is the
only wasm32 artifact; **`codex-core`** still exposes stable **contract types** the bridge
and hosts share.

## Extension surface (`codex_core::wasm_extension`)

Use `wasm_extension` for **reserved capability names**, future **`codex_submit` payload
keys** (`ws`, `tcp`, `app_server_rpc`), and **serde sketches** for WebSocket / TCP /
app-server RPC envelopes. Wire these into `wasm_bridge::host_call_from_request` when
`codex-core` logic begins executing on wasm.

## Documentation map

| Doc | Purpose |
|-----|---------|
| Repo `docs/maceip-wasm-roadmap.md` | Phases, exit criteria |
| Repo `docs/wasm-support-policy.md` | What ships today vs deferred |
| Repo `docs/wasm-extension-interfaces.md` | TCP / WebSocket / app-server plan |
| Repo `auto-web/abi/host-contract.json` | Host↔wasm JSON contract + `extension_interfaces` |

## Tests

- `cargo test -p codex-core wasm_bridge --lib`
- `cargo test -p codex-wasm-bridge` (includes `extension_ids_sync_tests`)
