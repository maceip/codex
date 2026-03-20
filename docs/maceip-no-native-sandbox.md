# No native Linux sandbox in this tree

This fork **does not build** the upstream `codex-linux-sandbox` helper (bubblewrap, vendored C,
`libcap`, Landlock, or seccomp in that binary).

- **Crate layout:** `codex-rs/linux-sandbox` remains as a **stub** library/binary so `codex-arg0`
  and workspace metadata stay aligned. Invoking the binary exits with status **2** and prints a
  short message.
- **`codex-core`:** Linux-only **`landlock`** and **`seccompiler`** crate dependencies were
  removed; error variants that existed only for those conversions were dropped. Logical “sandbox”
  errors (`SandboxErr`, `LandlockSandboxExecutableNotProvided`, etc.) are unchanged.
- **Policy:** Enforce isolation in the **host runtime** (wasm bridge, `host_sandbox_apply` stub,
  OS/container policy), not via the historical Linux helper.

Windows sandbox code remains in **`codex-windows-sandbox`** for Windows targets (mostly cfg-gated
stubs on non-Windows); removing that is separate if the host never runs Windows-native Codex.

## Validation

```bash
cd codex-rs && cargo check -p codex-linux-sandbox -p codex-core --lib
```

No `pkg-config libcap` or bubblewrap compile is required for these crates.
