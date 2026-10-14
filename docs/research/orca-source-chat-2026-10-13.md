# OrcaSlicer source-first chat — October 13, 2026

The production conversation now accepts an STL and saved machine/process/filament profiles before asking the user for physical scale or bed settings. Source selection is in Connections. No units, bed temperature or model dimensions are populated by a form or invented by discovery.

## Implementation

`discover_slice` reuses the existing native file pickers and validated STL/profile readers. Discovery returns triangle count and bounds in **unitless file coordinates**, actual inherited machine/process/material settings for the material's supported beds, and a fingerprint of the STL plus every captured profile source. This preflight is Hazlo's file inspection, not an observation of Orca's open document. It creates neither an engineering job nor a sliced output and does not rewrite the selected profiles.

The new `inspect_selected_slice_sources` tool refreshes the opaque project/folder-bound selection, refuses changed sources and tells the agent to clarify purpose, source units and genuine missing settings in plain language. STL has no declared physical unit; profile dimensions and temperatures retain their existing mm/Celsius meaning. An absent material temperature is refused rather than filled with a guessed value.

`propose_selected_slice` accepts only that selection and explicit source units, bed and temperature. Paths and arbitrary profiles cannot come from the agent. The shared capture compares the complete raw-source fingerprint, checks cancellation and project/runtime binding, then creates the existing one-use human approval. Native Orca CLI execution and independent saved 3MF/Gcode checks remain the effect boundary. Existing verified-result follow-ups retain model geometry, machine, process and material. No upload, printing, GUI mutation or hardware operation is exposed.

The chat sends the selected source capability to both Codex and Claude. Selecting another model omits the previous job from that source's initial context. It labels the STL as having no declared unit; inspection and conversational clarification establish the scale for a proposal. Approval stays in Hazlo and is never available to the agent. The shared catalog now contains **22 tools**.

## Verification

- Full library suite: **126 pass, 48 ignored**. New tests cover unitless bounds, unmodified profiles, inherited-source fingerprint changes, missing temperatures, cancellation, copied-project folder refusal and strict request fields. The first fixture assertion incorrectly assumed an origin-centered cube; discovery correctly reported its actual 100–110 coordinate offset and the assertion was corrected.
- `cargo check --bins --examples`, production frontend build and `tests/chat-flow.mjs` pass. Browser IPC fixtures cover both agents, foreign-project refusal, source context without invented values, no automatic approval and replacement of a previous result's context. They do not run native pickers or actual agents.
- Explicit native test `actual_selected_slice_native_workflow_and_source_boundaries` passes four actual Orca jobs: metric/inch first slices and temperature follow-ups. It also covers changed-profile refusal before a job, cancelled/consumed approval, original/prior-output preservation and recovered completed results.
- The rebuilt `installed_orca_workflow` harness passes the same four-job sequence through the real `hazlomexa-tools` MCP transport with stock inherited Prusa profiles. Both 10 mm cube cases yield 50 layers; readback is 10 × 10 × 10 mm for the metric source and approximately 9.99999237 × 9.99999237 × 10 mm for the inch source. Both follow-ups read back 50 °C. Actual inspected sources require scale confirmation, and initial proposals require human approval.
- Tested debug helper SHA-256: `e11f7c98ce238017814d69518be2d6f0b36c03f6e70cbaf6ba4e9d93627010d2`. Raw logs and test projects remain private. This harness uses fixture approval and linked backend code; its name does not establish installed main-window acceptance.
- The protocol harness's read-only `--catalog` reports 22 unique tools. Package preparation now derives its expected count from that linked harness instead of assuming seventeen; Python syntax validation passes. Package preparation and a new guest installation have not run for this slice.

Native UI automation remains stopped after Escape. Native source pickers, real signed-in clarification/tool turns, current installed acceptance and broader profile/model coverage remain unfinished. Saved-file slicing is explicitly scoped; a live Orca editor connection is not implemented. The full application goal stays active.
