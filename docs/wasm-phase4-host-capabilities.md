# Phase 4: Host filesystem, secrets, sandbox (MEMFS + Node)

Phase 4 from [`maceip-wasm-roadmap.md`](maceip-wasm-roadmap.md) lands in **`codex-cli/bin/host-phase4-capabilities.js`**.

## Node loader (default)

- **`createNodeFsAdapter()`** — passes through to `node:fs/promises` with the same paths the wasm module requests.
- **`createSecretStore({ envFallback })`** — JS `Map` first; with `envFallback: true` (CLI default) falls back to `process.env` on `secret_get`.
- **`registerPhase4HostCapabilities(globalThis, { bridge, trace, fs, secrets })`** — installs `host_fs_*`, `host_secret_*`, `host_sandbox_apply`.

## MEMFS (tests / deterministic hosts)

- **`createMemFsBackend()`** — pure in-memory tree keyed by POSIX paths. Use for harnesses that must not touch disk or for replay tooling.

## Sandbox

`host_sandbox_apply` always returns an explicit stub payload (`applied: false`, `stub: true`, human-readable `reason`) so wasm callers never assume Seatbelt/bubblewrap enforcement in this mode.

## Validation

```bash
node tools/auto-web/e2e-phase4-fs-secrets.mjs
# Full phases 3–6 + browser (see [`wasm-support-policy.md`](wasm-support-policy.md))
pnpm run e2e:wasm-milestone
```
