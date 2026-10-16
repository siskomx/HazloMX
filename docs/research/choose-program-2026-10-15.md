# Choose the program before editing — October 15, 2026

Hazlo's production chat now starts with “Ayúdame a elegir” instead of assuming FreeCAD. The person can describe an idea and its purpose before knowing the software, technical identifiers or measurements. The AI can compare the existing capabilities, ask what matters for the choice, and explain when a route edits an open document or produces a separate saved copy.

The existing program selector confirms the agreed choice. A visible chat button opens and focuses it; no engineering form or new tool is introduced. The conversation remains available when the person selects a program, and the existing provider connections enforce that program's scope on the next request.

The backend's new `planning` scope advertises only `get_design_capabilities`. It refuses every document inspection, result read, workflow-status call and proposal, including direct calls to tools outside the advertised catalog. Its capabilities include all existing program groups; its model context contains program names and explicitly requires selection, with no selected-source capability or current job. Picking a program enables only that program's existing tool scope and approvals.

The shared KiCad capability response explicitly identifies its open-document interface as PCB-only. This matches the [official add-on documentation](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/), which limits stable KiCad 9/10 IPC plugins to the PCB editor. It does not turn the existing saved schematic workflow into live schematic editing. Existing Orca slicing and CADCraft saved-copy limits remain explicit in their capabilities.

Validation passes:

- The actual authenticated MCP pipe advertises exactly one planning tool, returns cross-program capabilities, refuses all other catalog calls and leaves job/history empty. Existing six engineering scopes continue to pass their advertisement, refusal and approved-proposal checks.
- The Ollama batch check includes planning and rejects an unauthorized batch before any proposal.
- The production browser fixtures cover the initial unselected mode, four-provider planning context and selection-button handoff, followed by the existing six-program approval/review checks. No approval occurs during planning.
- All 175 regular desktop Rust tests pass; 58 environment-dependent tests remain ignored. The final KiCad capability metadata assertions also pass the focused MCP test. Clippy with warnings denied, production frontend build, bounded-context checks and eleven-scope conversation-history checks pass.

The first browser attempt exposed missing early returns in the new frontend mode; those are fixed. One run was invalidated by editing the Vite-served source during its execution; the stable rerun passes. A history fixture then exposed the missing explicit null current-job marker; restoring that marker preserves the existing history contract, and both history and full chat fixtures pass afterward.

These are backend and browser fixture results. Real-model software selection, native Hazlo UI, ordinary-user installation and the updated installer are not established here. The separately passed [native KiCad conversation](kicad-live-conversation-2026-10-15.md) and [Arduino conversation](arduino-live-conversation-2026-10-15.md) cover edits after a program is selected. The full multi-program goal remains active.
