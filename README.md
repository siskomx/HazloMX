# Hazlo Mexa

A Windows-first, open-source desktop interface for AI-assisted DIY and engineering workflows, built with Rust and Tauri.

## Status

Windows 11 x64 alpha. Hazlo is one natural-language chat: describe an idea,
define the missing parameters/software together, approve edits through the
selected program's native tools, then read back and finalize the result.
The programs own their editors and documents.

[Download the Windows alpha](https://github.com/siskomx/HazloMX/releases/tag/v0.1.0-alpha.2).

Start with the [quickstart](docs/quickstart.md). Installed Codex and Claude Code
clients are detected automatically; manual selection remains available. Real
signed-in Claude conversations and the [Codex native chat test](docs/research/codex-real-chat-2026-10-17.md)
now pass. Image references support identification
and reconstruction with a vision-capable connection; Rea supplies native
software/firmware analysis.

| Program | Verified scope and relevant limit |
|---|---|
| FreeCAD / CADPilot | Native creation/edit/readback, mixed units, image reconstruction, native saving and STEP export; exact source scopes are recorded |
| CADCraft | Selected patched native MCP runtime; real chat edit/Save As and independently checked DXF; installed stock runtime stays separate |
| Arduino Agent | Current release chat, visible native 250/125 ms edits and readback; its MCP writer saves immediately |
| Arduino CLI | Verified Uno compilation and saved artifacts; no firmware upload or live-editor claim |
| KiCad / Konnect | Current release PCB chat, natural component identification and inch/mm native moves; KiCad 10 live schematics stay deferred |
| OrcaSlicer | Selected MCP runtime live chat edits plus offline profile/slice/G-code finalization; project saving stays in Orca and transforms lack native Undo |
| Rea | Discovered native analysis tools and real approved JavaScript analysis/result review; not a physical photo-to-CAD engine |

See [finishing evidence](docs/research/app-finish-2026-10-17.md),
[current acceptance and Windows package status](docs/native-flow-status.md) and
the [application goal](docs/application-goal.md). Host conversations, debug
checks and guest installation tests retain their distinct scope. Editing and
slicing do not establish physical manufacturing readiness.

Native MCP schemas and CLI help govern execution. Repository checks refresh
catalog/release metadata and never install programs automatically. Further
model benchmarking remains deferred; application integration is the priority.
Developer build instructions and historical tests are in
[apps/desktop](apps/desktop/README.md). Earlier research and exact initial
combinations remain available in [supported combinations](docs/supported-combinations.md).

## License

Hazlo Mexa's original code is licensed under the [MIT License](LICENSE).
Third-party applications and dependencies retain their own licenses.

## Website

The [landing page](website/) is ready for Cloudflare Pages. See [website setup and hosting](website/README.md) for build, preview, and deployment settings.
