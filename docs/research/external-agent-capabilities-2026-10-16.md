# Native Arduino capability discovery for external agents

Project-wide external MCP connections now receive the existing Arduino
Agent native-buffer inspection, edit-proposal and save-proposal description
from `get_design_capabilities`. Previously that description was added only
for embedded Arduino Agent or planning connections, although a project-wide
connection already exposed the corresponding tools in its catalog.

The fix moves the existing description outside the program-only branch and
includes it when no single program scope was requested. It adds no tool,
permission, transport, provider or native operation. The separate saved-copy
description stays explicit. Native effects still require the running editor
companion, exact current document binding and human approval; compilation,
hardware control and automatic saved-file fallback remain unavailable there.

The new direct MCP-host regression fails before the change because the native
Agent description is absent. It passes afterward and checks project-wide
discovery plus all seven program scopes. Planning and Agent descriptions
retain the capability; other program descriptions do not gain it. Only the
Agent program scope exposes its native save tool; planning remains read-only.
The project-wide catalog retains the already available native tool. Discovery
creates no job or history entry.

All 20 regular host-tool tests pass with 12 existing ignored native cases.
Library Clippy with warnings denied and the custom-protocol desktop build
pass. The stdio helper and protocol host are rebuilt, and the actual 33-tool protocol check passes with no
engineering jobs. That transport check verifies the unchanged catalog and
general capability exchange; the new field and program-filter assertions
come from the direct host regression. Raw protocol logs stay private under
ignored `target/`.

This is capability discovery, not authenticated Codex/Claude model acceptance
or a new native editor demonstration. The latest verified installer predates
this source change. The full multi-program goal remains unfinished.
