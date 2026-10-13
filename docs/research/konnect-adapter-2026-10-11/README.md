# Konnect captured-copy adapter

October 11, 2026. Actual Windows Konnect 0.13.0 / KiCad 10.0.7 capture-to-edit adapter passes. This is backend adapter progress; shared app proposals/publication/recovery and native acceptance are unfinished.

`konnect_run::KonnectInput` binds job identity, exact integration version, captured KiCad source/dependencies and a typed reference/current/new value. The adapter verifies the identified executable and KiCad runtime, captures source bytes into Original, creates a portable Edited project using the existing dependency relocation logic, and stores the exact request. The original selected document is preserved.

The official rmcp 3.5.1 stdio client starts the pinned executable in a private configuration/profile with Windows Job ownership. Only the identified server/protocol and exact get/edit tool schemas are accepted. It confirms the current selected component value, dispatches the edit once, reads back the entire component metadata and closes the session. A pinned Windows handle independently observes server cessation. Connection/edit deadlines and cancellation paths retain staging without publishing and do not retry uncertain writes.

The independent Rust full-schematic verifier accepts only the approved Value changes. All other captured files must equal their independently regenerated portable bytes; Original remains hash-bound to the capture. The complete expected inventory, exact request, result identity and output hashes are checked before writing a bounded result manifest. Unexpected files, tampered requests and altered geometry/placement/content are refused.

The shared KiCad `portable_files` helper now exposes its existing relocation calculation to verification without duplicating the resource resolver; the original working-copy caller uses the same logic. All four hierarchy/resource capture regression tests pass.

## Evidence

- [Actual adapter status and source hashes](status.json), [final result](Final/Stage/result.json), [approved request](Final/Stage/input.json) and captured/Original/Edited files retain the real run. The selected R1 value changes from 10k to 12.34k. The test verifies source byte preservation and validates the entire stage again after completion. The earlier Stage/Capture fixture is retained separately; the Final fixture passes against the final source hashes.
- Two regular adapter tests check portable source/result validation, unexpected inventory, modified request, altered schematic, stale job identity and cancellation before dispatch without a process/private files. Four existing typed-contract/verifier tests cover malformed structure, exact decimal normalization, unrelated changes, duplicate properties and resource limits.
- Current suite: 75 regular tests pass, 23 separately scoped ignored entries. The actual adapter test is run explicitly, not counted as a regular test. Desktop/custom-protocol Clippy passes.
- The [preceding runtime/SDK gate](../konnect-runtime-2026-10-11/README.md) retains package identity, unsigned Authenticode result, raw/SDK catalog evidence, editor-lock refusal, independent KiCad netlist readback and actual SDK process-tree teardown fixtures.

```powershell
rtk cargo test -p hazlomexa --lib
rtk cargo test -p hazlomexa --lib actual_konnect_captured_copy_adapter -- --ignored --nocapture
rtk cargo clippy -p hazlomexa --all-targets --features tauri/custom-protocol -- -D warnings
```

## Remaining scope

Connect this adapter to shared app proposal/approval, artifact publication, history and recovery, then conversational agent inputs and native UI. Current typed editing handles component values in the selected schematic; broader schematic transformations and PCB native IPC editing remain required. Live cancellation/timeout and lost-response reconciliation, native/installed/second-environment workflows and the final installer are unverified. Dedicated native KiCad validation in the production adapter is still required; the earlier independent netlist gate does not certify every captured project. The full application goal remains active.
