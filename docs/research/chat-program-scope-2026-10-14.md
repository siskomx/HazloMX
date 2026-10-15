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

## Latest installer candidate

Source commit `3ebd8e3` builds unsigned 0.1.0 candidate **`472af63e…`**, including both the conversation-length correction and selected-program enforcement. All 6,584 tracked inputs remain identical through the build, and targeted scans find zero builder-profile paths in the app/helper binaries. The actual release helper passes the 26-tool standalone transport check without engineering jobs.

| Artifact | SHA-256 |
|---|---|
| Installer, 5,569,213 bytes | `472af63e4003d47a985c0b5abf18cc2c938e2cea42d89eb91e1f7d7f85391342` |
| Unbundled release app | `4b2938915f6fd30d08564fbff922d023c3ac67e8d24075b3d8f6360c5b5573a7` |
| Bundled helper | `02ba28ca09829087b21b2021639e61f2c926191c66b2be1d73f788f150561d26` |

The new preparer's fresh check observes 2,354 MiB available against the unchanged 3,072 MiB requirement and refuses before creating a configuration or guest. The earlier `4768e717…` prepared inputs remain preserved and are marked superseded by this newer source candidate, preventing them from being mistaken for current-source acceptance. This build has no corrected native or installed acceptance; explicit UI resume is still pending.

## Folder binding and current conversation follow-ups

The user has explicitly resumed native UI automation in owned Hazlo windows and the disposable guest. Earlier stop and pending-resume statements above describe those earlier attempts.

The program connection now validates the original project UUID **and folder** captured by the desktop request before starting its asynchronous provider connection. A new regression copies a project manifest into another folder, verifies refusal both at connection creation and on a previously created connection, and checks that neither path creates a job or history entry. All three desktop provider routes and protocol fixtures retain this original folder identity. The actual Codex 0.159.2 local protocol switch case passes again after this correction; cloud authentication/inference remains unverified.

All seven real-model integration fixtures now use the same restricted program route as production. The test harness checks the exact advertised catalog on every recorded model request, rather than only checking the final job. Real existing-cloud-profile Arduino and CADCraft conversations pass after the folder correction: five turns, two native jobs each, clarification, result-derived follow-up, preservation and recovery. Arduino compiles the confirmed Uno sketch with 1,000 ms then 500 ms delay (924 flash bytes each). CADCraft moves the selected circle by one inch (25.4 mm), then another 5 mm, preserving unrelated geometry and the earlier output.

The drawing rerun exposed ambiguity between the verified report hash and the drawing file hash. Validation remains strict; verified-result output now supplies an explicit editable-drawing follow-up pair, and the tool description and refusal explain which hash to use. The final real-model drawing run passes with eight advertised tools; Arduino passes with four. Earlier failed attempts are retained privately. These gates use backend fixture approvals, not native UI approval, and do not establish installed acceptance.

Automatic result review now stays internal to the provider request instead of appearing as a fabricated user message with tool names and job identifiers. It also preserves an unsent draft. The production browser fixture passes all 36 handoffs across four provider choices, including assertions for both behaviors, existing approval isolation, delayed/duplicate completion and cancellation. Frontend typecheck/build and Rust Clippy pass; the regular Rust suite passes 144 tests with 52 ignored. No new dependency, model download, approval bypass or native editor is introduced.

The 472af63e installer above predates these corrections. A fresh candidate and native Windows/installed acceptance are required. Broader live/unsaved editing, actual Agent conversation and authenticated Claude/Codex inference remain unfinished.
