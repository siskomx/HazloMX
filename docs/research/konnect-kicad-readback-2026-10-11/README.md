# Konnect independent KiCad readback

October 11, 2026. New Konnect component-value edits require independent KiCad 10.0.7 netlist readback before publication. Actual adapter and shared workflow tests pass; this is native CLI validation, not desktop GUI/live IPC acceptance.

After official-SDK editing and shutdown, the adapter runs the identified KiCad CLI with fixed `sch export netlist --format kicadxml` arguments on the private Edited project. Runtime executable/DLL/resource-directory pins remain held through execution. It reuses owned Windows process supervision, private configuration/resource paths, cancellation and the 120-second supervisor deadline.

The resulting `Netlist.xml` must be bounded, structurally valid XML with one export root, an identified `Eeschema 10.0.7` producer and exactly one selected component containing the approved value. The parser rejects incorrect values/versions, duplicate references, empty or nested selected fields, incomplete/extra roots and DTD declarations; escaped text values are decoded without external entity expansion. The complete schematic and every companion file must still satisfy the existing independent preservation checks after the CLI runs.

Request/result contract version 2 binds this stronger requirement. New proposals use version 2 and cannot accept a downgraded version-1 result. Earlier version-1 source-bound history remains readable under its original verification scope, with three artifact actions and an earlier-version label. It cannot be dispatched as a new operation without a fresh proposal. Current results expose Original, Edited, Netlist and report actions; netlists are explicitly selected in Explorer through the validated artifact boundary.

## Evidence

- [Status and source hashes](status.json), [actual adapter result](adapter/Stage/result.json), [native adapter netlist](adapter/Stage/Netlist.xml), and the retained shared project/hash inventory bind the current real runs.
- `rtk proxy cargo test -p hazlomexa --lib actual_konnect -- --ignored --nocapture --test-threads=1` passes both actual tests. Shared approval/publication/recovery additionally verifies the fourth artifact role while preserving selected source/prior outputs, an occupied destination and four-run restart state.
- Current regular suite: 77 passed, 24 separately scoped ignored entries. Three regular adapter/verifier tests include native XML tampering/DTD/escaping checks, version-1 legacy inventory acceptance, current version-2 netlist acceptance and downgrade refusal, plus cancellation before dispatch. Four typed schematic-contract tests remain passing.
- Eight-workflow browser fixtures pass version-2 Konnect's four artifact actions and literal report rendering; [updated browser screenshot](konnect-workflow.png). TypeScript/Vite build and desktop/custom-protocol Clippy pass.

Reproduce after provisioning the identified runtimes:

```powershell
rtk cargo test -p hazlomexa --lib
rtk cargo test -p hazlomexa --lib actual_konnect -- --ignored --nocapture --test-threads=1
rtk proxy node apps/desktop/tests/design-flow.mjs
```

## Remaining scope

Native desktop runtime/source pickers, approval/results/restart and installed/second-environment acceptance remain open. Live SDK/CLI cancellation, full production-deadline expiry and lost-response reconciliation remain unverified. Broader schematic transformations, hierarchical-reference fidelity, PCB live IPC and general agent-generated conversational edits remain required. Successful netlist generation confirms native readback, not an electrically correct circuit; ERC/design findings remain a separate workflow. The final installer and full application goal remain unfinished.
