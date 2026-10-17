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

Implementation is pending. Acceptance needs actual selected-image recognition,
unknown-scale clarification, a separately approved native reconstruction/readback,
unsupported-model refusal, preserved text-only conversations, and Rea discovery
plus an owned-input analysis check. Claude sign-in stays pending; KiCad 10 and
the deferred live-schematic choice stay unchanged.
