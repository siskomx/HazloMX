# Hazlo Mexa

A Windows-first, open-source desktop interface for AI-assisted DIY and engineering workflows, built with Rust and Tauri.

## Status

A Windows 11 x64 desktop alpha is implemented in [apps/desktop](apps/desktop/README.md): manual dimensions or an explicitly selected Ollama profile → validated rectangular-solid proposal → editable approval → verified FreeCAD FCStd/STEP files → explicit opening. Streaming, cancellation, persisted history, and crash recovery are implemented. The local Qwen3 profile remains experimental; the tested Kimi cloud profile passed the frozen Spanish cases.

The desktop source includes all nine planned engineering workflows, with Spanish chat, metric/imperial inputs, explicit approvals, native result readback and project recovery. Claude Code and Codex share Hazlo’s tools alongside the existing Ollama option. Completion still requires the remaining Windows, native GUI and signed-in acceptance gates:

| Integration | Planned first workflow | Current implementation |
|---|---|---|
| Arduino CLI / Arduino Agent | Create a valid sketch and compile with an explicitly selected board/core; interactive agent tools follow | Native and installed Uno compilation and Agent copy-editing backend checks pass; wider boards/libraries and signed-in/native GUI acceptance remain unfinished |
| KiCad / Konnect | Schematic/PCB checks and exports on project copies; editing tools follow | KiCad checks/exports and Konnect schematic/PCB copy edits support inspection, approval, saved-state verification and recovery; broader native GUI acceptance remains unfinished |
| CADCraft | Inspect and convert drawing copies | [Inspect first](docs/research/drawing-source-first-2026-10-13/README.md), DXF/DWG/PNG conversion and approved circle edits pass scoped native checks; current guest and native GUI acceptance remain pending |
| CADPilot | Measure and edit a selected FreeCAD document with explicit approval and recovery | Native inspection, box edits and box/cylinder additions on copies pass scoped backend checks; native GUI/live-document acceptance remains unfinished |
| OrcaSlicer | Verify profiles and slice to a project file | Captured machine/process/material profiles, metric/inch meshes and verified slicing files pass scoped native checks; wider profile/native GUI acceptance remains unfinished |

These tracks reuse project selection, explicit approvals, owned-process cancellation, artifact verification and recovery. Each requires its own adapter and Windows acceptance evidence. The initial FreeCAD goal and its unresolved local-model benchmark do not represent completion of this broader application.

Current priority is application integration, with further AI-model evaluation deferred at the user's request. Further providers, a universal MCP gateway and automatic source builds remain separate later work. Compilation and slicing do not authorize firmware upload or printer control.

The [current application goal](docs/application-goal.md) requires completing all of these integrations, their native workflows and their Windows verification.

The [implementation readiness research](docs/research/2026-10-10-implementation-readiness.md) records source-level findings, provider contracts, integration choices, and the recommended architecture. The [implementation gates](docs/implementation-gates.md) define the Windows proofs required before building the full application. The [source evidence](docs/research/source-evidence.json) pins inspected releases and source fingerprints.

The [verified combinations](docs/supported-combinations.md) and [acceptance audit](docs/acceptance-audit-2026-10-11.md) identify the exact scope of passing FreeCAD evidence, including second-Windows-environment, WebView2 bootstrap/offline failure, disk-exhaustion and visible-artifact checks. Local-model accuracy remains experimental. The [later integration gates](docs/implementation-gates.md#later-gates-that-protect-the-first-architecture) define the additional proofs needed for the rest of the application.

## License

Hazlo Mexa's original code is licensed under the [MIT License](LICENSE).
Third-party applications and dependencies retain their own licenses.

## Website

The [landing page](website/) is ready for Cloudflare Pages. See [website setup and hosting](website/README.md) for build, preview, and deployment settings.
