# Measured CADPilot change proposals through shared tools

October 12, 2026. `propose_cad_document_change` connects the existing selected-copy CADPilot workflow to the shared MCP host. The agent starts from a completed CADPilot job and the report hash returned by `read_verified_result`, reads actual catalog/current dimensions, clarifies the target and missing values, then submits a typed operation. Workflow status now identifies the integration for each job/history entry; capabilities describe CADPilot's verified operations, units and limits separately from new rectangular solid generation.

Only existing inspect/edit-box/add-box/add-cylinder operations are accepted. Project identity, completed job kind and report hash resolve the source privately. Inspection jobs use their verified original; editing jobs use their verified edited FCStd. The file remains pinned during capture, captured source hash is bound, and captured addon modules retain their upstream manifest identity. The existing shared preparation measures the copy again, validates the target and prepares an explicit source-bound approval preview. Human approval and native verification/publication remain unchanged. The tool receives no arbitrary source/addon path, Python, approval token or hardware action.

Actual FreeCAD 1.1.4 / CADPilot 0.7.7 / release helper / official rmcp 3.5.1 gate passes:

- A desktop-owned inspection establishes the actual `Target` native box and 30 mm height.
- MCP rejects a stale report hash and prepares an edit using centimetres (1 × 2 × 3.5 cm), with actual before measurements and schema-10 MCP provenance.
- The human workflow's one-use approval dispatches the edit; verified readback confirms height 35 mm and volume 7000 mm³.
- A subsequent MCP proposal uses the verified edited document and measures 35 mm as its starting height. That proposal is cancelled; original selected bytes remain unchanged.
- An official SDK cancellable request is stopped after observing a live FreeCADCmd measurement process. Independent PID observation confirms process cessation in 36 ms for this fixture. No additional proposal is left; published history is preserved.
- Reopen recovers three runs: the original approved inspection, completed MCP edit and cancelled follow-up.

MCP request cancellation now signals the existing native capture/measurement cancellation flag. This extends owned-process cancellation to proposal preparation; it does not replace the existing request/project checks or cleanup of late unapproved proposals. The SDK cancellation gate is not a measurement of desktop stop-button latency or full production deadlines.

Validation: 87 regular tests pass (29 separately scoped ignored runtime/guest gates); the actual measured CAD tool gate passes in 4.92 seconds. All-target desktop/custom-protocol Clippy, TypeScript/Vite build and eight-workflow browser fixtures pass. Installed Claude Code 2.1.285 passes the expanded ten-tool catalog's credential-free initialization/error boundary in 13.51 seconds, without an authenticated model turn.

Runtime gate root: `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-f4fd7e8b-afcc-488a-a570-adf8a473dca7`. Claude gate: `target/claude-stream-probe/5bd0c674-a34a-47f0-bbfa-b417a55bdc2a`. Proposal/result/cancellation evidence and source hashes are retained beside this file. Temporary client configuration, access tokens and credentials are excluded.

Pending: signed-in natural-language clarification/tool selection, native desktop UI acceptance of this route, installed workflow, broader document fidelity and live editing of open native documents. The full application goal remains open.
