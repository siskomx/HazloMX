# Program tool catalog and upstream release checks

The existing `get_design_capabilities` tool now includes the exact callable
tool names for the selected program, its upstream repositories, the tested
adapter versions and the native MCP/CLI discovery methods. All agent routes
reuse this response and their existing tool definitions. The tool count stays
35; no new editor, operation form or AI provider is added.

The registry covers FreeCAD, CADPilot, CADCraft, Arduino CLI, Arduino Agent,
KiCad, Konnect and OrcaSlicer. Version constants come from the adapters.
CADCraft explicitly distinguishes the saved-copy 0.5.0 CLI from the selected
maintained 0.7.0 live runtime. Its previously stale live capability response
now includes the tested DXF export.

An ordinary capability read is offline. When the user asks about updates,
the agent can pass `check_updates_for`, for example `"konnect"`, to query
that fixed public repository's latest release endpoint. The response reports
the release tag and whether it differs from the tested adapter version.
It does not claim that a different release is installed, compatible, newer,
or provides additional callable tools. Timeouts, HTTP failures, missing
releases and malformed metadata remain explicit unavailable results.

The existing HTTP client gains native TLS support. Requests contain no
project paths, document contents or credentials; arbitrary URLs and redirects
are refused. Responses are bounded to 256 KiB and ten seconds, cancellation
drops the request, and only release tags enter the agent result. Release
descriptions and repository instructions are not imported. Checks do not
install, download or enable software/tools. Cross-program queries are refused
before network access through both the internal chat and external MCP paths.

Verification: 198 regular Rust tests pass, with 56 separately scoped tests
ignored; the explicit real CADCraft public HTTPS release check passes.
Fixtures cover registry/schema agreement, selected-program catalogs,
no project writes, malformed/unavailable/oversized release responses and
cancellation. The production chat IPC fixture passes all four AI choices,
including the existing native approval/review flows.

This is catalog provenance and release checking, not a complete upstream MCP
proxy. Existing native discovery and execution retain their current adapter
limits. The remaining architecture work is to expose the connected programs'
actual schemas and route their native tools through the existing approval and
readback boundary, refreshing discovery when their catalog changes. Repository
metadata cannot substitute for that runtime discovery. The latest retained
installer predates this change; no Windows guest or native UI was repeated.

Primary references: [MCP tool discovery](https://modelcontextprotocol.io/specification/2026-07-28/server/tools),
[CADPilot](https://github.com/LBurny/cadpilot),
[Konnect](https://github.com/mixelpixx/Konnect),
[CADCraft](https://github.com/storytold/cadcraft),
[Arduino Agent](https://github.com/mixelpixx/Arduino-Agent),
[Arduino CLI](https://github.com/arduino/arduino-cli),
[FreeCAD](https://github.com/FreeCAD/FreeCAD),
[KiCad](https://gitlab.com/kicad/code/kicad),
and [OrcaSlicer](https://github.com/OrcaSlicer/OrcaSlicer).
