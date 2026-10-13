# Konnect Windows runtime and SDK gate

October 11, 2026. Konnect 0.13.0 Windows x64 and the official Rust MCP SDK 3.5.1 pass the scoped tests below. Application dispatch, approval, publication/recovery and native PCB interaction remain unfinished.

## Identified combination

- [Upstream release](https://github.com/mixelpixx/Konnect/releases/tag/v0.13.0), source commit `6bbe3e4f890ba1d37c0e5d5f38ccd03d90958c9e`.
- [Package/source/executable identity](identity.json): release ZIP 13,354,938 bytes, SHA-256 `9099ad41becbfdbd00263af827a9ebab10d5732a6e42e8dfcff9a1a07dc1f089`; single `konnect.exe` 35,495,424 bytes, SHA-256 `09969ec5f05600039e206acedb0b6b785528f1bced2dbe2c7f426483013e4f2c`. Download digest matches the release API metadata and independent hashing. [Actual Authenticode result](signature.json) is NotSigned; no publisher signature is claimed.
- KiCad CLI 10.0.7 from the [separate signed runtime gate](../kicad-runtime-2026-10-11/README.md); CLI hash is retained in the raw status.
- Explicit private configuration, piped stdin, stdio transport and private APPDATA/state directories. No `init`, installer script, global registration or real KiCad IPC connection is invoked. Upstream guidance discovery may read the real user home through Windows KnownFolder APIs; private environment variables do not prove all reads are isolated.

## Actual file edit gate

[Raw-wire status](raw/status.json), [initialization](raw/initialize.json), [dynamic catalog](raw/expanded-tools.json), [wire](raw/wire.json), [selected component/readback](raw/schematic-edit.json) and [editor-lock refusal](raw/lock-refusal.json) retain the actual results. This bounded raw-wire probe is a fixture, not the production MCP client, and does not use Windows Job ownership.

The server identifies itself as Konnect 0.13.0 with protocol `2025-06-18` and list-change notifications. Initially 21 tools are present; loading `sch_components`, `sch_analysis` and `sch_export` yields 64, and unloading components yields 46. Names and input schemas come from the actual catalog.

The selected fixture R1 value changes from 10k to 12.34k on an owned schematic copy. Before/after component catalogs and an independent complete schematic syntax-tree comparison permit only the approved Value change, including numeric spelling normalization. A held KiCad editor lock returns a conflict and preserves the schematic and lock. An independent KiCad CLI XML [netlist](raw/Edited.net) confirms R1's value. All four originals and the three unchanged companion files remain byte-identical. The raw server exits successfully after closing stdin.

## Actual SDK and process ownership gate

[Official SDK status](sdk/status.json) and [SDK catalogs](sdk/catalogs.json) confirm the same negotiated protocol, tool counts and two list-change notifications through `rmcp`, rather than a handwritten production client. The owned child uses `process-wrap` 10.0.1 JobObject, KillOnDrop and CreationFlags wrappers. Job membership is observed through Windows `IsProcessInJob`; after explicit session cancellation, a pinned Windows process handle independently confirms server cessation. This SDK probe calls catalog/toolset tools only, not document mutation tools.

The regular `process::tests::sdk_stdio_close_stops_owned_tree` test additionally starts a real parent/descendant fixture through the SDK transport and independently observes both processes cease after transport closure. The [first immediate descendant assertion](initial-shutdown-assertion.log) failed because termination had not yet been observed; the corrected check bounds independent cessation observation to five seconds, rather than assuming that returning from `close` proves descendant termination. The final test passes. This fixture establishes SDK process-tree cleanup, not native KiCad editor shutdown.

Current validation: 69 regular Rust tests pass, 22 separately scoped ignored entries; desktop/custom-protocol Clippy passes. Two independent verifier self-checks cover selected-value edits, other-component/placement/wire tampering, missing references and malformed document delimiters. [Source hashes](source-hashes.json) bind the gate code and dependencies.

Reproduce after provisioning the identified KiCad fixture/runtime and Konnect package:

```powershell
rtk proxy python -m unittest discover -s tests/konnect -p test_*.py
rtk proxy python tests/konnect/runtime_probe.py
rtk cargo run -p hazlomexa --example konnect_protocol
rtk cargo test -p hazlomexa --lib sdk_stdio_close_stops_owned_tree
```

## Typed edit contract and SDK mutation follow-up

The Rust `konnect::ValueEdit` contract now validates reference/current/new value and rejects stale state. `verify_value_edit` parses the entire schematic with bounded bytes, elements and depth, normalizes decimal numbers exactly without floating-point rounding, and permits only the selected root component Value fields to change. It rejects unrelated changes, duplicate/missing properties and invalid structure. This is adapter contract code, not an exposed app editing command.

The [SDK edit status](sdk-edit/status.json), [request and readback](sdk-edit/schematic-edit.json) and [source hashes](sdk-edit/source-hashes.json) retain a subsequent actual run. The official client verifies the exact identified get/edit input schemas, reads R1's current 10k value, dispatches the typed 12.34k edit once, reads it back and passes the Rust full-document verifier. Original/companion bytes remain unchanged and the pinned server handle confirms cessation after session cancellation. The earlier catalog-only SDK evidence above is retained separately.

Four regular contract/verifier tests cover the real fixture, unrelated placement/wire/reference edits, decimal normalization beyond f64 integer precision, stale values, malformed data, duplicate properties and byte/depth limits. Current regular suite: 73 passed, 22 separately scoped ignored entries; desktop/custom-protocol Clippy passes. Reproduce the updated SDK gate with the same example command above. No native IPC or shared app approval/publication/recovery success is inferred from this edit gate.

## Remaining implementation

Implement the typed schematic-copy adapter using the official SDK and owned process wrapper, fresh catalog/schema verification, approval-bound source/reference/value, independent readback, shared nonreplacing publication and recovery. Uncertain writes must be reconciled from captured copy state without blindly retrying a mutation. Live cancellation/timeout and lost-response evidence remain required.

This gate does not certify broader schematic editing, PCB live IPC/native editing, native app pickers/results, metric/imperial editing previews, installed/second-environment acceptance or a new installer. The existing installer predates these changes. The full application goal stays active.
