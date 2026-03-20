# Auto-Web Harness: repo in, runnable output out

## Purpose

This document turns goal 2 from `WORK.md` into a concrete harness design.

The harness input is a GitHub repo or local repo path.
The harness output is a runnable portability package for our environments, plus the machine-readable artifacts needed to automate repeat runs.

This is not limited to Codex. Codex is the first target because it has the same shape future targets will have:

- mostly portable application logic
- a small amount of native-only behavior
- some process, terminal, network, filesystem, sandbox, or secret assumptions
- a need to replace those assumptions with explicit host services

## Required properties

- Input is a repo URL or local checkout.
- Output is deterministic enough to rerun on later repos.
- Output runs in our environments instead of assuming a generic browser-only runtime.
- The LLM is optional glue, not the only source of truth.
- The harness must emit machine-readable state so later runs are inspectable and replayable.

## Non-goals

- Fully autonomous perfect source translation for every repo shape.
- Replacing build systems wholesale.
- Making every project browser-ready on the first pass.
- Hiding unsupported native behavior instead of surfacing it.

## Recommended architecture

### Stage 1: Repo intake

Inputs:

- `repo_url` or local path
- target runtime profile such as `node-hosted-wasm`, `browser-hosted-wasm`, or `custom-hosted-wasm`
- optional policy file with allowed rewrites and forbidden dependencies

Actions:

- clone or open the repo
- detect primary languages and build systems
- detect package boundaries, entrypoints, and lockfiles
- detect whether the repo already contains wasm-related work

Outputs:

- normalized workspace snapshot
- toolchain summary
- build graph summary

### Stage 2: Native-surface inventory

Actions:

- scan for direct use of process spawning, PTY, sockets, filesystem, keyring, sandboxing, terminal UI, FFI, and OS-specific crates or packages
- classify each hit as `portable`, `gated`, `stubbed`, `host-provided`, or `excluded`
- identify the minimal runtime surface actually needed for the first runnable milestone

Outputs:

- `auto-web/manifest.json`
- `auto-web/native-surfaces.json`
- `auto-web/blockers.md`

This is the first point where the harness becomes reusable. The inventory pass should be mostly deterministic and should not depend on an LLM.

### Stage 3: Port strategy synthesis

Actions:

- choose a port pattern per surface:
  - `keep-as-is`
  - `cfg-gate`
  - `stub`
  - `bridge-to-host`
  - `exclude-from-target`
- choose the minimal public runtime surface for the target repo
- generate phase order and independent work tracks
- call an LLM only for bounded tasks such as patch planning, seam naming, or ambiguity resolution

Outputs:

- `auto-web/plan.md`
- `auto-web/phase-map.json`
- `auto-web/abi/host-contract.json`

The key rule is that the LLM consumes the inventory and emits proposals, but the harness stores the accepted plan in structured form.

### Stage 4: Seam generation

Actions:

- generate or update a target-specific bridge crate or module such as `wasm-bridge`
- generate host capability interfaces for exec, network, filesystem, secrets, and streaming
- generate stubs for unsupported native-only behavior
- insert target gates at the smallest viable seam points

Outputs:

- generated bridge module or crate
- generated host adapter skeleton
- generated patch bundle
- updated portability manifest

This is the stage most likely to be partially automated and partially reviewed by a human.

### Stage 5: Validation harness generation

Actions:

- generate compile checks for native and target builds
- generate ABI contract tests
- generate smoke tests that instantiate the target runtime and complete one full request cycle
- generate unsupported-feature tests so failures stay explicit

Outputs:

- `auto-web/tests/`
- `auto-web/validation.json`
- CI snippets or workflow fragments

### Stage 6: Runtime packaging

Actions:

- package the generated target build and host adapter
- emit a runnable command for our environments
- emit a machine-readable report describing what is supported, stubbed, and excluded

Outputs:

- runnable package
- `auto-web/report.json`
- `auto-web/README.md`

## LLM role

The LLM should be used, but only inside a harness that constrains it.

Good LLM uses:

- summarize blockers into phase language
- propose seam locations
- draft small patch sets
- rank alternative bridge strategies
- explain why a dependency is likely target-hostile

Bad LLM uses:

- being the only inventory mechanism
- inventing a build graph
- silently deciding what to exclude without recording it
- emitting free-form output that the harness cannot replay

The harness should always persist:

- what the detector found
- what the planner proposed
- what a human accepted
- what patches were generated
- what validations passed or failed

## Standard output contract

For every repo, the harness should try to emit the same top-level artifact set:

- `auto-web/manifest.json`: normalized repo inventory and portability status
- `auto-web/plan.md`: human-readable phases and task list
- `auto-web/phase-map.json`: machine-readable phase graph
- `auto-web/abi/host-contract.json`: host capability contract
- `auto-web/patches/`: generated patch units
- `auto-web/tests/`: generated compile and smoke validations
- `auto-web/report.json`: final support matrix and unresolved blockers

## Minimal Codex pilot

The first useful milestone is not full automation. The first useful milestone is a repeatable dry run that can inspect a repo and emit the same portability artifacts every time.

For this repo, the pilot should do exactly this:

- ingest the Codex workspace
- classify crates into `portable`, `gated`, and `excluded`
- emit a host capability contract for exec, network, filesystem, secrets, and sandbox stubs
- emit a phase map that matches the wasm roadmap
- emit validation requirements for native and wasm builds

If that works, the next step is limited code generation:

- generate the `wasm-bridge` skeleton
- generate host adapter skeletons
- generate validation scaffolding

## Why this helps future repos

Future repos with the same shape should be able to reuse the harness even if they do not use JavaScript today.

The reusable part is not the final adapter language. The reusable part is the pipeline:

- inventory native surfaces
- classify portability strategy
- synthesize seams
- generate bridge contracts
- generate validations
- package the result for the target environment

That keeps the input simple, repo in, and the output practical, runnable artifacts plus an explicit support matrix.
