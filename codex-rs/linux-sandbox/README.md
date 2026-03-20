# codex-linux-sandbox (stub)

This crate is a **compatibility stub** for this fork: it does **not** compile or ship
bubblewrap, libcap, Landlock, or seccomp. The `codex-linux-sandbox` executable exits with
status **2** and prints a short message.

Native sandboxing is expected to be enforced by the **host** (e.g. wasm bridge / MaceIP),
not by a separate Linux helper binary.

Upstream Codex behavior is described in the parent repository; this tree intentionally diverges
here to avoid `libcap.pc`, vendored bubblewrap, and related build dependencies.
