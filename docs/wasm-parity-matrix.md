# WASM parity matrix (expectations vs reality)

This fork ships **`codex-wasm-bridge`** as a JSON-in / JSON-out **kernel** around host capabilities. “Production confidence” here is **boundary** confidence (imports, correlation IDs, cancellation plumbing), not proof that Codex’s full native reasoning, tool graph, or IDE server runs inside wasm.

| Surface                                         | In wasm today        | Parity notes                                                                                                          |
| ----------------------------------------------- | -------------------- | --------------------------------------------------------------------------------------------------------------------- |
| `codex_submit` / callbacks                      | Yes                  | Thin routing; no full `codex-core` agent loop in the wasm artifact.                                                   |
| `host_exec_request`                             | Yes (host-dependent) | Replaces native spawn; behavior depends on the host broker.                                                           |
| `host_http_request`                             | Yes                  | Fetch-backed in reference hosts; streaming supported.                                                                 |
| Phase 4 FS / secrets / sandbox stubs            | Yes                  | MEMFS + secret store + sandbox acknowledge stub in harnesses / CLI.                                                   |
| Structured keys `ws` / `tcp` / `app_server_rpc` | Kernel → import      | Routed to `host_websocket_request` / `host_tcp_socket` / `host_app_server_rpc`. Host may return `missing_capability`. |
| `codex-app-server` crate                        | No (Milestone A)     | RPC shape can be forwarded via `app_server_rpc` **envelope** only.                                                    |
| Native Linux sandbox / PTY / TUI                | No                   | Intentionally out of wasm path; see policy doc.                                                                       |

CI covers wasm32 compile, Rust tests, `host-contract.json` / golden JSON checks, Node harnesses, and Puppeteer browser smoke — not full product parity with native Codex.
