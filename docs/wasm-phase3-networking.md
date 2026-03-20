# Phase 3: WASM networking and async (Cursor + Cloud)

This document closes **Phase 3** from [`maceip-wasm-roadmap.md`](maceip-wasm-roadmap.md): move HTTP and streaming off native Rust transports onto the **host bridge**, keep the **wasm module graph** free of `tokio` I/O reactors, and record what full `codex-core` parity still needs later.

## Runtime model (wasm-safe async)

- **`codex-wasm-bridge` exports are `async` in the wasm-bindgen sense** but the Rust side does not depend on `tokio::net`, `tokio::spawn`, or `reqwest` today. Scheduling is **single-threaded** and **host-driven**: the host runs `fetch`, streams body bytes, and calls `codex_deliver_callback` with results.
- **Full `codex-core` on `wasm32`** is still out of scope for this milestone: core pulls `reqwest`, `tokio` process/net features, and other native-only stacks. The compile gate for “no accidental `tokio::net` in wasm” is:  
  `cargo check --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm`  
  When core is eventually wasm-targeted, add a matching check for `codex-core` with a dedicated `wasm` feature set.

## Inventory: where native HTTP / streaming lives (Cursor)

Rough seam map for later `#[cfg(not(target_arch = "wasm32"))]` / transport injection:

| Area | Crate / module | Mechanism |
|------|------------------|-----------|
| Default HTTP client | `codex-core` `default_client` | `reqwest::Client` |
| API / OAuth / downloads | `codex-core` `client`, `auth`, plugins | `reqwest` |
| Streaming / SSE helpers | `codex-core` (e.g. `stream_events_utils`, `eventsource-stream` consumers) | `reqwest` + stream types |
| Responses / backend | `codex-api`, `codex-client` | `reqwest` |
| App server IDE RPC | `codex-app-server` | Axum / sockets (native) |
| Realtime WebSocket experiments | flags in `features`, `model_provider_info` | native paths only today |

**Parity targets for a future wasm core build:**

- **Required early:** HTTPS GET/POST (buffered), timeouts, clear transport errors → map to `host_http_request` + `http_response` or `http_stream_chunk`.
- **Required for Responses / SSE-style APIs:** **chunked / streamed response** via `http_stream_chunk` (`done` marks termination).
- **Defer or host-special-case:** long-lived **WebSockets** (browser `WebSocket` vs Node adapter), low-level **TCP** (`tokio::net`), **Axum** servers.

Contract details live in [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json).

## Bridge types (Rust)

In `codex-core::wasm_bridge`:

- `WasmBridgeHttpRequest` — `url`, optional `method`, `headers`, `body`, `timeout_ms`, **`stream_response`**.
- `WasmBridgeHttpResponse` — buffered reply (`status`, `headers`, `body`).
- `WasmBridgeHttpStreamChunk` — `chunk` + **`done`** for streaming termination.

Submit shape remains: `payload.http` → capability `host_http_request` (correlation id merged by the bridge when invoking the host).

## Cloud: Node loader behavior

`codex-cli/bin/codex.js`:

- **`stream_response: false` / omitted:** one callback with `kind: http_response` after `resp.text()`.
- **`stream_response: true`:** read `Response.body` with `getReader()`, decode UTF-8 with `TextDecoder` (`stream: true`), emit **`http_stream_chunk`** per read; the final read uses `done: true`. Empty or missing body uses a single terminal chunk.
- **Errors:** `TimeoutError` → `timeout`; `AbortError` → `cancelled`; else `internal` (matches existing behavior for buffered requests).

## Validation (run locally)

```bash
# Wasm graph (no core)
cd codex-rs && cargo check --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm

# Bridge + contract tests (native test build links core)
cd codex-rs && cargo test -p codex-wasm-bridge

# Core wasm_bridge unit tests (needs linux-sandbox build inputs on Linux)
cd codex-rs && cargo test -p codex-core wasm_bridge --lib

# Host harness (real wasm + glue)
node tools/auto-web/host-harness.js --wasm

# E2E: local HTTP server + production host fetch + real wasm (`tools/auto-web/wasm-out`)
node tools/auto-web/e2e-phase3-network.mjs

# Later phases (same `wasm-out` layout): MEMFS/secrets (`e2e-phase4-fs-secrets.mjs`), full lifecycle (`e2e-phase5-lifecycle.mjs`), export drift (`verify-wasm-exports.mjs`).

# Browser (Puppeteer + Chrome-for-Testing `chrome@stable` — see [`wasm-browser-smoke.md`](wasm-browser-smoke.md)):
pnpm run e2e:wasm-browser-puppeteer
```


## Goal 2 (auto-web)

HTTP request/response and stream chunk shapes are JSON-stable so harnesses can **record and replay** the same payloads for regression tooling.
