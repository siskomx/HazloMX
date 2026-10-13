# FreeCAD and Arduino through shared agent tools

The MCP surface now has six tools: capabilities, status, selected-sheet inspection, selected-component value proposal, rectangular-solid proposal and Arduino compile proposal. Capabilities explain current shape/board limits and instruct the agent to gather intent and missing measurements in plain language. New geometry accepts explicit mm/cm/m/in/ft, converts with the existing scale factors and validates the shared millimeter range. Sketches keep the existing Uno/core and 64 KiB boundaries. Neither tool can approve, execute or control hardware.

Both new proposal paths check the selected project under the same Workflow lock used to create the operation. Agent tool input is recorded as `provider: mcp`, independently of untrusted client/model names. New snapshot version 10 binds that provenance; earlier manual/Ollama operation versions remain valid. Recovery rejects downgraded MCP snapshots and version-10 snapshots claiming a manual origin. Desktop picker operations remain manual; pending changes and native execution keep their existing approval/verification boundary.

Passing coverage: all five units; missing inputs/units; out-of-range dimensions; stale projects; unsupported boards; oversized sketches; no returned approval tokens; cancelled token refusal; six-run provenance recovery; source/version downgrade refusal; current browser MCP history label. 82 regular Rust tests pass and 27 separately scoped ignored entries remain. TypeScript/Vite and Clippy pass.

The actual release-helper/pipe test creates an inch-based FreeCAD proposal, receives explicit desktop one-use approval, generates verified files, then proposes/approves/compiles an Arduino Uno sketch. Both completed version-10 MCP results recover after closing the owner. Retained owned root: `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-f6121efb-8595-4a46-8989-6f81957433e6`. The actual schematic value test separately passes version-10 MCP provenance, native Konnect/KiCad validation and recovery at `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-6c95d198-c9ba-4751-8a22-a5d537274836`.

Installed Codex/Claude client protocol gates pass the expanded catalog at `target/codex-probe/804491a2-ce9f-40aa-ab3f-d5716248d449`, without model inference. Codex directly reads/refuses selections; Claude reports a healthy connection. These do not prove model-generated FreeCAD/Arduino proposals or conversational reasoning.

Reproduce after provisioning the identified runtimes and building the release helper:

```powershell
rtk proxy cargo test -p hazlomexa --lib actual_mcp_solid_and_compile_approval_recovery -- --ignored --nocapture --test-threads=1
rtk proxy cargo test -p hazlomexa --lib actual_mcp_proposal_keeps_desktop_approval_and_verification -- --ignored --nocapture --test-threads=1
```

Remaining: embedded natural language chat/provider sessions, model-driven client turns/edits, source-bound agent tools for the other engineering workflows, live cancellation/reconciliation, native UI and updated installer/second-environment acceptance. The current primitive/Uno capability limits remain explicit; the full application goal is unchanged.

Retained [scoped status/source hashes](status.json), [solid provenance](solid-run.json), [native solid result](Solid/result.json), [editable FCStd](Solid/solid.FCStd), [STEP](Solid/solid.step), [compile provenance](compile-run.json), [native compile result](Compile/result.json), [sketch](Compile/Sketch.ino), [expanded Codex catalog](mcp-status.json) and [Claude health check](claude-mcp-list.txt) bind these runs independently of temporary directories. The regular downgrade test explicitly chooses a FreeCAD history entry, so input/version mismatch cannot substitute for the provenance refusal.
