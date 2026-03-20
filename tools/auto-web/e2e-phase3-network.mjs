#!/usr/bin/env node
/**
 * Phase 3 E2E: real Node `fetch` against a local HTTP server, wasm bridge, and
 * production `host-http-fetch` (same module as codex-cli --wasm).
 */

import http from "node:http";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";
import { runHostHttpRequest } from "../../codex-cli/bin/host-http-fetch.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const wasmOutDir = path.join(__dirname, "wasm-out");

function loadWasmBridge() {
  const require = createRequire(import.meta.url);
  const glue = path.join(wasmOutDir, "codex_wasm_bridge.js");
  return require(glue);
}

function missingCapabilityEnvelope(cid, cap) {
  return {
    correlation_id: cid,
    error: {
      code: "missing_capability",
      message: `unexpected host call in http e2e: ${cap}`,
      details: null,
    },
  };
}

/** @param {(s: string) => void} deliver */
function installNonHttpStubs(wasm, deliver) {
  const stub = (cap) => (json) => {
    const p = JSON.parse(json);
    const cid = p.correlation_id;
    deliver(JSON.stringify(missingCapabilityEnvelope(cid, cap)));
  };
  globalThis.host_exec_request = stub("host_exec_request");
  globalThis.host_fs_read = stub("host_fs_read");
  globalThis.host_fs_write = stub("host_fs_write");
  globalThis.host_fs_list = stub("host_fs_list");
  globalThis.host_fs_stat = stub("host_fs_stat");
  globalThis.host_fs_remove = stub("host_fs_remove");
  globalThis.host_secret_get = stub("host_secret_get");
  globalThis.host_secret_set = stub("host_secret_set");
  globalThis.host_sandbox_apply = stub("host_sandbox_apply");
}

async function waitFor(pred, ms = 15000) {
  const t0 = Date.now();
  while (Date.now() - t0 < ms) {
    if (pred()) return;
    await new Promise((r) => setTimeout(r, 15));
  }
  throw new Error(`timeout after ${ms}ms`);
}

function startBackend() {
  const server = http.createServer((req, res) => {
    const host = req.headers.host || "127.0.0.1";
    const u = new URL(req.url || "/", `http://${host}`);

    if (req.method === "GET" && u.pathname === "/buffer") {
      res.writeHead(200, { "Content-Type": "text/plain" });
      res.end("e2e-buffered-ok");
      return;
    }

    if (req.method === "GET" && u.pathname === "/stream") {
      res.writeHead(200, {
        "Content-Type": "text/plain",
        "Transfer-Encoding": "chunked",
      });
      res.write("alpha");
      setTimeout(() => {
        res.write("beta");
        res.end();
      }, 30);
      return;
    }

    if (req.method === "POST" && u.pathname === "/echo") {
      const chunks = [];
      req.on("data", (c) => chunks.push(c));
      req.on("end", () => {
        const body = Buffer.concat(chunks).toString("utf8");
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(body);
      });
      return;
    }

    res.writeHead(404);
    res.end();
  });

  return new Promise((resolve, reject) => {
    server.listen(0, "127.0.0.1", () => {
      const addr = server.address();
      if (!addr || typeof addr === "string") {
        reject(new Error("no server address"));
        return;
      }
      resolve({ server, port: addr.port });
    });
    server.on("error", reject);
  });
}

async function main() {
  const wasm = loadWasmBridge();
  const recorded = [];

  const deliverToWasm = (json) => {
    wasm.codex_deliver_callback(json);
  };

  installNonHttpStubs(wasm, deliverToWasm);

  globalThis.host_http_request = (json) => {
    const req = JSON.parse(json);
    runHostHttpRequest(req, {
      deliver: (envelope) => {
        recorded.push(envelope);
        deliverToWasm(JSON.stringify(envelope));
      },
    }).catch((err) => {
      deliverToWasm(
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

  const initJson = await wasm.codex_init(JSON.stringify({ version: "0.1.0" }));
  const init = JSON.parse(initJson);
  if (!init.ok && init.error) {
    throw new Error(`codex_init failed: ${JSON.stringify(init)}`);
  }

  const { server, port } = await startBackend();
  const base = `http://127.0.0.1:${port}`;

  try {
    // --- Buffered GET ---
    const cid1 = randomUUID();
    recorded.length = 0;
    const sub1 = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cid1,
        kind: "tool",
        payload: {
          http: { url: `${base}/buffer`, method: "GET" },
        },
      }),
    );
    const ack1 = JSON.parse(sub1);
    if (ack1.status !== "accepted") {
      throw new Error(`buffered submit: ${sub1}`);
    }
    await waitFor(() =>
      recorded.some(
        (e) => e.correlation_id === cid1 && e.payload?.kind === "http_response",
      ),
    );
    const buf = recorded.find(
      (e) => e.correlation_id === cid1 && e.payload?.kind === "http_response",
    );
    if (
      buf.payload.status !== 200 ||
      !String(buf.payload.body).includes("e2e-buffered-ok")
    ) {
      throw new Error(`unexpected buffered response: ${JSON.stringify(buf)}`);
    }
    console.log("ok: buffered http_response");

    // --- Streamed GET ---
    const cid2 = randomUUID();
    recorded.length = 0;
    const sub2 = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cid2,
        kind: "tool",
        payload: {
          http: {
            url: `${base}/stream`,
            method: "GET",
            stream_response: true,
          },
        },
      }),
    );
    const ack2 = JSON.parse(sub2);
    if (ack2.status !== "accepted") {
      throw new Error(`stream submit: ${sub2}`);
    }
    await waitFor(() =>
      recorded.some(
        (e) =>
          e.correlation_id === cid2 &&
          e.payload?.kind === "http_stream_chunk" &&
          e.payload.done === true,
      ),
    );
    const streamChunks = recorded.filter(
      (e) =>
        e.correlation_id === cid2 && e.payload?.kind === "http_stream_chunk",
    );
    const assembled = streamChunks.map((e) => e.payload.chunk).join("");
    if (assembled !== "alphabeta") {
      throw new Error(
        `stream assemble got ${JSON.stringify(assembled)} chunks=${JSON.stringify(streamChunks)}`,
      );
    }
    console.log("ok: streamed http_stream_chunk → alphabeta");

    // --- POST body echo ---
    const cid3 = randomUUID();
    const postBody = JSON.stringify({ phase3: "e2e-post", n: 42 });
    recorded.length = 0;
    const sub3 = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cid3,
        kind: "tool",
        payload: {
          http: {
            url: `${base}/echo`,
            method: "POST",
            headers: { "content-type": "application/json" },
            body: postBody,
          },
        },
      }),
    );
    const ack3 = JSON.parse(sub3);
    if (ack3.status !== "accepted") {
      throw new Error(`post submit: ${sub3}`);
    }
    await waitFor(() =>
      recorded.some(
        (e) => e.correlation_id === cid3 && e.payload?.kind === "http_response",
      ),
    );
    const echo = recorded.find(
      (e) => e.correlation_id === cid3 && e.payload?.kind === "http_response",
    );
    if (echo.payload.body !== postBody) {
      throw new Error(`post echo mismatch: ${JSON.stringify(echo.payload)}`);
    }
    console.log("ok: POST echo body");

    await wasm.codex_shutdown();
    console.log("\nPhase 3 network E2E: all checks passed.");
  } finally {
    server.close();
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
