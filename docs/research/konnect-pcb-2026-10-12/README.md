# Konnect PCB move and native readback gate

October 12, 2026. The typed `FootprintMove` contract, full-board verifier and `konnect_pcb_protocol` example now pass an actual Konnect 0.13.0 closed-board move through the official Rust MCP SDK, followed by KiCad 10.0.7 checking/export and saved-copy readback. This is integration foundation evidence; PCB editing is not yet connected to the application's proposals, chat, publication or recovery.

## Identified interface

The [pinned Konnect source](https://github.com/mixelpixx/Konnect/blob/6bbe3e4f890ba1d37c0e5d5f38ccd03d90958c9e/crates/konnect-core/src/tools/pcb_components.rs) provides `move_component` with live IPC preferred and a guarded closed-board fallback. The gate uses the [previously identified release/runtime](../konnect-runtime-2026-10-11/README.md), executable SHA-256 `09969ec5f05600039e206acedb0b6b785528f1bced2dbe2c7f426483013e4f2c`, protocol `2025-06-18` and rmcp 3.5.1. It explicitly configures an unreachable owned IPC endpoint and edits an owned fixture copy. No real user's board or configuration is selected.

The actual loaded [catalog](catalogs.json) matches the exact `move_component` and `get_board_info` input schemas, including `additionalProperties: false`. Before/after metadata confirms file readback, 29 layers, two copper layers and six nets. [Actual move and inventories](pcb-edit.json), [missing-target refusal](refused.json) and [checked status](status.json) retain the exchange. Job Object membership and server cessation are independently observed through pinned Windows process handles.

KiCad 9/10's [IPC API](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/) requires a running PCB editor and is distinct from CLI export/checking. This gate does not establish live IPC, unsaved-editor readback, busy state or editor-lock behavior.

## Exact operation and verification

The source fixture J1 moves from `(200.8, 156.6)` to `(203.34, 155.33)` mm: +0.1 inch in X and −0.05 inch in Y. Its UUID is bound alongside its reference and current position. The board inventory supports current property-style references/UUIDs and the fixture's legacy text-style references/timestamps; ambiguous, duplicated, missing or nonfinite identities/positions are refused. Pre-6 module syntax returns an upgrade message.

The existing bounded S-expression parser is reused for schematic and PCB documents. The whole-board comparison permits only the selected footprint's root X/Y change, with equivalent zero-angle spelling allowed. Pads, tracks, nets, zones, other footprints, orientation and all unrelated syntax-tree content must remain unchanged. Decimal spelling is compared exactly after normalization, rather than accepting broad floating-point tolerances. Konnect writes X/Y with six decimal places in mm; destinations requiring finer precision and physical no-ops are refused. Arithmetic noise from unit conversion can be normalized at that precision without accepting unreviewed rounding.

The first schema check missed Konnect's generated `additionalProperties: false`; it failed before mutation. A subsequent exact verifier caught float arithmetic noise in the requested Y coordinate. The corrected precision contract and regression preserve that rejection boundary rather than accepting unrelated board differences. Intermediate runs remain under `target/konnect-probe/pcb-sdk-*`.

## Native KiCad result

Final gate: `target/konnect-probe/pcb-sdk-c66d6599-332b-4807-b99e-726be425bb35`.

Both before and after copies pass the existing captured-project KiCad adapter and complete artifact validator. It performs DRC with zone refill/save and generates Gerber/drill outputs under private profiles. The [baseline](native-Before.json) has zero errors and six warnings. The [edited board](native-After.json) has one missing-connection error and seven warnings, including a newly dangling track. The baseline already contains an isolated-copper warning and five library-footprint mismatch warnings; disabled checks remain explicit. These counts use the application's refill/save path, unlike the older standalone no-refill runtime fixture.

Actual saved `Checked` PCB inventories equal the corresponding pre-check inventories for every footprint's reference, UUID, library, X/Y and rotation. The original input directory and all edited companion files are independently rechecked byte-for-byte after both native checks; the edited PCB differs only in the intended root position. KiCad's refilled/saved checking copies remain separate from the exact Konnect-edited board, since refill and native serialization legitimately change more than placement. Retained [before](BeforeKiCadCheck/Check.json) and [after](AfterKiCadCheck/Check.json) reports, saved boards, exports and [file hashes](file-hashes.json) preserve those scopes. Successful edit/check/export is not a manufacturing-readiness claim.

## Tests and remaining work

Seven Konnect unit tests pass, including whole-board pad/other-footprint/net tampering, stale position/UUID, sub-nanometer inputs, physical no-ops, unit-conversion noise, malformed/ambiguous identities and legacy-module refusal. Existing schematic checks remain intact. The full regular Rust suite passes 104 tests with 30 separately scoped ignored entries; desktop/custom-protocol Clippy and formatting pass.

Reproduce after provisioning the identified runtime/fixture directories: `rtk cargo run -p hazlomexa --example konnect_pcb_protocol`.

Next required work: approval-bound captured PCB requests, application job/schema/artifact/recovery integration, component discovery and conversational before/after preview, shared agent tools, native UI and installed workflows. Live native IPC needs separate exact-board/unsaved-state, editor-busy/lock, interruption, uncertain-write reconciliation and saved-result gates. The full application goal remains active; no PCB app-support or live-editing completion is claimed here.
