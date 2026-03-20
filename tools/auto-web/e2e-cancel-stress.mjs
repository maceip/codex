#!/usr/bin/env node
/**
 * Best-effort cancel stress: many in-flight slow HTTP submits + codex_cancel under wasm.
 * Asserts clean shutdown (no process throw) — not a latency benchmark.
 */

import http from "node:http";
import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { runHostHttpRequest } from "../../codex-cli/bin/host-http-fetch.js";
import {
  createMemFsBackend,
  createSecretStore,
  registerPhase4HostCapabilities,
} from "../../codex-cli/bin/host-phase4-capabilities.js";
import { installSyntheticExtensionHosts } from "./extension-host-stubs.mjs";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmOut = path.join(__dirname, "wasm-out");

async function main() {
  const require = createRequire(import.meta.url);
  const wasm = require(path.join(wasmOut, "codex_wasm_bridge.js"));

  const server = http.createServer((req, res) => {
    if (req.method === "GET" && req.url === "/slow") {
      setTimeout(() => {
        res.writeHead(200, { "Content-Type": "text/plain" });
        res.end("slow-ok");
      }, 250);
      return;
    }
    res.writeHead(404);
    res.end();
  });
  await new Promise((resolve, reject) => {
    server.listen(0, "127.0.0.1", (err) => (err ? reject(err) : resolve()));
  });
  const port = server.address().port;
  const base = `http://127.0.0.1:${port}`;

  globalThis.host_exec_request = (json) => {
    const p = JSON.parse(json);
    const cid = p.correlation_id;
    wasm.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        capability: "exec",
        payload: {
          kind: "exec_exit",
          correlation_id: cid,
          exit_code: 0,
          signal: null,
          cancelled: false,
        },
      }),
    );
  };

  globalThis.host_http_request = (json) => {
    const req = JSON.parse(json);
    runHostHttpRequest(req, {
      deliver: (env) => wasm.codex_deliver_callback(JSON.stringify(env)),
    }).catch((err) => {
      wasm.codex_deliver_callback(
        JSON.stringify({
          correlation_id: req.correlation_id,
          error: {
            code: "internal",
            message: err?.message || String(err),
            details: null,
          },
        }),
      );
    });
  };

  installSyntheticExtensionHosts((j) => wasm.codex_deliver_callback(j));

  registerPhase4HostCapabilities(globalThis, {
    bridge: () => wasm,
    trace: () => {},
    fs: createMemFsBackend(),
    secrets: createSecretStore({ envFallback: false }),
  });

  const init = JSON.parse(
    await wasm.codex_init(JSON.stringify({ version: "0.1.0" })),
  );
  if (init.error) {
    throw new Error(JSON.stringify(init));
  }

  const slowCid = randomUUID();
  await wasm.codex_submit(
    JSON.stringify({
      correlation_id: slowCid,
      kind: "tool",
      payload: { http: { url: `${base}/slow`, method: "GET" } },
    }),
  );
  await new Promise((r) => setTimeout(r, 10));
  wasm.codex_cancel(
    JSON.stringify({ correlation_id: slowCid, reason: "stress" }),
  );

  const n = 16;
  const ids = [];
  for (let i = 0; i < n; i++) {
    const cid = randomUUID();
    ids.push(cid);
    await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cid,
        kind: "tool",
        payload: { http: { url: `${base}/slow`, method: "GET" } },
      }),
    );
  }
  for (const cid of ids) {
    wasm.codex_cancel(
      JSON.stringify({ correlation_id: cid, reason: "stress" }),
    );
  }

  await wasm.codex_shutdown();
  server.close();
  console.log("e2e-cancel-stress: ok");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
