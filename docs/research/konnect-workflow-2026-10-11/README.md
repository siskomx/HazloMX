# Konnect shared backend workflow

October 11, 2026. Actual Konnect 0.13.0/KiCad 10.0.7 backend approval, publication and recovery pass for a typed component-value edit on a captured schematic copy. Native/frontend acceptance and broader editing remain unfinished.

`Workflow::propose_konnect` captures KiCad sources and dependencies, validates the selected reference/current value from the captured schematic, and binds the typed edit to schema 9 project/job history. Pending runtime changes are refused. Shared one-use approval, owned execution, validated whole-directory nonreplacing publication and recovery reuse existing workflow boundaries. Reports and original/edited schematic artifacts are individually validated before opening.

The [captured-copy adapter](../konnect-adapter-2026-10-11/README.md) provides exact runtime/tool-schema checks, official SDK get/edit/get, complete schematic preservation verification and independently observed shutdown. The backend additionally restores source-bound history and checks the complete output directory before publication or recovery.

## Actual acceptance evidence

[Status/source hashes](status.json) and the retained project bind the final actual test. It exercises:

- Refusal of a stale current value before proposing, denial of cancelled approval, and single-use approval.
- Schema 9 typed serialization and pending Konnect/KiCad runtime-change refusal.
- R1 10k to 12.34k editing with inch report context, successful publication and validated original/edited/report artifact roles; unrelated artifact roles are refused.
- An occupied destination under mil report context remains intact. Verified staging is retained and publication failure stays distinct from generation/verification success.
- Cancellation after approval but before dispatch publishes nothing. Original selected source and prior completed outputs remain byte-identical.
- Reopening recovers four runs: one Completed, two Cancelled and one verified publication failure. Completed artifacts remain verifiable.

The first run caught an audit-version guard still limited to schemas 2 through 8 and failed proposal saving with `Versión e historial de ejecución inconsistentes.` The guard now includes schema 9. A separate regular proposal-only test covers exact schema serialization/recovery and stale current values; it uses runtime path fixtures without dispatch and is not runtime evidence.

Current suite: 76 regular tests pass, 24 separately scoped ignored entries. This actual workflow test runs explicitly. Desktop/custom-protocol Clippy passes.

```powershell
rtk cargo test -p hazlomexa --lib
rtk cargo test -p hazlomexa --lib actual_konnect_workflow_publication_and_recovery -- --ignored --nocapture
rtk cargo clippy -p hazlomexa --all-targets --features desktop,custom-protocol -- -D warnings
```

## Remaining scope

Frontend/native commands, runtime/source pickers, conversational edit proposals/agent inputs, user-visible history/results and native/installed acceptance remain unfinished. Typed editing currently handles selected schematic component values; broader schematic transformations and PCB live IPC editing are still required. Dedicated native KiCad checks in the adapter, live cancellation/timeout and lost-response reconciliation remain unverified. The final installer remains outstanding; the full application goal stays active.
