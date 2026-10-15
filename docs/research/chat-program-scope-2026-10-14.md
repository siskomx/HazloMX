# Bind desktop conversations to their selected program — October 14, 2026

The desktop previously put the selected program in the prompt while exposing the entire shared tool catalog. A model could request another program's proposal tool if that program's source/runtime had been connected earlier in the same project. Existing source validation and human approval still applied, but program routing depended on the model following the prompt.

The production chat now sends the selected program as a separate typed desktop-command argument. Claude and Codex receive a project/folder-bound MCP connection restricted to that program. Ollama uses the same scoped host wrapper for its advertised tools and dispatch checks. A forbidden call is refused before proposal preparation. Ollama validates the complete tool-call batch before dispatching it. Capability results describe the selected program, including Agent's edit-copy scope without compilation or hardware control.

| Desktop program | Advertised tools, including three shared reads |
|---|---:|
| FreeCAD / CADPilot | 7 |
| CADCraft | 8 |
| Arduino CLI | 4 |
| Arduino Agent | 5 |
| KiCad / Konnect schematic and PCB | 11 |
| OrcaSlicer | 6 |

The shared reads are capabilities, project workflow status and verified-result access. Existing verified results remain readable within the bound project so the conversation can reference earlier work across programs. Every other tool belongs to exactly one program group. The standalone project connector retains the complete 26-tool catalog; the new restriction applies to desktop conversations with an explicit program choice. No approval tool, native editing engine or hardware action is introduced. Existing exact source/revision checks, cancellation, late-result refusal and result-derived follow-ups remain in force.

## Evidence

- **143 regular Rust tests pass, 52 ignored.** New real SDK/pipe tests check all six advertised catalogs, unknown-program refusal, valid cross-program proposal requests producing no job/history and capability isolation. Correctly scoped FreeCAD/Arduino proposals still reach human review, keep approval nonces out of tool results and are cancelled by the backend fixture without execution. A real loopback Ollama protocol test checks every scope and rejects a cross-program batch before any proposal.
- The production browser matrix passes all four AI choices, six program groups and 36 approval/result-review handoffs, asserting the separate program argument for both user requests and automatic review. The legacy nine-workflow browser regression also passes with its old workflow aliases mapped to their program groups. These are browser IPC fixtures.
- The actual installed **Codex 0.159.2** client passes six local HTTP-fixture cases with the exact model-facing restricted catalogs. A seventh case resumes the same conversation while changing FreeCAD's seven tools to Arduino's four, reads real refreshed MCP status, preserves earlier messages and revokes the old connection. There is no cloud inference or signed-in acceptance in this gate.
- The first Codex attempt selects the different `0.162.0-alpha.17.2` executable currently on PATH and is refused by the existing version check before inference. That failure is retained. The subsequent checks select the already-installed npm 0.159.2 client explicitly; no client/model download or version-pin change occurs.
- Desktop binaries/examples compile, frontend typecheck/production build and Clippy with warnings denied pass. Raw client transcripts, test homes, paths and source fixture files remain private under ignored `target/`.

The [receipt](chat-program-scope-2026-10-14.json) records these scopes. Actual Claude conversation, real-model/native UI behavior of this change and installed acceptance remain unfinished. Native UI automation is still stopped after Escape; the resume question remains pending. Prepared installer `4768e717…` includes the earlier conversation-length correction but predates this program-binding change, so it cannot establish current-source acceptance. The full application goal remains active.
