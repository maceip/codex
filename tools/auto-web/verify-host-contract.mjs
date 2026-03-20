#!/usr/bin/env node
/**
 * Lightweight ABI drift check: host-contract.json shape + wasm export names
 * match what loaders expect (complements Rust tests).
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const contractPath = path.resolve(
  __dirname,
  "../../auto-web/abi/host-contract.json",
);

const REQUIRED_EXPORTS = [
  "codex_init",
  "codex_submit",
  "codex_deliver_callback",
  "codex_cancel",
  "codex_shutdown",
];

const raw = readFileSync(contractPath, "utf8");
let contract;
try {
  contract = JSON.parse(raw);
} catch (e) {
  console.error("host-contract.json: invalid JSON", e);
  process.exit(1);
}

if (!contract.wasm_exports?.methods) {
  console.error("host-contract.json: missing wasm_exports.methods");
  process.exit(1);
}

for (const name of REQUIRED_EXPORTS) {
  if (!contract.wasm_exports.methods[name]) {
    console.error(`host-contract.json: wasm_exports.methods missing ${name}`);
    process.exit(1);
  }
}

if (!contract.host_imports) {
  console.error("host-contract.json: missing host_imports");
  process.exit(1);
}

if (!contract.extension_interfaces?.structured_submit_keys) {
  console.error(
    "host-contract.json: missing extension_interfaces.structured_submit_keys",
  );
  process.exit(1);
}

const ext = contract.extension_interfaces.structured_submit_keys;
for (const k of ["ws", "tcp", "app_server_rpc"]) {
  if (!ext[k]?.planned_capability) {
    console.error(
      `host-contract.json: extension_interfaces missing ${k}.planned_capability`,
    );
    process.exit(1);
  }
}

const him = contract.extension_interfaces?.host_import_methods ?? {};
for (const k of [
  "host_websocket_request",
  "host_tcp_socket",
  "host_app_server_rpc",
]) {
  if (typeof him[k] !== "string" || !him[k].length) {
    console.error(
      `host-contract.json: extension_interfaces.host_import_methods missing or empty string for ${k}`,
    );
    process.exit(1);
  }
}

console.log(
  "verify-host-contract: ok (exports + extension_interfaces present)",
);
