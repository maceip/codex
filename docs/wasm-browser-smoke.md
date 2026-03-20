# Browser wasm smoke (`codex-wasm-bridge`)

## Required browser bundle (authoritative)

Automated tests use **[Puppeteer](https://pptr.dev/)** to drive **Google Chrome for Testing** (Chromium-based) downloaded by **`@puppeteer/browsers`** — **not** your system Chrome unless you override the path explicitly.

| Item                  | Value                                                                                                                                                                                                                                |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Automation driver** | `puppeteer` (devDependency on the repo root `package.json`)                                                                                                                                                                          |
| **Browser binary**    | Chrome for Testing installed with **`pnpm exec puppeteer browsers install chrome@stable`**                                                                                                                                           |
| **Why `@stable`?**    | The `chrome` build pinned to a specific patch inside `puppeteer` often hits partial/corrupt ZIPs on flaky networks; `chrome@stable` uses the same CfT infrastructure and is what `tools/auto-web/e2e-browser-puppeteer.mjs` invokes. |
| **Override**          | Set **`CODEX_PUPPETEER_EXECUTABLE`** to a Chrome/Chromium executable if you intentionally use a local build.                                                                                                                         |

The E2E runs **`node tools/auto-web/build-wasm-bundles.mjs`** first (no optional assets): **`cargo build` wasm32** + **`wasm-bindgen`** to **`tools/auto-web/wasm-out`** (Node) and **`tools/auto-web/wasm-browser`** (**`no-modules`** glue + **`index.html`**). Generated **`wasm-browser/`** is gitignored.

## Run locally

```bash
pnpm install   # allows puppeteer postinstall via package.json → pnpm.onlyBuiltDependencies
pnpm run build:wasm-bundles
pnpm run e2e:wasm-browser-puppeteer
```

Puppeteer launches headless Chrome with `--no-sandbox` for typical Linux CI/containers.

## Web bindgen target

Browser loads **`wasm-bindgen` `no-modules`** output so host imports (`host_exec_request`, …) resolve as **global** bindings before **`codex_wasm_bridge.js`** runs (see `tools/auto-web/browser-harness.html`).

## Contract

Host callback shapes match [`auto-web/abi/host-contract.json`](../auto-web/abi/host-contract.json).
