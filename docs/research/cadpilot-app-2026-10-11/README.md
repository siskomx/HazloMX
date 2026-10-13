# CADPilot application workflow

October 11, 2026. App source, seven-workflow browser fixtures and real shared backend checks pass against identified CADPilot 0.7.7 primitives with FreeCAD 1.1.4 Windows x64. Native desktop/installer acceptance remains unfinished.

Select **Documento existente · CADPilot**. Start with **Medir e inspeccionar primero** when dimensions or internal object names are unknown. Select the original FCStd and the identified CADPilot folder containing `rpc_server`. The backend captures sources, runs a read-only inspection on a private copy and presents the actual object catalog, millimetre dimensions and volume, plus metric/imperial display conversions. No AI model is dispatched.

To edit a box or add a box/cylinder, choose the measured internal object name or a new name, enter dimensions/position and units, then inspect the proposal. The preview includes initial geometry and the new parameters in their entered units and normalized millimetres. Pending fields are locked; changes require cancellation and a fresh proposal. The original is preserved.

Approval binds captured source/module hashes, project/job, measured catalog/geometry and typed operation/parameters. Execution rechecks the measurements before editing the working copy. Changing the FreeCAD runtime while a CADPilot proposal is pending is rejected because the inspection must be repeated; the existing new-solid FreeCAD approval behavior is retained. The [fixed adapter](../cadpilot-adapter-2026-10-11/README.md) provides pre-load document validation, enabled-undo rollback, native parameter readback, native reopening and STEP checks. Rust additionally verifies saved request bytes, geometry invariants, target volume, sources, output hashes and the complete allowed file inventory.

Schema 8 extends the existing project snapshots without rewriting earlier runs. Shared one-use approval, supervision, whole-directory nonreplacing publication, cancellation-before-dispatch and recovery are reused. Inspection publishes the original and read-only report; mutating operations additionally expose FCStd/STEP actions. Reports render as literal text. Unsupported GUI-state/object/dependency cases have specific Spanish errors instead of exposing arbitrary producer traceback text.

## Evidence

- [Actual test status and source hashes](status.json), [stdout](stdout.log), [stderr](stderr.log). Reproduce with `rtk proxy python tests/cadpilot/verify_adapter.py`. Both actual tests pass serially: owned adapter and shared workflow.
- Shared workflow exercises inspection, inch-based box editing, centimetre-based box creation and millimetre-based cylinder creation with measured previews. It rejects stale/used approvals and pending runtime changes. All expected artifact roles are verified; original and prior output bytes remain unchanged. An occupied publication destination remains intact while verified staging is retained. Cancellation before dispatch publishes nothing. Reopening recovers seven runs: four Completed, two Cancelled and one verified publication failure.
- [Closed project file hashes](project-hashes.json) retain the exact actual shared-workflow fixture. The separate adapter test deliberately tampers with its final STEP to prove rejection; that private stage is not clean publication evidence.
- Browser fixtures pass inspection without known dimensions, measured names/dimensions and imperial display, numeric inch edits, locked fields, two/four artifact actions, literal report escaping and no model dispatch. [Fixture screenshot](cadpilot-workflow.png) was visually inspected; this is mocked IPC, not native acceptance.
- Current regular Rust suite: 68 passed, 22 separately scoped ignored entries. Desktop/custom-protocol Clippy and frontend build pass. Two policy tests and nine real fixed-macro cases also pass after the current inspection/report limits were applied.

## Remaining scope

The first file adapter supports at most 32 independent native primitive objects and rejects GUI persistence, Python objects/properties/extensions, expressions, linked documents and other uncertified resources/dependencies before loading. Actual operation fixtures cover boxes/cylinders; broader native types and document fidelity need additional gates. This includes ordinary GUI-saved FreeCAD documents, PartDesign histories and assemblies: their rejection is an unfinished capability, not full CADPilot support.

Read-only proposal preparation currently runs under the command's busy state with an owned timeout; dedicated import/inspection cancellation is unfinished. Live execution cancellation/independent cessation and full production-deadline evidence, lost-response recovery cases, constrained interactive RPC/native addon behavior, native pickers/approval/results/restart and installed/second-environment acceptance remain open. The certified installer predates CADPilot. The full application goal remains active.
