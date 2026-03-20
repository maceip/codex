#!/usr/bin/env node
// Unified entry point for the Codex CLI.
// Two paths: native binary spawn (default) or WASM bridge (--wasm / CODEX_WASM=1).
// Set CODEX_WASM_TRACE=1 for machine-parseable JSON lifecycle events on stderr.

import { spawn } from "node:child_process";
import { existsSync, statSync } from "fs";
import { createRequire } from "node:module";
import path from "path";
import { fileURLToPath } from "url";
import { randomUUID } from "node:crypto";

import { runHostHttpRequest } from "./host-http-fetch.js";
import {
  createNodeFsAdapter,
  createSecretStore,
  registerPhase4HostCapabilities,
} from "./host-phase4-capabilities.js";
import { registerExtensionTransportHosts } from "./host-extension-transport.js";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const require = createRequire(import.meta.url);

// ---------------------------------------------------------------------------
// Lifecycle tracing
// ---------------------------------------------------------------------------
const TRACE = process.env.CODEX_WASM_TRACE === "1";
const traceLog = [];
function trace(event, detail = {}) {
  traceLog.push(event);
  if (TRACE) console.error(JSON.stringify({ t: Date.now(), event, ...detail }));
  else
    console.error(
      `[wasm] ${detail.error ? `${event}: ${detail.error}` : event}`,
    );
}

// ---------------------------------------------------------------------------
// Artifact discovery
// ---------------------------------------------------------------------------
function detectWasmMode() {
  return process.env.CODEX_WASM === "1" || process.argv.includes("--wasm");
}

function findWasmArtifacts() {
  const searchDirs = [
    path.resolve(__dirname, "../../tools/auto-web/wasm-out"),
    path.resolve(__dirname, "../vendor/wasm"),
  ];
  for (const dir of searchDirs) {
    const gluePath = path.join(dir, "codex_wasm_bridge.js");
    const wasmPath = path.join(dir, "codex_wasm_bridge_bg.wasm");
    const g = existsSync(gluePath),
      w = existsSync(wasmPath);
    if (g && w) return { gluePath, wasmPath, dir };
    if (g && !w) {
      trace("error:missing_wasm", { error: `.wasm missing at ${wasmPath}` });
      return null;
    }
    if (!g && w) {
      trace("error:missing_glue", { error: `JS glue missing at ${gluePath}` });
      return null;
    }
  }
  trace("error:no_artifacts", {
    error: "no wasm-bindgen artifacts found",
    searched: searchDirs,
  });
  console.error(
    "[wasm] Build: cd codex-rs && cargo build --target wasm32-unknown-unknown -p codex-wasm-bridge --no-default-features --features wasm && wasm-bindgen --target nodejs --out-dir tools/auto-web/wasm-out codex-rs/target/wasm32-unknown-unknown/debug/codex_wasm_bridge.wasm",
  );
  return null;
}

function reportArtifactInfo(artifacts) {
  try {
    const ws = statSync(artifacts.wasmPath),
      gs = statSync(artifacts.gluePath);
    const kb = Math.round(ws.size / 1024);
    const age = Math.max(Date.now() - gs.mtimeMs, Date.now() - ws.mtimeMs);
    const ageStr =
      age < 60000
        ? `${Math.round(age / 1000)}s`
        : age < 3600000
          ? `${Math.round(age / 60000)}m`
          : `${Math.round(age / 3600000)}h`;
    trace("artifacts_found", { dir: artifacts.dir, wasm_kb: kb, age: ageStr });
    if (Math.abs(Date.now() - gs.mtimeMs - (Date.now() - ws.mtimeMs)) > 60000) {
      trace("warn:artifact_mismatch", {
        error: "JS glue and .wasm have different timestamps",
      });
    }
  } catch {
    /* non-fatal */
  }
}

// ---------------------------------------------------------------------------
// Session tracker with diagnostics
// ---------------------------------------------------------------------------
class SessionTracker {
  constructor() {
    this._exitCode = 0;
    this._execActive = 0;
    this._execSeen = false;
    this._execTotal = 0;
    this._callbacksDelivered = 0;
    this._activeCallbackCids = new Set();
    this._shutdownReason = null;
    this._resolve = null;
    this._done = new Promise((r) => {
      this._resolve = r;
    });
  }

  execStarted(cid) {
    this._execActive++;
    this._execTotal++;
    this._execSeen = true;
    if (cid) this._activeCallbackCids.add(cid);
  }

  execFinished(cid, exitCode) {
    this._exitCode = exitCode;
    this._execActive--;
    this._callbacksDelivered++;
    if (cid) this._activeCallbackCids.delete(cid);
    if (this._execActive <= 0) this._resolve(exitCode);
  }

  execErrored(cid) {
    this._exitCode = 1;
    this._execActive--;
    this._callbacksDelivered++;
    if (cid) this._activeCallbackCids.delete(cid);
    if (this._execActive <= 0) this._resolve(1);
  }

  get execWasSeen() {
    return this._execSeen;
  }
  get done() {
    return this._done;
  }

  finish(code, reason) {
    this._shutdownReason = reason || "explicit";
    this._resolve(code);
  }

  diagnostics() {
    return {
      exec_seen: this._execSeen,
      exec_active: this._execActive,
      exec_total: this._execTotal,
      callbacks_delivered: this._callbacksDelivered,
      active_cids: [...this._activeCallbackCids],
      exit_code: this._exitCode,
      shutdown_reason: this._shutdownReason,
    };
  }
}

// ---------------------------------------------------------------------------
// Host imports
// ---------------------------------------------------------------------------

function installHostImports(session) {
  const parse = (json) => {
    try {
      return JSON.parse(json);
    } catch {
      return {};
    }
  };
  const bridge = () => globalThis.__codex_bridge;

  globalThis.host_exec_request = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    session.execStarted(cid);
    trace("host_call:exec", { cid, command: req.command });
    try {
      const child = spawn(req.command, req.args || [], {
        cwd: req.cwd || undefined,
        env: req.env ? { ...process.env, ...req.env } : process.env,
        stdio: ["pipe", "pipe", "pipe"],
      });
      child.stdout?.on("data", (data) => {
        process.stdout.write(data);
        bridge()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "exec",
            payload: {
              kind: "exec_chunk",
              correlation_id: cid,
              stream: "stdout",
              data: data.toString(),
            },
          }),
        );
      });
      child.stderr?.on("data", (data) => {
        process.stderr.write(data);
        bridge()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "exec",
            payload: {
              kind: "exec_chunk",
              correlation_id: cid,
              stream: "stderr",
              data: data.toString(),
            },
          }),
        );
      });
      child.on("exit", (code, signal) => {
        const ec = code ?? 1;
        trace("host_call:exec_exit", { cid, exit_code: ec });
        bridge()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "exec",
            payload: {
              kind: "exec_exit",
              correlation_id: cid,
              exit_code: ec,
              signal: signal || null,
              cancelled: false,
            },
          }),
        );
        session.execFinished(cid, ec);
      });
      child.on("error", (err) => {
        trace("host_call:exec_error", { cid, error: err.message });
        bridge()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            error: { code: "internal", message: err.message, details: null },
          }),
        );
        session.execErrored(cid);
      });
      if (req.stdin) {
        child.stdin?.write(req.stdin);
        child.stdin?.end();
      }
    } catch (err) {
      trace("host_call:exec_error", { cid, error: err.message });
      bridge()?.codex_deliver_callback(
        JSON.stringify({
          correlation_id: cid,
          error: { code: "internal", message: err.message, details: null },
        }),
      );
      session.execErrored(cid);
    }
  };

  globalThis.host_http_request = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    runHostHttpRequest(req, {
      deliver: (envelope) =>
        bridge()?.codex_deliver_callback(JSON.stringify(envelope)),
      trace: (event, detail) => trace(event, detail),
    }).catch((err) => {
      trace("host_call:http_unhandled", { cid, error: err?.message });
      bridge()?.codex_deliver_callback(
        JSON.stringify({
          correlation_id: cid,
          error: {
            code: "internal",
            message: err?.message || String(err),
            details: null,
          },
        }),
      );
    });
  };

  registerExtensionTransportHosts(globalThis, { bridge });

  const wasmSecrets = createSecretStore({ envFallback: true });
  registerPhase4HostCapabilities(globalThis, {
    bridge,
    trace,
    fs: createNodeFsAdapter(),
    secrets: wasmSecrets,
  });
}

// ---------------------------------------------------------------------------
// Bridge lifecycle
// ---------------------------------------------------------------------------
async function runWasmBridge(artifacts) {
  const session = new SessionTracker();
  installHostImports(session);
  reportArtifactInfo(artifacts);

  let wasm;
  try {
    wasm = require(artifacts.gluePath);
    trace("glue_loaded");
  } catch (err) {
    trace("error:glue_load", { error: err.message });
    process.exit(1);
  }
  globalThis.__codex_bridge = wasm;

  let initResult;
  try {
    initResult = await wasm.codex_init(JSON.stringify({ version: "0.1.0" }));
  } catch (err) {
    trace("error:init_threw", { error: err.message });
    process.exit(1);
  }
  const initParsed = JSON.parse(initResult);
  if (initParsed.error) {
    trace("error:init_failed", { error: initParsed.error.message });
    process.exit(1);
  }
  trace("init_ok");

  const args = process.argv.slice(2).filter((a) => a !== "--wasm");
  const correlationId = randomUUID();
  const submitBody =
    args.length === 0
      ? {
          correlation_id: correlationId,
          kind: "idle",
          payload: { cwd: process.cwd() },
        }
      : {
          correlation_id: correlationId,
          kind: "cli",
          payload: { args, cwd: process.cwd() },
        };
  let submitResult;
  try {
    submitResult = await wasm.codex_submit(JSON.stringify(submitBody));
  } catch (err) {
    trace("error:submit_threw", { error: err.message });
    process.exit(1);
  }
  const submitParsed = JSON.parse(submitResult);
  if (submitParsed.error) {
    trace("error:submit_failed", { error: submitParsed.error.message });
    process.exit(1);
  }
  trace("submit_ok", { cid: correlationId });

  let shuttingDown = false;
  const shutdown = async (exitCode, reason) => {
    if (shuttingDown) return;
    shuttingDown = true;
    session.finish(exitCode, reason);
    try {
      await wasm.codex_shutdown();
    } catch {
      /* ignore */
    }
    const diag = session.diagnostics();
    trace("shutdown", { exit_code: exitCode, reason, ...diag });
    process.exit(exitCode);
  };

  process.on("SIGINT", () => shutdown(130, "sigint"));
  process.on("SIGTERM", () => shutdown(143, "sigterm"));

  const GRACE_MS = 500;
  await new Promise((resolve) => setTimeout(resolve, GRACE_MS));

  let exitCode;
  if (session.execWasSeen) {
    trace("exec_dispatched", session.diagnostics());
    exitCode = await session.done;
    trace("exec_done", { exit_code: exitCode, ...session.diagnostics() });
  } else {
    trace("no_exec", session.diagnostics());
    exitCode = 0;
  }

  await shutdown(exitCode, session.execWasSeen ? "exec_completed" : "no_exec");
}

// ---------------------------------------------------------------------------
// Mode selection
// ---------------------------------------------------------------------------
if (detectWasmMode()) {
  const artifacts = findWasmArtifacts();
  if (!artifacts) process.exit(1);
  await runWasmBridge(artifacts);
} else {
  // --- Original native binary spawn path (unchanged) ---
  const PLATFORM_PACKAGE_BY_TARGET = {
    "x86_64-unknown-linux-musl": "@openai/codex-linux-x64",
    "aarch64-unknown-linux-musl": "@openai/codex-linux-arm64",
    "x86_64-apple-darwin": "@openai/codex-darwin-x64",
    "aarch64-apple-darwin": "@openai/codex-darwin-arm64",
    "x86_64-pc-windows-msvc": "@openai/codex-win32-x64",
    "aarch64-pc-windows-msvc": "@openai/codex-win32-arm64",
  };
  const { platform, arch } = process;
  let targetTriple = null;
  switch (platform) {
    case "linux":
    case "android":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-unknown-linux-musl";
          break;
        case "arm64":
          targetTriple = "aarch64-unknown-linux-musl";
          break;
      }
      break;
    case "darwin":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-apple-darwin";
          break;
        case "arm64":
          targetTriple = "aarch64-apple-darwin";
          break;
      }
      break;
    case "win32":
      switch (arch) {
        case "x64":
          targetTriple = "x86_64-pc-windows-msvc";
          break;
        case "arm64":
          targetTriple = "aarch64-pc-windows-msvc";
          break;
      }
      break;
  }
  if (!targetTriple)
    throw new Error(`Unsupported platform: ${platform} (${arch})`);
  const platformPackage = PLATFORM_PACKAGE_BY_TARGET[targetTriple];
  if (!platformPackage)
    throw new Error(`Unsupported target triple: ${targetTriple}`);
  const codexBinaryName = process.platform === "win32" ? "codex.exe" : "codex";
  const localVendorRoot = path.join(__dirname, "..", "vendor");
  const localBinaryPath = path.join(
    localVendorRoot,
    targetTriple,
    "codex",
    codexBinaryName,
  );
  let vendorRoot;
  try {
    vendorRoot = path.join(
      path.dirname(require.resolve(`${platformPackage}/package.json`)),
      "vendor",
    );
  } catch {
    if (existsSync(localBinaryPath)) vendorRoot = localVendorRoot;
    else {
      const pm = detectPM();
      throw new Error(
        `Missing ${platformPackage}. Reinstall: ${pm === "bun" ? "bun" : "npm"} install -g @openai/codex@latest`,
      );
    }
  }
  if (!vendorRoot) {
    const pm = detectPM();
    throw new Error(
      `Missing ${platformPackage}. Reinstall: ${pm === "bun" ? "bun" : "npm"} install -g @openai/codex@latest`,
    );
  }
  function detectPM() {
    const ua = process.env.npm_config_user_agent || "";
    if (/\bbun\//.test(ua)) return "bun";
    if ((process.env.npm_execpath || "").includes("bun")) return "bun";
    if (__dirname.includes(".bun/install/global")) return "bun";
    return ua ? "npm" : null;
  }
  const archRoot = path.join(vendorRoot, targetTriple);
  const binaryPath = path.join(archRoot, "codex", codexBinaryName);
  const pathSep = process.platform === "win32" ? ";" : ":";
  const additionalDirs = [];
  const pathDir = path.join(archRoot, "path");
  if (existsSync(pathDir)) additionalDirs.push(pathDir);
  const env = {
    ...process.env,
    PATH: [
      ...additionalDirs,
      ...(process.env.PATH || "").split(pathSep).filter(Boolean),
    ].join(pathSep),
  };
  env[detectPM() === "bun" ? "CODEX_MANAGED_BY_BUN" : "CODEX_MANAGED_BY_NPM"] =
    "1";
  const child = spawn(binaryPath, process.argv.slice(2), {
    stdio: "inherit",
    env,
  });
  child.on("error", (err) => {
    console.error(err);
    process.exit(1);
  });
  ["SIGINT", "SIGTERM", "SIGHUP"].forEach((sig) =>
    process.on(sig, () => {
      if (!child.killed)
        try {
          child.kill(sig);
        } catch {}
    }),
  );
  const r = await new Promise((resolve) => {
    child.on("exit", (code, signal) => {
      resolve(
        signal
          ? { type: "signal", signal }
          : { type: "code", exitCode: code ?? 1 },
      );
    });
  });
  if (r.type === "signal") process.kill(process.pid, r.signal);
  else process.exit(r.exitCode);
}
