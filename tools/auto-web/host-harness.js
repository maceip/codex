#!/usr/bin/env node
// tools/auto-web/host-harness.js v0.6.0
// Phase 3: streaming HTTP stub + dispatch coverage for http_stream_chunk path.

import { readFile } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { execFile } from "node:child_process";
import path from "node:path";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const REQUIRED_EXPORTS = ["codex_init", "codex_submit", "codex_deliver_callback", "codex_cancel", "codex_shutdown"];

function missingCapability(cid, cap) { return { correlation_id: cid, error: { code: "missing_capability", message: `Host does not provide: ${cap}`, details: null } }; }

// --- Stub host capabilities ---
class StubHostCapabilities {
  constructor() { this.log = []; this._wasmBridge = null; }
  _cb(cid, cap, payload) { this._wasmBridge?.codex_deliver_callback(JSON.stringify({ correlation_id: cid, capability: cap, payload })); }
  handleExecRequest(p) { this.log.push({ capability: "exec", payload: p }); const cid = p.correlation_id; const cbs = [{ kind: "exec_chunk", correlation_id: cid, stream: "stdout", data: `stub: ${p.command}` }, { kind: "exec_exit", correlation_id: cid, exit_code: 0, signal: null, cancelled: false }]; cbs.forEach(cb => this._cb(cid, "exec", cb)); return cbs; }
  handleHttpRequest(p) {
    this.log.push({ capability: "network", payload: p });
    const cid = p.correlation_id;
    if (p.stream_response) {
      this._cb(cid, "network", { kind: "http_stream_chunk", correlation_id: cid, chunk: "a", done: false });
      this._cb(cid, "network", { kind: "http_stream_chunk", correlation_id: cid, chunk: "b", done: true });
      return;
    }
    this._cb(cid, "network", { kind: "http_response", correlation_id: cid, status: 200, headers: {}, body: '{"stub":true}' });
  }
  handleFsRead(p) { this.log.push({ capability: "fs_read", payload: p }); this._cb(p.correlation_id, "fs_read", { correlation_id: p.correlation_id, data: "stub", encoding: "utf-8" }); }
  handleFsWrite(p) { this.log.push({ capability: "fs_write", payload: p }); this._cb(p.correlation_id, "fs_write", { correlation_id: p.correlation_id, bytes_written: 4 }); }
  handleFsList(p) { this.log.push({ capability: "fs_list", payload: p }); this._cb(p.correlation_id, "fs_list", { correlation_id: p.correlation_id, entries: [{ name: "stub.txt", kind: "file", size: 4 }] }); }
  handleFsStat(p) { this.log.push({ capability: "fs_stat", payload: p }); this._cb(p.correlation_id, "fs_stat", { correlation_id: p.correlation_id, exists: true, kind: "file", size: 4, modified_ms: Date.now() }); }
  handleFsRemove(p) { this.log.push({ capability: "fs_remove", payload: p }); this._cb(p.correlation_id, "fs_remove", { correlation_id: p.correlation_id, removed: true }); }
  handleSecretGet(p) { this.log.push({ capability: "secret_get", payload: p }); this._cb(p.correlation_id, "secret_get", { correlation_id: p.correlation_id, value: null, found: false }); }
  handleSecretSet(p) { this.log.push({ capability: "secret_set", payload: p }); this._cb(p.correlation_id, "secret_set", { correlation_id: p.correlation_id, stored: true }); }
  handleSandboxApply(p) { this.log.push({ capability: "sandbox", payload: p }); this._cb(p.correlation_id, "sandbox", { correlation_id: p.correlation_id, applied: false, stub: true, reason: "wasm mode" }); }
  installGlobals() {
    const self = this, parse = (j) => { try { return JSON.parse(j); } catch { return {}; } };
    globalThis.host_exec_request = (j) => self.handleExecRequest(parse(j));
    globalThis.host_http_request = (j) => self.handleHttpRequest(parse(j));
    globalThis.host_fs_read = (j) => self.handleFsRead(parse(j));
    globalThis.host_fs_write = (j) => self.handleFsWrite(parse(j));
    globalThis.host_fs_list = (j) => self.handleFsList(parse(j));
    globalThis.host_fs_stat = (j) => self.handleFsStat(parse(j));
    globalThis.host_fs_remove = (j) => self.handleFsRemove(parse(j));
    globalThis.host_secret_get = (j) => self.handleSecretGet(parse(j));
    globalThis.host_secret_set = (j) => self.handleSecretSet(parse(j));
    globalThis.host_secret_delete = (j) => self.handleSecretSet(parse(j));
    globalThis.host_sandbox_apply = (j) => self.handleSandboxApply(parse(j));
  }
}

// --- Mock ---
function createMockWasmInstance() {
  return { exports: {
    codex_init() { return JSON.stringify({ ok: true }); },
    codex_submit(j) { return JSON.stringify({ correlation_id: JSON.parse(j).correlation_id, status: "accepted" }); },
    codex_deliver_callback() {},
    codex_cancel() {},
    async codex_shutdown() {},
  }, _isMock: true };
}

// --- Real wasm loader ---
async function loadRealWasm(dir) {
  const f = path.join(dir, "codex_wasm_bridge.js");
  try { await readFile(f); } catch { throw new Error(`glue not found: ${f}`); }
  const r = createRequire(import.meta.url);
  const w = r(f);
  return { exports: { codex_init: w.codex_init, codex_submit: w.codex_submit, codex_deliver_callback: w.codex_deliver_callback, codex_cancel: w.codex_cancel, codex_shutdown: w.codex_shutdown }, _isMock: false };
}

// --- Helpers ---
async function submitHostCall(exports, result, cap, payload, label) {
  const cid = randomUUID();
  try {
    const r = await exports.codex_submit(JSON.stringify({ correlation_id: cid, kind: "host_call", payload: { host_call: { capability: cap, payload } } }));
    const p = typeof r === "string" ? JSON.parse(r) : r;
    if (p.status === "accepted") result.pass(label, `cid=${cid}`);
    else result.fail(label, JSON.stringify(p));
  } catch (err) { result.fail(label, err.message); }
}

class HarnessResult {
  constructor() { this.checks = []; }
  pass(n, d = null) { this.checks.push({ name: n, passed: true, detail: d }); }
  fail(n, d = null) { this.checks.push({ name: n, passed: false, detail: d }); }
  get allPassed() { return this.checks.every(c => c.passed); }
  report() {
    const t = this.checks.length, p = this.checks.filter(c => c.passed).length, f = t - p;
    console.log("\n=== Host Harness Report ===");
    for (const c of this.checks) console.log(`  [${c.passed ? "PASS" : "FAIL"}] ${c.name}${c.detail ? ` — ${c.detail}` : ""}`);
    console.log(`\nTotal: ${t}  Passed: ${p}  Failed: ${f}`);
    return { total: t, passed: p, failed: f };
  }
}

// --- Loader integration: spawn CLI in trace mode, verify event sequence ---
function runLoaderTraceTest(result) {
  const cliPath = path.resolve(__dirname, "../../codex-cli/bin/codex.js");
  return new Promise((resolve) => {
    execFile("node", [cliPath, "--wasm"], {
      env: { ...process.env, CODEX_WASM: "1", CODEX_WASM_TRACE: "1" },
      timeout: 10000,
    }, (err, stdout, stderr) => {
      const lines = stderr.split("\n").filter(l => l.startsWith("{"));
      const events = [];
      for (const line of lines) {
        try { events.push(JSON.parse(line)); } catch { /* skip */ }
      }
      const eventNames = events.map(e => e.event);

      // Check required events exist
      const required = ["artifacts_found", "glue_loaded", "init_ok", "submit_ok", "no_exec", "shutdown"];
      for (const name of required) {
        if (eventNames.includes(name)) result.pass(`loader_trace:${name}`);
        else result.fail(`loader_trace:${name}`, `missing from trace output`);
      }

      // Check ordering: each required event must come after the previous one
      let lastIdx = -1;
      let orderOk = true;
      for (const name of required) {
        const idx = eventNames.indexOf(name);
        if (idx <= lastIdx && idx !== -1) { orderOk = false; break; }
        if (idx !== -1) lastIdx = idx;
      }
      if (orderOk) result.pass("loader_trace:ordering", required.join("→"));
      else result.fail("loader_trace:ordering", `events out of order: ${eventNames.join(",")}`);

      // Check shutdown diagnostics
      const shutdownEvent = events.find(e => e.event === "shutdown");
      if (shutdownEvent) {
        if (shutdownEvent.exit_code === 0) result.pass("loader_diag:exit_code", "0");
        else result.fail("loader_diag:exit_code", `${shutdownEvent.exit_code}`);

        if (shutdownEvent.reason === "no_exec") result.pass("loader_diag:reason", "no_exec");
        else result.fail("loader_diag:reason", shutdownEvent.reason);

        if (shutdownEvent.exec_seen === false) result.pass("loader_diag:exec_seen", "false");
        else result.fail("loader_diag:exec_seen", `${shutdownEvent.exec_seen}`);

        if (shutdownEvent.callbacks_delivered === 0) result.pass("loader_diag:callbacks", "0");
        else result.fail("loader_diag:callbacks", `${shutdownEvent.callbacks_delivered}`);

        if (Array.isArray(shutdownEvent.active_cids) && shutdownEvent.active_cids.length === 0) result.pass("loader_diag:active_cids", "[]");
        else result.fail("loader_diag:active_cids", JSON.stringify(shutdownEvent.active_cids));
      } else {
        result.fail("loader_diag:shutdown_event", "missing");
      }

      resolve();
    });
  });
}

// --- Main test runner ---
async function runHarness(mode, wasmOutDir) {
  const result = new HarnessResult();
  const capabilities = new StubHostCapabilities();

  let instance;
  try {
    if (mode === "wasm") { capabilities.installGlobals(); instance = await loadRealWasm(wasmOutDir); capabilities._wasmBridge = instance.exports; result.pass("wasm_load", `real wasm`); }
    else { instance = createMockWasmInstance(); result.pass("wasm_load", "mock"); }
  } catch (err) { result.fail("wasm_load", err.message); return result.report(); }

  const exports = instance.exports;

  for (const n of REQUIRED_EXPORTS) { if (typeof exports[n] === "function") result.pass(`export:${n}`); else result.fail(`export:${n}`); }

  try { const r = await exports.codex_init(JSON.stringify({ version: "0.1.0" })); const p = JSON.parse(typeof r === "string" ? r : JSON.stringify(r)); if (p.ok || !p.error) result.pass("init"); else result.fail("init", JSON.stringify(p)); } catch (e) { result.fail("init", e.message); }

  try { const r = await exports.codex_submit(JSON.stringify({ correlation_id: randomUUID(), kind: "prompt", payload: { text: "test" } })); if (JSON.parse(r).status === "accepted") result.pass("submit:plain"); else result.fail("submit:plain"); } catch (e) { result.fail("submit:plain", e.message); }

  // Happy-path dispatches
  await submitHostCall(exports, result, "host_exec_request", { command: "echo", args: ["hello"] }, "dispatch:exec");
  await submitHostCall(exports, result, "host_http_request", { url: "https://example.com", method: "GET" }, "dispatch:network");
  await submitHostCall(exports, result, "host_http_request", { url: "https://example.com/stream", method: "GET", stream_response: true }, "dispatch:http_stream");
  await submitHostCall(exports, result, "host_fs_read", { path: "/tmp/t", encoding: "utf-8" }, "dispatch:fs_read");
  await submitHostCall(exports, result, "host_fs_write", { path: "/tmp/o", data: "hi", encoding: "utf-8" }, "dispatch:fs_write");
  await submitHostCall(exports, result, "host_fs_list", { path: "/tmp" }, "dispatch:fs_list");
  await submitHostCall(exports, result, "host_fs_stat", { path: "/tmp/t" }, "dispatch:fs_stat");
  await submitHostCall(exports, result, "host_fs_remove", { path: "/tmp/o", recursive: false }, "dispatch:fs_remove");
  await submitHostCall(exports, result, "host_secret_get", { key: "K" }, "dispatch:secret_get");
  await submitHostCall(exports, result, "host_secret_set", { key: "K", value: "V" }, "dispatch:secret_set");
  await submitHostCall(exports, result, "host_sandbox_apply", { policy: "default" }, "dispatch:sandbox");

  // Callback tests
  try { exports.codex_deliver_callback(JSON.stringify({ correlation_id: randomUUID(), capability: "exec", payload: { kind: "exec_exit", exit_code: 0 } })); result.pass("callback:manual"); } catch (e) { result.fail("callback:manual", e.message); }
  try { exports.codex_deliver_callback(JSON.stringify(missingCapability(randomUUID(), "x"))); result.pass("callback:missing_cap"); } catch (e) { result.fail("callback:missing_cap", e.message); }
  try { exports.codex_cancel(JSON.stringify({ correlation_id: randomUUID(), reason: "test" })); result.pass("cancel"); } catch (e) { result.fail("cancel", e.message); }

  // Failure paths
  await submitHostCall(exports, result, "__unknown__", {}, "failure:unknown_cap");
  try { exports.codex_deliver_callback("{{bad json}}"); result.pass("failure:malformed_cb"); } catch (e) { result.fail("failure:malformed_cb", e.message); }
  try { exports.codex_deliver_callback("{}"); result.pass("failure:empty_cb"); } catch (e) { result.fail("failure:empty_cb", e.message); }
  try { exports.codex_cancel(JSON.stringify({ correlation_id: "nonexistent", reason: "t" })); result.pass("failure:cancel_ghost"); } catch (e) { result.fail("failure:cancel_ghost", e.message); }
  try { await exports.codex_shutdown(); result.pass("shutdown:first"); } catch (e) { result.fail("shutdown:first", e.message); }
  try { await exports.codex_shutdown(); result.pass("shutdown:double"); } catch (e) { result.fail("shutdown:double", e.message); }
  try { const r = await exports.codex_submit(JSON.stringify({ correlation_id: randomUUID(), kind: "prompt", payload: {} })); const p = JSON.parse(r); result.pass("failure:submit_post_shutdown", p.error ? `error:${p.error.code}` : "accepted"); } catch (e) { result.fail("failure:submit_post_shutdown", e.message); }
  try { const r = await exports.codex_init(JSON.stringify({ version: "0.1.0" })); const p = JSON.parse(r); if (p.ok || !p.error) result.pass("failure:reinit"); else result.fail("failure:reinit"); } catch (e) { result.fail("failure:reinit", e.message); }
  try { await exports.codex_shutdown(); } catch {}

  // Import coverage (wasm only)
  if (mode === "wasm") {
    for (const cap of ["exec", "network", "fs_read", "fs_write", "fs_list", "fs_stat", "fs_remove", "secret_get", "secret_set", "sandbox"]) {
      if (capabilities.log.some(l => l.capability === cap)) result.pass(`import:${cap}`);
      else result.fail(`import:${cap}`);
    }
    if (capabilities.log.some((l) => l.capability === "network" && l.payload?.stream_response)) result.pass("import:network_stream");
    else result.fail("import:network_stream");
  }

  // Loader integration test (wasm only)
  if (mode === "wasm") {
    await runLoaderTraceTest(result);
  }

  return result.report();
}

// --- Main ---
const args = process.argv.slice(2);
const useWasm = args.includes("--wasm") || args.includes("--wasm-dir");
const wasmDirIdx = args.indexOf("--wasm-dir");
const wasmOutDir = wasmDirIdx >= 0 ? args[wasmDirIdx + 1] : path.join(__dirname, "wasm-out");
const mode = useWasm ? "wasm" : "mock";

console.log(`codex-wasm host harness v0.6.0`);
console.log(`mode: ${mode}`);
if (useWasm) console.log(`wasm-out dir: ${wasmOutDir}`);

const report = await runHarness(mode, wasmOutDir);
process.exit(report.failed > 0 ? 1 : 0);
