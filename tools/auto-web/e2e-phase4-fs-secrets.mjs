#!/usr/bin/env node
/**
 * Phase 4 E2E: MEMFS + in-memory secrets + sandbox stub via real wasm and
 * production `host-phase4-capabilities.js` (same as codex-cli wasm path).
 */

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
const wasmOutDir = path.join(__dirname, "wasm-out");

function loadWasmBridge() {
  const require = createRequire(import.meta.url);
  return require(path.join(wasmOutDir, "codex_wasm_bridge.js"));
}

function missingCapabilityEnvelope(cid, cap) {
  return {
    correlation_id: cid,
    error: {
      code: "missing_capability",
      message: `unexpected in phase4 e2e: ${cap}`,
      details: null,
    },
  };
}

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

async function main() {
  const wasm = loadWasmBridge();
  const recorded = [];
  const rawDeliver = wasm.codex_deliver_callback.bind(wasm);
  wasm.codex_deliver_callback = (json) => {
    try {
      recorded.push(JSON.parse(json));
    } catch {
      recorded.push({ parse_error: true, json });
    }
    return rawDeliver(json);
  };

  const memFs = createMemFsBackend();
  const secrets = createSecretStore({ envFallback: false });

  const deliverToWasm = (json) => {
    wasm.codex_deliver_callback(json);
  };

  const stub = (cap) => (json) => {
    const p = JSON.parse(json);
    deliverToWasm(
      JSON.stringify(missingCapabilityEnvelope(p.correlation_id, cap)),
    );
  };

  globalThis.host_exec_request = stub("exec");
  globalThis.host_http_request = (json) => {
    const req = JSON.parse(json);
    runHostHttpRequest(req, {
      deliver: (envelope) => deliverToWasm(JSON.stringify(envelope)),
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

  registerPhase4HostCapabilities(globalThis, {
    bridge: () => wasm,
    trace: () => {},
    fs: memFs,
    secrets,
  });

  const initJson = await wasm.codex_init(JSON.stringify({ version: "0.1.0" }));
  const init = JSON.parse(initJson);
  if (!init.ok && init.error) {
    throw new Error(`codex_init failed: ${JSON.stringify(init)}`);
  }

  async function submitAndWait(cid, predicate) {
    recorded.length = 0;
    const sub = await wasm.codex_submit(
      JSON.stringify({
        correlation_id: cid,
        kind: "tool",
        payload: predicate.payload,
      }),
    );
    const ack = JSON.parse(sub);
    if (ack.status !== "accepted") {
      throw new Error(`submit failed: ${sub}`);
    }
    await waitFor(() => recorded.some(predicate.done));
  }

  // --- fs_write + fs_read ---
  const cidW = randomUUID();
  await submitAndWait(cidW, {
    payload: {
      fs_write: {
        path: "/mem/hello.txt",
        data: "phase4-memfs",
        encoding: "utf-8",
        create_dirs: true,
      },
    },
    done: (e) => e.correlation_id === cidW && e.payload?.bytes_written > 0,
  });
  console.log("ok: fs_write (mem)");

  const cidR = randomUUID();
  await submitAndWait(cidR, {
    payload: {
      fs_read: { path: "/mem/hello.txt", encoding: "utf-8" },
    },
    done: (e) =>
      e.correlation_id === cidR && e.payload?.data === "phase4-memfs",
  });
  console.log("ok: fs_read (mem)");

  // --- fs_list ---
  const cidL = randomUUID();
  await submitAndWait(cidL, {
    payload: { fs_list: { path: "/mem" } },
    done: (e) =>
      e.correlation_id === cidL &&
      e.payload?.entries?.some((x) => x.name === "hello.txt"),
  });
  console.log("ok: fs_list (mem)");

  // --- fs_stat exists / missing ---
  const cidS = randomUUID();
  await submitAndWait(cidS, {
    payload: { fs_stat: { path: "/mem/hello.txt" } },
    done: (e) =>
      e.correlation_id === cidS &&
      e.payload?.exists === true &&
      e.payload?.kind === "file",
  });
  console.log("ok: fs_stat exists");

  const cidSm = randomUUID();
  await submitAndWait(cidSm, {
    payload: { fs_stat: { path: "/mem/nope" } },
    done: (e) => e.correlation_id === cidSm && e.payload?.exists === false,
  });
  console.log("ok: fs_stat missing");

  // --- secrets ---
  const cidSec = randomUUID();
  await submitAndWait(cidSec, {
    payload: { secret_set: { key: "E2E_K", value: "E2E_V" } },
    done: (e) => e.correlation_id === cidSec && e.payload?.stored === true,
  });
  const cidSg = randomUUID();
  await submitAndWait(cidSg, {
    payload: { secret_get: { key: "E2E_K" } },
    done: (e) =>
      e.correlation_id === cidSg &&
      e.payload?.found === true &&
      e.payload?.value === "E2E_V",
  });
  console.log("ok: secret_set + secret_get (isolated store)");

  // --- sandbox stub ---
  const cidSb = randomUUID();
  await submitAndWait(cidSb, {
    payload: { sandbox_apply: { policy: "default" } },
    done: (e) =>
      e.correlation_id === cidSb &&
      e.payload?.stub === true &&
      e.payload?.applied === false,
  });
  console.log("ok: sandbox_apply stub");

  // --- fs_remove ---
  const cidRm = randomUUID();
  await submitAndWait(cidRm, {
    payload: { fs_remove: { path: "/mem/hello.txt", recursive: false } },
    done: (e) => e.correlation_id === cidRm && e.payload?.removed === true,
  });
  const cidAfter = randomUUID();
  await submitAndWait(cidAfter, {
    payload: { fs_stat: { path: "/mem/hello.txt" } },
    done: (e) => e.correlation_id === cidAfter && e.payload?.exists === false,
  });
  console.log("ok: fs_remove");

  await wasm.codex_shutdown();
  console.log("\nPhase 4 FS/secrets/sandbox E2E: all checks passed.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
