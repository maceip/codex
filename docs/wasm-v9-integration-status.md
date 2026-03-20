# maceip / v9 integration status (tracked proof)

**There is no CI job in _this_ repository that boots [maceip/v9](https://github.com/maceip/v9) and proves the bridge runs there.** That integration is a **cross-repo cliff**: this tree owns the **contract** (`auto-web/abi/host-contract.json`, Rust `extension_ids`, Node loaders); v9 owns the EdgeJS runtime and broker wiring.

| Proof line                                          | Status                                                                                      |
| --------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| Wasm artifact builds (`codex-wasm-bridge`)          | Yes — see `wasm-bridge-ci` and `rust-ci` wasm guard.                                        |
| Host imports defined + kernel routes extension keys | Yes — see harnesses, `verify-host-contract.mjs`, `extension-host-stubs.mjs`.                |
| v9 (EdgeJS) runtime E2E                             | **Not automated here** — checklist: [`wasm-maceip-v9-bridge.md`](wasm-maceip-v9-bridge.md). |

When v9 can consume the bridge, expect either a submodule job, a scheduled cross-repo workflow, or a pinned artifact download step that runs `pnpm run e2e:wasm-milestone` (or a subset) inside v9’s harness.
