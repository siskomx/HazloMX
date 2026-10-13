# Konnect desktop workflow source

October 11, 2026. The eighth integration is connected to the Spanish desktop UI and bounded native commands. Browser fixtures and the actual shared backend pass; native desktop/installed acceptance remains unfinished.

Choose **Editar esquema · Konnect**. Select identified Konnect 0.13.0 and KiCad CLI 10.0.7 runtimes. Enter the component reference and its new value, or use the local chat command **cambia R1 a 12.34k** / **change R1 to 12.34k** to fill those inputs. Unsupported phrasing receives a clarification; it does not dispatch a model or a tool. This is a typed local conversational input path, not general agent reasoning or live native editing.

Select the source project folder and schematic through native pickers. The backend reads the actual current component value; the user need not know it. The subsequent capture checks that the selected reference/current value still agrees before proposing the change. The preview shows the exact file, reference and current → new value plus captured dependencies. Pending controls are locked; changing a proposal requires cancellation and a fresh proposal. Shared approval dispatches the existing validated copy adapter.

Published results/history expose three separate validated actions: show Original schematic, show Edited schematic, and read the Konnect report as literal text. Schematic actions explicitly select the verified files in Explorer. They do not implement a live KiCad IPC session. Selected edits are restored into controls when loading job state. Existing source/history formats remain compatible with schema 9.

Component values are electrical/text values, not length measurements. This workflow does not offer an unused length-unit selector; physical coordinates remain unchanged. Existing geometric workflows retain their metric/imperial inputs and conversions.

## Evidence

- [Status and exact source hashes](status.json), [actual project file hashes](project-hashes.json) and the retained project bind the final actual shared-workflow run through `propose_konnect_value`, which automatically reads the current value. It retains the [approval/publication/recovery scope](../konnect-workflow-2026-10-11/README.md).
- `rtk proxy node tests/design-flow.mjs` passes eight-workflow browser fixtures: runtime gates, local chat-filled reference/value, mm native context, exact proposal payload, current/new preview, locked inputs, explicit approval, three artifact roles and literal report escaping with no model dispatch for the integration workflows. The first Konnect report-dialog fixture check failed because the mock did not return a Konnect report; the missing mock case was added and the full fixture passed.
- [Browser fixture screenshot](konnect-workflow.png) was visually inspected. This is mocked IPC, not native acceptance.
- Current Rust suite: 76 regular tests pass, 24 separately scoped ignored entries. The real Konnect workflow test passes explicitly. Desktop/custom-protocol Clippy and TypeScript/Vite build pass.

Native command names, build manifest and capability permissions are connected together; the frontend cannot pass arbitrary MCP tool names or arbitrary artifact paths. Original source and prior results remain preserved by the backend validators.

## Remaining scope

Verify real native runtime/source pickers, approval/results/restart and installed/second-environment behavior. Live execution cancellation/timeout, uncertain-write recovery, dedicated native KiCad checks, broader schematic transformations and PCB live IPC remain open. General agent-generated inputs and conversational edits across all integrations remain required; the narrow local command above does not complete that requirement. The final installer predates this workflow; the full application goal stays active.
