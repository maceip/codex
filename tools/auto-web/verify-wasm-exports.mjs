#!/usr/bin/env node
/**
 * Phase 6 ABI guard: ensure generated glue exports the wasm-bindgen surface
 * expected by loaders (drift fails CI early).
 */

import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const gluePath = path.join(__dirname, "wasm-out", "codex_wasm_bridge.js");

const REQUIRED = [
  "codex_init",
  "codex_submit",
  "codex_deliver_callback",
  "codex_cancel",
  "codex_shutdown",
];

const require = createRequire(import.meta.url);
const glue = require(gluePath);
const missing = REQUIRED.filter((n) => typeof glue[n] !== "function");
if (missing.length) {
  console.error(
    "Missing or non-function wasm glue exports:",
    missing.join(", "),
  );
  process.exit(1);
}
console.log(`verify-wasm-exports: ok (${REQUIRED.join(", ")})`);
