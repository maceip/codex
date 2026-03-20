#!/usr/bin/env node
/**
 * Phase 5: one Node smoke — full request lifecycle using production host modules
 * (MEMFS + secrets + real fetch + synthetic exec) without spawning codex native.
 */

import http from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { runHostHttpRequest } from "../../codex-cli/bin/host-http-fetch.js";
import {
  createMemFsBackend,
  createSecretStore,
  registerPhase4HostCapabilities,
} from "../../codex-cli/bin/host-phase4-capabilities.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

async function waitFor(pred, ms = 20000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (pred()) {
      return;
    }
    await new Promise((r) => setTimeout(r, 15));
  }
  throw new Error(`timeout after ${ms}ms`);
}

function startServer() {
  const server = http.createServer((req, res) => {
    if (req.method === "GET" && req.url === "/ping") {
      res.writeHead(200, { "Content-Type": "text/plain" });
      res.end("lifecycle-ping");
      return;
    }
    res.writeHead(404);
    res.end();
  });
  return new Promise((resolve, reject) => {
    server.listen(0, "127.0.0.1", () => {
      const a = server.address();
      if (!a || typeof a === "string") {
        reject(new Error("address"));
        return;
      }
      resolve({ server, port: a.port });
    });
    server.on("error", reject);
  });
}

async function main() {
  const wasmOut = path.join(__dirname, "wasm-out");
  const require = createRequire(import.meta.url);
  const wasm = require(path.join(wasmOut, "codex_wasm_bridge.js"));
  const recorded = [];
  const raw = wasm.codex_deliver_callback.bind(wasm);
  wasm.codex_deliver_callback = (json) => {
    try {
      recorded.push(JSON.parse(json));
    } catch {
      recorded.push({});
    }
    return raw(json);
  };

  const memFs = createMemFsBackend();
  const secrets = createSecretStore({ envFallback: false });

  globalThis.host_exec_request = (json) => {
    const p = JSON.parse(json);
    const cid = p.correlation_id;
    const deliver = (obj) => wasm.codex_deliver_callback(JSON.stringify(obj));
    deliver({
      correlation_id: cid,
      capability: "exec",
      payload: {
        kind: "exec_chunk",
        correlation_id: cid,
        stream: "stdout",
        data: `stub: ${p.command}`,
      },
    });
    deliver({
      correlation_id: cid,
      capability: "exec",
      payload: {
        kind: "exec_exit",
        correlation_id: cid,
        exit_code: 0,
        signal: null,
        cancelled: false,
      },
    });
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

  registerPhase4HostCapabilities(globalThis, {
    bridge: () => wasm,
    trace: () => {},
    fs: memFs,
    secrets,
  });

  const init = JSON.parse(
    await wasm.codex_init(JSON.stringify({ version: "0.1.0" })),
  );
  if (init.error) {
    throw new Error(`init: ${JSON.stringify(init)}`);
  }

  const { server, port } = await startServer();
  try {
    const cidFs = randomUUID();
    recorded.length = 0;
    let sub = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cidFs,
        kind: "tool",
        payload: {
          fs_write: {
            path: "/lifecycle/t.txt",
            data: "x",
            encoding: "utf-8",
            create_dirs: true,
          },
        },
      }),
    );
    if (JSON.parse(sub).status !== "accepted") {
      throw new Error(sub);
    }
    await waitFor(() =>
      recorded.some(
        (e) => e.correlation_id === cidFs && e.payload?.bytes_written,
      ),
    );

    const cidNet = randomUUID();
    recorded.length = 0;
    sub = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cidNet,
        kind: "tool",
        payload: {
          http: { url: `http://127.0.0.1:${port}/ping`, method: "GET" },
        },
      }),
    );
    if (JSON.parse(sub).status !== "accepted") {
      throw new Error(sub);
    }
    await waitFor(() =>
      recorded.some(
        (e) =>
          e.correlation_id === cidNet &&
          e.payload?.kind === "http_response" &&
          String(e.payload.body).includes("lifecycle-ping"),
      ),
    );

    const cidEx = randomUUID();
    recorded.length = 0;
    sub = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cidEx,
        kind: "host_call",
        payload: {
          host_call: {
            capability: "host_exec_request",
            payload: { command: "true", args: [] },
          },
        },
      }),
    );
    if (JSON.parse(sub).status !== "accepted") {
      throw new Error(sub);
    }
    await waitFor(() =>
      recorded.some(
        (e) => e.correlation_id === cidEx && e.payload?.kind === "exec_exit",
      ),
    );

    const cidSec = randomUUID();
    recorded.length = 0;
    sub = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cidSec,
        kind: "tool",
        payload: { secret_set: { key: "L5", value: "v" } },
      }),
    );
    if (JSON.parse(sub).status !== "accepted") {
      throw new Error(sub);
    }
    await waitFor(() =>
      recorded.some((e) => e.correlation_id === cidSec && e.payload?.stored),
    );

    await wasm.codex_shutdown();
    console.log(
      "Phase 5 lifecycle E2E: fs + http + exec + secret + shutdown ok.",
    );
  } finally {
    server.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
