#!/usr/bin/env node
/**
 * Browser E2E: builds wasm + no-modules glue, serves over HTTP, drives
 * Chromium via Puppeteer (bundled browser). Fails if any step fails — no skips.
 */

import { execSync, spawnSync } from "node:child_process";
import { createReadStream, existsSync } from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import puppeteer from "puppeteer";

const toolsAutoWeb = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(toolsAutoWeb, "..", "..");
const wasmBrowserDir = path.join(toolsAutoWeb, "wasm-browser");
const indexHtml = path.join(wasmBrowserDir, "index.html");
const wasmJs = path.join(wasmBrowserDir, "codex_wasm_bridge.js");
const wasmBin = path.join(wasmBrowserDir, "codex_wasm_bridge_bg.wasm");

function mime(p) {
  if (p.endsWith(".html")) return "text/html; charset=utf-8";
  if (p.endsWith(".js")) return "application/javascript; charset=utf-8";
  if (p.endsWith(".wasm")) return "application/wasm";
  return "application/octet-stream";
}

function ensureBuilt() {
  const r = spawnSync(
    process.execPath,
    [path.join(toolsAutoWeb, "build-wasm-bundles.mjs")],
    {
      cwd: repoRoot,
      stdio: "inherit",
    },
  );
  if (r.status !== 0) {
    throw new Error(`build-wasm-bundles failed with code ${r.status}`);
  }
  if (!existsSync(indexHtml) || !existsSync(wasmJs) || !existsSync(wasmBin)) {
    throw new Error(`missing browser bundle under ${wasmBrowserDir}`);
  }
}

function startServer(port) {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url || "/", `http://127.0.0.1:${port}`);

    if (req.method === "GET" && url.pathname === "/browser-ping") {
      res.writeHead(200, { "Content-Type": "text/plain; charset=utf-8" });
      res.end("browser-ping-ok");
      return;
    }

    if (req.method === "GET" && url.pathname === "/favicon.ico") {
      res.writeHead(204).end();
      return;
    }

    const safePath =
      url.pathname === "/" || url.pathname === ""
        ? "index.html"
        : url.pathname.replace(/^\//, "");
    if (safePath.includes("..") || safePath.includes("\0")) {
      res.writeHead(400).end();
      return;
    }

    const filePath = path.join(wasmBrowserDir, safePath);
    if (!existsSync(filePath)) {
      res.writeHead(404).end();
      return;
    }

    res.writeHead(200, { "Content-Type": mime(filePath) });
    createReadStream(filePath).pipe(res);
  });

  return new Promise((resolve, reject) => {
    server.listen(port, "127.0.0.1", () => resolve(server));
    server.on("error", reject);
  });
}

/**
 * Puppeteer’s *pinned* `chrome` build often 404s or partial-downloads in constrained
 * networks; `chrome@stable` is still the same **Chrome-for-Testing** pipeline and is
 * what we treat as the supported “Puppeteer Chrome” bundle for this repo.
 */
function resolvePuppeteerChromeExecutable() {
  if (process.env.CODEX_PUPPETEER_EXECUTABLE) {
    return process.env.CODEX_PUPPETEER_EXECUTABLE;
  }
  return execSync(
    'pnpm exec puppeteer browsers install chrome@stable --format "{{path}}"',
    { cwd: repoRoot, encoding: "utf8" },
  )
    .trim()
    .split("\n")
    .filter(Boolean)
    .pop();
}

async function main() {
  ensureBuilt();

  const executablePath = resolvePuppeteerChromeExecutable();
  if (!executablePath) {
    throw new Error(
      "could not resolve Chrome-for-Testing executable from Puppeteer",
    );
  }

  const server = await startServer(0);
  const addr = server.address();
  if (!addr || typeof addr === "string") {
    throw new Error("server address");
  }
  const origin = `http://127.0.0.1:${addr.port}`;

  const browser = await puppeteer.launch({
    headless: true,
    executablePath,
    args: ["--no-sandbox", "--disable-setuid-sandbox"],
  });

  try {
    const page = await browser.newPage();
    await page.evaluateOnNewDocument((o) => {
      globalThis.__BROWSER_HARNESS_ORIGIN__ = o;
    }, origin);

    page.on("pageerror", (err) => {
      console.error("[pageerror]", err);
    });
    page.on("console", (msg) => {
      if (msg.type() === "error") {
        console.error("[page console]", msg.text());
      }
    });

    await page.goto(`${origin}/`, { waitUntil: "load", timeout: 120_000 });

    await page.waitForFunction(
      () =>
        globalThis.__CODEX_BROWSER_E2E__ &&
        typeof globalThis.__CODEX_BROWSER_E2E__ === "object",
      { timeout: 120_000 },
    );

    const result = await page.evaluate(() => globalThis.__CODEX_BROWSER_E2E__);
    if (!result.ok) {
      throw new Error(result.error || "harness reported failure");
    }

    const status = await page.$eval("#status", (el) => el.textContent);
    if (status !== "ok") {
      throw new Error(`unexpected #status: ${status}`);
    }

    console.log(`e2e-browser-puppeteer: ok (Chrome binary: ${executablePath})`);
  } finally {
    await browser.close();
    server.close();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
