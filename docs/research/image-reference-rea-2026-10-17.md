# Images and Rea in the simple-chat flow

Requested: attach a reference image, identify the part, agree on the information
needed to recreate it, edit in the chosen native program and check that result.
The image is evidence for the conversation; it is not an exact physical ruler.
Ask for a known dimension/reference and units when scale is unknown.

The existing selected Ollama cloud profile currently advertises `vision`,
`tools`, `completion` and `thinking` through its actual `/api/show` response.
This is a capability check, not a passed image conversation. Images must be
sent only to a provider/model supporting image input, with an actionable message
for other selections. Preserve the text-only flow and its approval boundary.

[Rea's documentation](https://github.com/morluto/rea#what-you-can-analyze)
describes software, binaries, firmware and runtime analysis through MCP and CLI.
It does not document physical photo-to-CAD reconstruction. Use an image-capable
agent for that reasoning and the selected CAD program's tools for geometry.
Rea is an additional optional native analysis integration, rather than a
replacement CAD engine. Its actual installed catalog governs available tools.

The npm registry reports `rea-agents` 6.5.0 and Node support
`^22.19.0 || ^24.11.0 || >=26.0.0` at this checkpoint. The
[manual MCP route](https://github.com/morluto/rea/blob/main/docs/installation.md#limit-rea-to-one-project-or-agent-profile)
supports project-specific connection. Hazlo should use its own selected native
connection rather than running setup against personal agent configurations.
Repository refresh checks releases/catalog metadata; it does not install updates.

Implemented in the existing chat: one PNG/JPEG reference, preview and removal.
The browser re-encodes it as JPEG, removes original metadata/name, bounds the
source to 8 MiB/20 megapixels and the prepared reference to 1536 pixels/512 KiB.
Hazlo keeps image bytes in memory, outside saved conversation history, and clears
them on provider/project/new-conversation changes. Automatic result review does
not resend them. The selected provider or its client may retain transmitted input.

Ollama must advertise vision as well as tools. Codex must confirm image input for
the thread's active model through its native model catalog; missing metadata
refuses transmission. The current Claude connection explicitly refuses images;
its text flow remains available and its sign-in test remains pending.

Rea 6.5.0 passes actual Windows MCP discovery with **207 tools**, and an explicitly
approved backend harness runs its own `analyze_javascript_application` against a
disposable three-file JavaScript fixture. Native analysis/readback completes and
the input files remain byte-for-byte unchanged. This is neither a Hazlo UI
conversation nor coverage of every Rea tool or external analysis dependency.
The isolated npm install verifies the registry integrity, disables lifecycle
scripts and does not run Rea setup or modify personal agent registrations.

Rea's default verbose catalog exceeds Hazlo's 1 MiB catalog boundary. Use Rea's
own compact input-schema presentation and omit output schemas in the selected
MCP configuration; all tool names and input constraints remain discoverable,
and Rea still validates output against its canonical contracts:

```json
{
  "command": "C:/path/to/node.exe",
  "args": ["C:/path/to/rea-agents/scripts/rea.mjs", "mcp"],
  "env": {
    "REA_MCP_INPUT_SCHEMA_PROFILE": "compact",
    "REA_MCP_OUTPUT_SCHEMA_PROFILE": "off"
  }
}
```

Source checks pass: 204 regular Rust tests (62 external/native tests ignored),
Clippy with warnings denied, production frontend build, seven-program native
routing fixtures and 28 native IPC approval/review cases across four AI choices.
The image browser fixture checks normalization, metadata removal, no saved image,
provider/new-chat clearing, malformed refusal and no automatic retransmission.
These are protocol/UI fixtures, not real image inference.

[Real selected-image recognition, scale clarification, approved native CAD
creation/readback and a Rea chat now pass](image-rea-live-2026-10-17.md).
The follow-up source has 205 passing regular tests and a rebuilt Windows package;
its fresh guest lifecycle, installed chat/picker and project-preservation checks
pass. Guest inference and all engineering workflows were not repeated there;
the real conversations remain separate host evidence. KiCad 10 and the deferred
live schematic choice stay unchanged.
