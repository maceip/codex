#!/usr/bin/env node
/**
 * Always produces Node + browser (no-modules) wasm-bindgen outputs from the
 * same `codex_wasm_bridge.wasm` artifact — no optional assets.
 */

import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const toolsAutoWeb = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(toolsAutoWeb, "..", "..");
const codexRs = path.join(repoRoot, "codex-rs");
const wasmPath = path.join(
  codexRs,
  "target/wasm32-unknown-unknown/debug/codex_wasm_bridge.wasm",
);
const outNode = path.join(toolsAutoWeb, "wasm-out");
const outBrowser = path.join(toolsAutoWeb, "wasm-browser");
const harnessSrc = path.join(toolsAutoWeb, "browser-harness.html");

execSync(
  "cargo build --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm",
  { cwd: codexRs, stdio: "inherit" },
);

mkdirSync(outNode, { recursive: true });
mkdirSync(outBrowser, { recursive: true });

execSync(`wasm-bindgen "${wasmPath}" --target nodejs --out-dir "${outNode}"`, {
  cwd: repoRoot,
  stdio: "inherit",
});
execSync(
  `wasm-bindgen "${wasmPath}" --target no-modules --out-dir "${outBrowser}"`,
  { cwd: repoRoot, stdio: "inherit" },
);

copyFileSync(harnessSrc, path.join(outBrowser, "index.html"));

console.log(
  "build-wasm-bundles: wasm-out (nodejs) + wasm-browser (no-modules + index.html) ready.",
);
