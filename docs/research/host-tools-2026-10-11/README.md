# Shared host tools and actual MCP workflow

Source: `apps/desktop/src-tauri/src/host_tools.rs`, desktop discovery callers and pinned `rmcp 3.5.1` server feature. No second Workflow owner or speculative provider framework is introduced.

Passing commands:

```powershell
rtk cargo test -p hazlomexa --lib
rtk proxy cargo test -p hazlomexa --lib host_tools -- --nocapture
rtk proxy cargo test -p hazlomexa --lib actual_mcp_proposal_keeps_desktop_approval_and_verification -- --ignored --nocapture --test-threads=1
rtk cargo clippy -p hazlomexa --all-targets --features desktop,custom-protocol -- -D warnings
rtk proxy node apps/desktop/tests/design-flow.mjs
```

Regular suite: 80 passing, 25 separately scoped ignored. Actual MCP workflow: one passing test, 11.89 seconds. The retained owned test root is `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-7a7c9a2d-eb6c-4487-8531-3ea635abdf6e`; project `4061bdd0-a119-479b-bc1b-51b4353ebb3d`, run `99ddb1d4-a702-44c8-b5b4-1fdc9966f9ca`. The test uses the pinned native Konnect 0.13.0/KiCad 10.0.7 pair already documented by the runtime gates.

Proved: shared selected-sheet capability; actual official-SDK protocol list/read/propose/status calls; denied unknown identifiers/extra paths/stale selections; no exposed approval token; proposal remains pending until desktop approval; one-use approval; complete native edit/readback verification/publication; original preservation; recovery after closing the project owner. The first failing native gate exposed extended Windows canonical paths; the next failing recovery assertion exposed an attempt to open an already-owned project. Both are corrected in the same boundary and tests.

Unproved: real Codex/Claude Code authentication or inference, an external process attached to the live desktop, embedded conversation, cancellation during a live MCP mutation, native discovery UI and installed acceptance. Remaining workflow tools and agent transports are unfinished. Do not describe the SDK test client as Codex or Claude Code.

Retained [source hashes and scoped status](status.json), [recovered run](run.json), [published result](Artifacts/result.json), [original sheet](Artifacts/Original/test.kicad_sch), [edited sheet](Artifacts/Edited/test.kicad_sch) and [native netlist](Artifacts/Netlist.xml) preserve the passing run independently of the temporary directory. Private profile/configuration and process logs are not copied into this evidence.
