# MaceIP / v9 (EdgeJS) integration checklist

**Goal (from `WORK.md`):** run Codex wasm inside the **[maceip/v9](https://github.com/maceip/v9)**
EdgeJS browser runtime with minimal fork surface.

**Status:** No in-repo CI proves this bridge inside v9 yet — see [`wasm-v9-integration-status.md`](wasm-v9-integration-status.md).

## What this repo provides today

- **`codex-wasm-bridge`** + **`wasm-bindgen`** glue (`nodejs` + **`no-modules`** for classic global hosts).
- Reference hosts: **Node** (`codex-cli --wasm`), **browser** (`tools/auto-web/e2e-browser-puppeteer.mjs`).
- **`host-contract.json`** + extension plan for TCP / WebSocket / app-server forwarding.

## v9-specific work (not automated here yet)

1. **Glue target:** confirm v9’s JS engine can load **`no-modules`** output or bundle ES modules with global `host_*` shims (same pattern as `browser-harness.html`).
2. **Network:** map `host_http_request` to v9’s fetch / WebTransport path (avoid deprecated proxies per Friscy rules in your environment).
3. **Filesystem:** map `host_fs_*` to v9 MEMFS or proxied FS (`napi-bridge/memfs.js` patterns).
4. **Exec:** map `host_exec_request` to v9’s command broker (no raw `spawn` from wasm).
5. **CI:** add a cross-repo or submodule job that builds this bridge and runs **`e2e:wasm-milestone`** (or a subset) inside v9’s test harness when artifacts are available.

## Reference clone

```bash
git clone https://github.com/maceip/v9.git
```

Track upstream EdgeJS + `napi-bridge` APIs separately; this Codex repo stays the **contract** owner.
