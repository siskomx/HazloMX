# CADCraft native AutoLISP binding checkpoint

The released CADCraft 0.7.0 Windows application provides a native route that the pinned 0.5.0 interface did not: programmatic AutoLISP with drawing-local variables. The existing owned desktop probe now has an opt-in 0.7.0 mode. Production runtime selection and saved-copy adapters remain unchanged; live chat writes are not enabled.

## Native contract and actual result

The [0.7.0 native command](https://github.com/storytold/cadcraft/blob/v0.7.0/crates/engine/src/cmd/lisp.rs) evaluates source in blocking mode and returns its value or error. The [native evaluator](https://github.com/storytold/cadcraft/blob/v0.7.0/crates/engine/src/lisp/mod.rs) keeps environments by internal document UID and checks that a running routine still belongs to the active drawing. This suggests a way to validate a binding and edit through one program-owned call, instead of separate host reads and writes.

The actual owned GUI probe establishes:

- A fresh token and captured circle state stored in drawing-local LISP variables do not change engineering geometry.
- A wrong token refuses the edit without changing geometry.
- One native call compares both the token and exact captured `entget` state, then changes only the circle's X coordinate to 1 mm.
- Reusing that snapshot after the edit refuses another change and preserves the current circle.
- Capturing the new state permits a 0.1-inch follow-up, reaching X = 3.54 mm. The complete inspected entity projection differs only in X; counts and layers remain unchanged.
- Each edit is one native Undo step. Two Undo calls restore the intermediate and original inspected geometry.
- Closing and reopening the identical saved drawing does not inherit the old token: the guarded edit is refused even though the engineering geometry is identical.
- The saved source remains byte-identical and the owned editor process tree is stopped and joined.

The first 0.7.0 run stopped safely because the older probe assumed one startup document. This release also opens a blank drawing. The harness now verifies the unique active document's exact owned source path instead; the subsequent native sequence passes. The earlier failed run remains private.

The official [0.7.0 portable release](https://github.com/storytold/cadcraft/releases/tag/v0.7.0) was extracted into an ignored test directory after checking its published SHA256SUMS. The GUI executable is independently pinned to `d92dd49cfc81521480d65c30d0abbc91b12b28be1fe95e0586c63d8d6c8544f5`. No installed application or user profile was replaced. The example builds and passes Clippy with warnings denied. Raw protocol traffic, sources, settings and logs remain private in ignored `target/`.

To reproduce, provision the checked portable runtime under `target/cadcraft-lisp-probe/runtime/cadcraft-0.7.0-windows-x64-portable`, build `owned_cadcraft_live_probe`, and run it with `HAZLOMEXA_CADCRAFT_LISP_PROBE=1` on an unlocked desktop. Without that flag, its existing pinned 0.5.0 probe remains selected.

## Remaining implementation boundary

This is an actual native document-lifetime and target-state proof, not an integrated Hazlo flow. The token and snapshot are test-only bookkeeping; they are not an authentication mechanism. Production dispatch still needs verified process/endpoint ownership, bounded typed operations and observations, approval binding to the captured state, complete required drawing-state validation, independent result verification, uncertain-outcome recovery and native/provider/installed acceptance. The probe does not establish whole-drawing state binding or protection against another actor changing its LISP variables.

The [0.5.0 protocol findings](cadcraft-live-contract-2026-10-15.md) remain valid for that release. The 0.7.0 result changes the next action: investigate a narrowly scoped native AutoLISP adapter rather than assuming a custom CADCraft build is necessary. Do not expose arbitrary model-written LISP or advertise saved-copy work as live editing.

## Expanded state guard and verified counterexample

The next actual 0.7.0 run expands the same-call snapshot to all entities enumerated by native `entnext`, the native `LAYER` table projection, source `INSUNITS` and current `CTAB`. It passes refusal after switching to the other owned document, adding an unrelated circle, changing source units, and locking the target layer. Each refused attempt preserves the independently inspected current geometry. Native Undo restores the intervening circle and layer changes, and the original metric/inch edit, follow-up, two Undo steps and reopen-refusal sequence still passes.

The broader check also establishes a concrete limit rather than assuming that these projections represent the complete drawing. Changing layer 0's lineweight to 0.5 mm changes the full native `layer` command result, but leaves both `drawing.inspect.layers` and the AutoLISP snapshot identical. A **read-only** snapshot comparison therefore still matches; no guarded mutation is attempted against that changed layer. Native Undo restores the exact full layer result.

The first counterexample attempt expected `drawing.inspect.layers` to expose lineweight and failed safely. Inspecting the [native layer reader](https://github.com/storytold/cadcraft/blob/v0.7.0/crates/engine/src/cmd/layer.rs) identified the separate full layer command; the corrected probe confirms its changed result and unchanged reduced projections. The [AutoLISP table reader](https://github.com/storytold/cadcraft/blob/v0.7.0/crates/engine/src/lisp/drawing.rs) likewise omits lineweight from its layer records. The passing probe explicitly retains `whole_drawing_guard: false` and `unobserved_lineweight_counterexample: true`.

This closes the tested tab/non-target/unit/layer-lock cases, while contradicting complete-state binding through the present snapshot. Another host read of the full layer command before the edit would leave the same check-to-write gap. A complete native precondition or an equally complete native snapshot remains necessary before registering production live writes. No app runtime selection, chat capability, approval, model behavior or installer is changed. The expanded example builds, passes Clippy with warnings denied, preserves saved source bytes and stops its owned editor. Earlier failed attempts and final raw evidence remain private.
