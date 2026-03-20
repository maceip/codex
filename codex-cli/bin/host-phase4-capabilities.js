/**
 * Phase 4: filesystem, secrets, and sandbox host adapters (Node + optional MEMFS).
 * Used by codex-cli wasm mode and auto-web E2E harnesses.
 */

import {
  readFile,
  writeFile,
  mkdir,
  readdir,
  stat,
  rm,
} from "node:fs/promises";
import path from "node:path";

/**
 * @typedef {object} FsAdapter
 * @property {(p: string, encoding: string | null) => Promise<string | Buffer>} readFile
 * @property {(p: string, data: string | Buffer) => Promise<void>} writeFile
 * @property {(p: string, opts?: { recursive?: boolean }) => Promise<void>} mkdir
 * @property {(p: string) => Promise<import("node:fs").Dirent[]>} readdir
 * @property {(p: string) => Promise<import("node:fs").Stats>} stat
 * @property {(p: string, opts?: { recursive?: boolean }) => Promise<void>} rm
 */

/**
 * @param {{ envFallback?: boolean }} [options]
 */
export function createSecretStore(options = {}) {
  const { envFallback = true } = options;
  /** @type {Map<string, string>} */
  const map = new Map();
  return {
    /** @param {string} key */
    get(key) {
      if (map.has(key)) {
        return map.get(key) ?? null;
      }
      if (envFallback) {
        return process.env[key] ?? null;
      }
      return null;
    },
    /** @param {string} key @param {string} value */
    set(key, value) {
      map.set(key, value);
    },
    /** @param {string} key */
    delete(key) {
      map.delete(key);
    },
  };
}

/**
 * Real disk I/O (current CLI default). Optional `jailRoot` confines paths.
 * @param {{ jailRoot?: string | null }} [opts]
 * @returns {FsAdapter}
 */
export function createNodeFsAdapter(opts = {}) {
  const jailRoot = opts.jailRoot ?? null;

  /** @param {string} reqPath */
  function resolvePath(reqPath) {
    if (!jailRoot) {
      return reqPath;
    }
    const resolved = path.resolve(jailRoot, reqPath);
    const root = path.resolve(jailRoot);
    if (!resolved.startsWith(root + path.sep) && resolved !== root) {
      const err = new Error("path escapes jailRoot");
      err.code = "EPERM";
      throw err;
    }
    return resolved;
  }

  return {
    readFile: (p, enc) =>
      readFile(
        resolvePath(p),
        enc === "binary" || enc === null ? null : "utf-8",
      ),
    writeFile: (p, data) => writeFile(resolvePath(p), data),
    mkdir: (p, o) => mkdir(resolvePath(p), o),
    readdir: (p) => readdir(resolvePath(p), { withFileTypes: true }),
    stat: (p) => stat(resolvePath(p)),
    rm: (p, o) => rm(resolvePath(p), o),
  };
}

/**
 * In-memory POSIX-like tree for deterministic wasm tests (MEMFS).
 * @returns {FsAdapter}
 */
export function createMemFsBackend() {
  /** @type {Map<string, 'dir' | { content: Buffer }>} */
  const nodes = new Map([["/", "dir"]]);

  /** @param {string} p */
  function norm(p) {
    let x = path.posix.normalize(String(p).replaceAll("\\", "/"));
    if (!x.startsWith("/")) {
      x = `/${x}`;
    }
    const cleaned = x.replace(/\/+$/, "");
    return cleaned === "" ? "/" : cleaned;
  }

  /** @param {string} np */
  function ensureParentDirs(np) {
    if (np === "/" || np === "") {
      return;
    }
    const parent = path.posix.dirname(np);
    ensureParentDirs(parent);
    if (!nodes.has(parent)) {
      nodes.set(parent, "dir");
    } else if (nodes.get(parent) !== "dir") {
      const err = new Error("ENOTDIR");
      err.code = "ENOTDIR";
      throw err;
    }
  }

  return {
    async readFile(p, encoding) {
      const np = norm(p);
      const n = nodes.get(np);
      if (!n || n === "dir") {
        const err = new Error(`ENOENT ${np}`);
        err.code = "ENOENT";
        throw err;
      }
      const buf = n.content;
      if (encoding === null || encoding === "binary") {
        return buf;
      }
      return buf.toString("utf8");
    },
    async writeFile(p, data) {
      const np = norm(p);
      ensureParentDirs(np);
      const buf = Buffer.isBuffer(data) ? data : Buffer.from(data, "utf8");
      nodes.set(np, { content: buf });
    },
    async mkdir(p, options) {
      const np = norm(p);
      if (options?.recursive) {
        const parts = np.split("/").filter(Boolean);
        let cur = "";
        for (const part of parts) {
          cur = `${cur}/${part}`;
          nodes.set(cur, "dir");
        }
        return;
      }
      const parent = path.posix.dirname(np);
      if (parent !== "/" && !nodes.has(parent)) {
        const err = new Error("ENOENT");
        err.code = "ENOENT";
        throw err;
      }
      if (nodes.has(np)) {
        const err = new Error("EEXIST");
        err.code = "EEXIST";
        throw err;
      }
      nodes.set(np, "dir");
    },
    async readdir(p) {
      const np = norm(p);
      if (!nodes.has(np) || nodes.get(np) !== "dir") {
        const err = new Error("ENOENT");
        err.code = "ENOENT";
        throw err;
      }
      const prefix = np === "/" ? "/" : `${np}/`;
      /** @type {Map<string, import("node:fs").Dirent>} */
      const found = new Map();
      for (const k of nodes.keys()) {
        if (k === np || k === "/") {
          continue;
        }
        if (!k.startsWith(prefix)) {
          continue;
        }
        const rest = k.slice(prefix.length);
        const name = rest.split("/")[0];
        if (!name || found.has(name)) {
          continue;
        }
        const full =
          `${np === "/" ? "" : np}/${name}`.replace("//", "/") || `/${name}`;
        const node = nodes.get(full);
        const kind = node?.content ? "file" : "dir";
        found.set(name, {
          name,
          isDirectory: () => kind === "dir",
          isFile: () => kind === "file",
          isSymbolicLink: () => false,
        });
      }
      return [...found.values()];
    },
    async stat(p) {
      const np = norm(p);
      const n = nodes.get(np);
      if (!n) {
        const err = new Error("ENOENT");
        err.code = "ENOENT";
        throw err;
      }
      const isDir = n === "dir";
      return {
        isDirectory: () => isDir,
        size: isDir ? 0 : n.content.length,
        mtimeMs: Date.now(),
      };
    },
    async rm(p, options) {
      const np = norm(p);
      const n = nodes.get(np);
      if (!n) {
        const err = new Error("ENOENT");
        err.code = "ENOENT";
        throw err;
      }
      if (n === "dir" && options?.recursive) {
        for (const k of [...nodes.keys()]) {
          if (k === np || k.startsWith(`${np}/`)) {
            nodes.delete(k);
          }
        }
        return;
      }
      if (n === "dir") {
        for (const k of nodes.keys()) {
          if (k.startsWith(`${np}/`)) {
            const err = new Error("ENOTEMPTY");
            err.code = "ENOTEMPTY";
            throw err;
          }
        }
      }
      nodes.delete(np);
    },
  };
}

/**
 * Install `host_fs_*`, `host_secret_*`, `host_sandbox_apply` on `globalTarget`
 * (usually `globalThis`).
 *
 * @param {typeof globalThis} globalTarget
 * @param {object} opts
 * @param {() => unknown} opts.bridge wasm exports (`codex_deliver_callback`, …)
 * @param {(event: string, detail?: object) => void} [opts.trace]
 * @param {FsAdapter} opts.fs
 * @param {ReturnType<typeof createSecretStore>} opts.secrets
 */
export function registerPhase4HostCapabilities(globalTarget, opts) {
  const { bridge, trace = () => {}, fs, secrets } = opts;
  const parse = (json) => {
    try {
      return JSON.parse(json);
    } catch {
      return {};
    }
  };
  const b = () => bridge();

  globalTarget.host_fs_read = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    (async () => {
      try {
        const enc = req.encoding === "binary" ? null : "utf-8";
        const data = await fs.readFile(req.path, enc);
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "fs_read",
            payload: {
              correlation_id: cid,
              data:
                typeof data === "string"
                  ? data
                  : Buffer.from(data).toString("base64"),
              encoding: typeof data === "string" ? "utf-8" : "base64",
            },
          }),
        );
      } catch (err) {
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            error: {
              code: err.code === "ENOENT" ? "not_found" : "internal",
              message: err.message,
              details: null,
            },
          }),
        );
      }
    })();
  };

  globalTarget.host_fs_write = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    (async () => {
      try {
        if (req.create_dirs) {
          await fs.mkdir(path.dirname(req.path), { recursive: true });
        }
        const buf =
          req.encoding === "base64"
            ? Buffer.from(req.data, "base64")
            : req.data;
        await fs.writeFile(req.path, buf);
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "fs_write",
            payload: {
              correlation_id: cid,
              bytes_written: Buffer.byteLength(buf),
            },
          }),
        );
      } catch (err) {
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            error: {
              code: "internal",
              message: err.message,
              details: null,
            },
          }),
        );
      }
    })();
  };

  globalTarget.host_fs_list = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    (async () => {
      try {
        const dirents = await fs.readdir(req.path);
        const entries = dirents.map((d) => ({
          name: d.name,
          kind: d.isDirectory()
            ? "directory"
            : d.isSymbolicLink()
              ? "symlink"
              : "file",
          size: null,
        }));
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "fs_list",
            payload: { correlation_id: cid, entries },
          }),
        );
      } catch (err) {
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            error: {
              code: err.code === "ENOENT" ? "not_found" : "internal",
              message: err.message,
              details: null,
            },
          }),
        );
      }
    })();
  };

  globalTarget.host_fs_stat = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    (async () => {
      try {
        const s = await fs.stat(req.path);
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "fs_stat",
            payload: {
              correlation_id: cid,
              exists: true,
              kind: s.isDirectory() ? "directory" : "file",
              size: s.size,
              modified_ms: s.mtimeMs,
            },
          }),
        );
      } catch (err) {
        if (err.code === "ENOENT") {
          b()?.codex_deliver_callback(
            JSON.stringify({
              correlation_id: cid,
              capability: "fs_stat",
              payload: {
                correlation_id: cid,
                exists: false,
                kind: null,
                size: null,
                modified_ms: null,
              },
            }),
          );
        } else {
          b()?.codex_deliver_callback(
            JSON.stringify({
              correlation_id: cid,
              error: {
                code: "internal",
                message: err.message,
                details: null,
              },
            }),
          );
        }
      }
    })();
  };

  globalTarget.host_fs_remove = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    (async () => {
      try {
        await fs.rm(req.path, { recursive: !!req.recursive });
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            capability: "fs_remove",
            payload: { correlation_id: cid, removed: true },
          }),
        );
      } catch (err) {
        b()?.codex_deliver_callback(
          JSON.stringify({
            correlation_id: cid,
            error: {
              code: err.code === "ENOENT" ? "not_found" : "internal",
              message: err.message,
              details: null,
            },
          }),
        );
      }
    })();
  };

  globalTarget.host_secret_get = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    const value = secrets.get(req.key);
    b()?.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        capability: "secret_get",
        payload: { correlation_id: cid, value, found: value !== null },
      }),
    );
  };

  globalTarget.host_secret_set = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    secrets.set(req.key, req.value);
    b()?.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        capability: "secret_set",
        payload: { correlation_id: cid, stored: true },
      }),
    );
  };

  globalTarget.host_secret_delete = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    secrets.delete(req.key);
    delete process.env[req.key];
    b()?.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        capability: "secret_delete",
        payload: { correlation_id: cid, deleted: true },
      }),
    );
  };

  globalTarget.host_sandbox_apply = (json) => {
    const req = parse(json);
    const cid = req.correlation_id;
    trace("host_call:sandbox_stub", { cid });
    b()?.codex_deliver_callback(
      JSON.stringify({
        correlation_id: cid,
        capability: "sandbox",
        payload: {
          correlation_id: cid,
          applied: false,
          stub: true,
          reason: "sandbox is not enforced in wasm mode",
        },
      }),
    );
  };
}
