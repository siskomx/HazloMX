# Vacuum repair scenario: research before native edits

The requested scenario is a working Roborock Q7 M5+ with scheduled cleaning
without internet/cloud services. Local Wi-Fi is explicitly acceptable. No robot,
firmware image, board photographs or measured parts were supplied to this test.

## Gap and change

The installed alpha.5 chat could discover native engineering tools but could
not research missing public documentation. Claude's built-in tool list was empty;
Codex web search was disabled and its stream decoder rejected search items.

Claude now explicitly enables only WebSearch and WebFetch alongside existing
Hazlo MCP tools. Restricted mode, strict MCP configuration, disabled hooks and
the engineering approval boundary remain. Codex enables native live web search
and accepts its read-only lifecycle items. Shell, arbitrary local-file and
computer-use tools remain disabled. Queries and requested URLs go to the provider.
Shared instructions prohibit private data/credentials in queries and URLs,
prefer primary sources, require citations and distinguish documentation from
hardware verification.

Provider references: [Claude CLI](https://code.claude.com/docs/en/cli-reference),
[Claude tools](https://code.claude.com/docs/en/tools-reference),
[Codex configuration](https://learn.chatgpt.com/docs/config-file/config-reference),
[Codex app-server items](https://learn.chatgpt.com/docs/app-server).

## Real desktop checks

An owned debug Hazlo window used a new disposable project, the installed signed-in
Claude client and existing registered native runtimes. Selecting bundled Rea
automatically connected its 207-tool MCP catalog without selecting a config file.
The actual conversation used WebSearch, WebFetch, get_design_capabilities and
inspect_native_tools. It researched local-control compatibility, retained the
no-cloud/local-Wi-Fi requirement across a follow-up, identified that no firmware
was available to analyze and did not prepare an engineering operation.

The same window connected the existing scoped Codex account and completed a real
research turn with Rea still connected. It identified explicit Q7 M5/M5+ exclusion
from Local Server and avoided equating it with Q7 Max. Its initial manual link was
for Q7 M5 rather than the specifically requested plus manual; a follow-up found
the exact plus manual page and PDF. Independent reading of PDF page 05 confirmed
the reported robot model RRA0TCP and auto-empty dock model AED08LRR. The chat
explicitly distinguished model numbers from an FCC ID and from the user's unit.
These are model-generated findings, not a guarantee of error-free
research. Citations and exact hardware identity still need checking.

The library suite passes (215 tests; 64 explicitly ignored environment/hardware
checks), and library Clippy passes with warnings denied. The research tests cover
restricted Claude arguments, allowed search/fetch calls, nonterminal research
events and continued refusal of arbitrary shell/file tool events. The existing
Codex protocol fixture passes with its exact tool-surface expectation including
the single native web-search tool; its engineering namespace remains exact.

## Primary evidence and hardware limit

- [Roborock Q7 M5+ FCC manual page](https://support.roborock.com/hc/en-us/articles/46048239733273-Roborock-Q7-M5-FCC-User-Manual)
  exists. A public manual does not establish the identity or revision of a user's
  particular robot or expose an internal control interface.
- [Roborock Local Server's tested vacuums](https://python-roborock.github.io/local_roborock_server/tested_vacuums/)
  explicitly lists Q7 M5/M5+ as unsupported.
- [Valetudo's exhaustive supported list](https://valetudo.cloud/pages/general/supported-robots/)
  lists Q7 Max/Max+, not Q7 M5/M5+. Support cannot be transferred between them.
- [Home Assistant's Roborock integration](https://www.home-assistant.io/integrations/roborock/)
  describes variable Q-series compatibility, cloud-dependent setup/fallback and
  cloud retrieval of maps/routines. This does not establish the requested
  cloud-free scheduling on this exact unit.

No firmware was analyzed, controller fabricated, network reconfigured, robot
flashed or physical repair verified. Without supported model-specific control
or actual hardware/firmware evidence, generating a supposedly compatible board,
firmware or printable replacement would invent the missing engineering inputs.
The next evidence is the unit's model/regulatory label (personal identifiers
redacted), region and firmware version; opening the working robot is not required
for this initial identification. KiCad 10 live schematics remain deferred.

This change is tested in a debug desktop; alpha.5's installed/released binaries
do not contain the research change. No new installer or clean guest acceptance
is claimed by this scenario.
