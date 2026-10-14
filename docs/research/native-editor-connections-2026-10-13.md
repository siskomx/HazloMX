# Native editor connection boundary

October 13, 2026. This follows the chat-first correction and CADCraft native-command slice. An actual selected native document and fresh program readback are required; saved-copy tests do not establish the live gate.

## Evidence affecting the implementation

- CADCraft 0.5.0's [control handler](https://github.com/storytold/cadcraft/blob/315751e3ec98e708bcc831dd7f80ca0433a2e3d0/crates/ui-egui/src/control.rs) dispatches commands to the active session without a conditional document/revision guard. A separate inspection followed by a command does not prove that the same document stayed active. The existing native command plans retain their saved-copy scope.
- KiCad's [IPC documentation](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/) distinguishes PCB-only IPC in versions 9/10 from later schematic support. The selected KiCad 10.0.7 connector must keep schematic saved-file operations separate from live PCB operations. Native plugin actions supply an endpoint and instance token; this is the intended handoff for a verified editor session.
- Konnect's [PCB helpers](https://github.com/mixelpixx/Konnect/blob/6bbe3e4f890ba1d37c0e5d5f38ccd03d90958c9e/crates/konnect-core/src/tools/pcb_components.rs) include exact-board IPC queries. `get_component_list` is IPC-only; `get_board_info` can answer from disk and needs an explicit source check. Hybrid mutation tools need a live-only execution boundary: checking a response after a write cannot undo a file fallback.
- Konnect's [socket implementation](https://github.com/mixelpixx/Konnect/blob/6bbe3e4f890ba1d37c0e5d5f38ccd03d90958c9e/crates/konnect-ipc/src/socket.rs) records why raw stream/pipe probes can wedge KiCad's NNG endpoint. Hazlo uses the identified native client/protocol, with no raw endpoint probe.
- Inspected CADPilot RPC supports named documents but also broad Python execution; approved native dispatch still needs actual document/session binding. Arduino Agent's inspected sketch content calls read/write disk and alone do not establish unsaved-buffer support. OrcaSlicer's current connector remains file slicing.

## Implemented read-only PCB path

The production chat has optional KiCad IPC setup and a native PCB picker. `discover_live_pcb` uses the identified Konnect/KiCad runtimes and existing owned-process lifecycle. The twentieth host tool, `inspect_selected_live_pcb`, refreshes that opaque selection without giving the agent an endpoint, token or source path.

Only a local absolute Windows `ipc://` endpoint and two identified native read tools are allowed. Their actual input schemas are checked. A saved-file `get_board_info` response is refused before querying footprints. Two complete observations must match; incomplete counts, duplicate references, nonfinite/out-of-range coordinates, over 256 components and excessive size are refused. `components_sha256` binds this component projection, not a full PCB revision or atomic whole-document snapshot. Native process/instance identity across restart remains unverified.

The token stays in memory and the owned child's environment; it is not written to native configuration, exposed to the agent or stored in conversation history. Private configuration can contain the selected endpoint/path and remains outside publication. Live selection revokes the old saved-PCB capability. Returning to saved editing requires an explicit saved-source selection. The chat grants no live mutation and omits an unrelated saved job. File content remains untrusted data.

Initial picker inspection has a bounded deadline but no separate UI cancel handle yet; agent refreshes use MCP cancellation. Native-launched handoff, restart identity, live mutation approvals, uncertain-write reconciliation, native backup/undo checks and GUI acceptance remain unfinished. This connection must not become a live-write approval capability without those boundaries.

## Verification

- Full library suite: **123 pass, 47 ignored**. New units cover local endpoint validation, complete projections, privacy, copied-project folder refusal and saved/live capability separation.
- `rtk cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib actual_live_pcb_read_refuses_unreachable_editor_without_fallback -- --ignored --nocapture`: passes with actual identified Konnect and a fresh explicitly nonexistent NNG address. It proves fallback refusal, unchanged source, no job/history and no token in private native files. It never discovers or contacts the open editor. An initial test-only privacy scan hit the exclusive project ownership lock; scanning the native profile files passes.
- Existing actual `actual_konnect_workflow_publication_and_recovery` and `actual_konnect_pcb_workflow_publication_and_recovery` tests pass with backend approval fixtures after the shared dispatcher change.
- Both desktop binaries compile. Production frontend build and `tests/chat-flow.mjs` pass. Mocked browser IPC covers native/saved switching, read-only context, no automatic approval, token exclusion and input clearing. It does not run a real agent/editor.

No positive live KiCad observation, native UI action, signed-in conversation, live edit or replacement installer is claimed. Native UI automation remains stopped after Escape. No hardware operation ran. The full application goal and remaining program/installer gates stay active.
