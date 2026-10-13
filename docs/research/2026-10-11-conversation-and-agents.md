# Natural language and local agents

The October 11 user clarification changes the current goal: discovery-led conversation and Claude Code/Codex tool access are required. Existing non-FreeCAD chat branches are fixed guided replies; only FreeCAD currently dispatches Ollama inference. Do not present these branches as a general tool-using agent.

## Verified interfaces

Installed commands report `codex-cli 0.159.2` and `2.1.285 (Claude Code)`. Version/help inspection did not run a model, read credentials or modify agent configuration.

[Codex app-server documentation](https://learn.chatgpt.com/docs/app-server) documents bidirectional JSON-RPC with streamed conversation events, approvals and cancellation. Dynamic tools use an experimental capability and server-originated calls. Pin and test the installed protocol rather than attaching UI directly to all protocol types. This is the embedded Codex route; the server is not an MCP transport.

[Codex MCP migration guidance](https://learn.chatgpt.com/docs/mcp-server) says the former `codex mcp-server` hosting command was removed; external MCP tool servers remain supported. Do not build against the removed hosting interface.

[Claude Code programmatic operation](https://code.claude.com/docs/en/headless) supports noninteractive and streaming use. Allowed-tool lists grant permission; they are not sufficient isolation. [Claude Code MCP documentation](https://code.claude.com/docs/en/mcp) supports explicit MCP configuration and strict configuration for controlling loaded servers. Installed help also exposes built-in tool selection, stream-json input/output and session resumption. Inspect permission/tool denial behavior before enabling mutations. Minimal/bare mode changes authentication behavior, so do not silently choose it for a subscription connection.

## Concrete implementation boundary

Reuse the existing Workflow, typed adapters, captured sources, one-use human approval, owned cancellation and artifact verification. Agent tools must discover supported operations, inspect selected documents, propose typed changes, and read status/results. Never give an agent the human approval action. CLI permission prompts are distinct from approval of an engineering proposal.

Expose a single host tool implementation to both providers; MCP is the external-client interface. Embedded Codex uses app-server; embedded Claude Code uses its supported streaming/SDK interface. Avoid a second standalone workflow owner competing with the desktop process for project writes. Keep project selection and access grants explicit; no arbitrary paths supplied by an agent, general shell, global configuration mutation or credential extraction.

For chat, preserve messages, provider session IDs and pending questions. Translate complete tool calls into host operations, return typed success/error results, and keep cancelled/disconnected turns from applying late proposals. Require protocol negatives and an actual client/tool round trip before showing a provider as connected. Local installed agent processes may still send prompts/context to their cloud service; provider selection must disclose that before dispatch.

## Implemented first discovery path

Schematic discovery reads the user-selected sheet without Konnect or inference, inventories only placed symbols, collapses consistent multi-unit components and rejects ambiguous metadata. The selector shows symbol type and actual value; identifiers are automatically filled and explained in an optional technical section. Library templates and child sheets are not misreported as editable components on this sheet. Up to 256 components per sheet and the existing bounded parser/read policy apply.

Preparing this path binds the inspected file hash and project identity to the captured original before approval. A changed source is rejected and must be selected again. The final mutation continues through the existing Konnect value-only verifier and independent native KiCad readback.

Validation on October 11: 78 regular Rust tests pass (24 separately scoped ignored); desktop/custom-protocol Clippy passes; TypeScript/Vite build and the eight-workflow browser fixtures pass. The real `actual_konnect_workflow_publication_and_recovery` test passes with the discovered-source path, rejected project/hash mismatches, native KiCad readback, approval, publication and recovery. Browser coverage includes a user who does not know the identifier, discovered choice/current value, literal untrusted values and locked selection during approval. Native discovery pickers and installed acceptance have not been exercised. Power symbols are visible but read-only under the existing edit contract.

## Shared host and MCP SDK implementation

The desktop discovery and proposal commands now use `host_tools::HostTools` around the same `Arc<Workflow>`. The `McpTools` official-SDK server handler exposes `get_workflow_status`, `inspect_selected_schematic` and `propose_schematic_value`. Callers receive a user-selected source capability, component data and job phases; they cannot set paths, change project/runtime selection, obtain approval tokens or approve/execute jobs. Strict typed argument decoding, bounded requests, stale project/source rejection and literal data handling remain host-enforced; protocol annotations are only hints.

The regular SDK duplex test performs actual protocol initialization, tool listing and a selected-sheet call; it verifies operational errors separately from success. A separate identified-runtime test sends a value proposal through the SDK, verifies it is awaiting human approval, attempts an unavailable approval tool, then uses the desktop Workflow approval to run native Konnect/KiCad verification. It reads completion through MCP, preserves the original and recovers the result after closing the original project owner. [Retained evidence and scope](host-tools-2026-10-11/README.md).

This test exposed the earlier discovery path's use of Windows extended canonical paths, which the capture policy correctly rejects. Discovery now uses the existing ordinary-local-path validation and pins both source-root and entry ancestry before reading. The first protocol gate also tried reopening a project while its owner was still live; corrected recovery closes the owner first.

Current validation: 80 regular Rust tests pass, 25 separately scoped ignored entries; desktop/custom-protocol Clippy and eight-workflow browser fixtures pass. No authenticated agent turn, model inference, global agent configuration or paid API call was used.

## Local desktop connection

The desktop now owns an opt-in authenticated Windows named-pipe bridge to the same HostTools. The standalone stdio helper connects using a temporary capability, then relays the SDK protocol without owning a project or executing engineering operations. Remote pipe clients, malformed endpoints and incorrect tokens are refused; sessions are bounded. Revocation closes active sessions. Each handler is frozen to the project at enablement and refuses later-project data, independently of UI revocation. Cancellation cleanup lives inside the blocking tool worker as well as the response handler so dropping an async caller cannot skip late-proposal cleanup.

The UI displays temporary Claude Code JSON/Codex TOML configuration, discloses context sharing, revokes access on project changes, clears configuration on revocation and polls while tools are enabled so externally prepared proposals become reviewable. It distinguishes tool access from the unfinished embedded agent chat. Setup: [external agent tools](../agent-tools-setup.md). Packaging builds the release helper and includes it beside the desktop; final installer acceptance is still pending.

Current checks: 81 regular Rust tests pass with 26 separately scoped ignored entries; Clippy, TypeScript/Vite build and browser fixtures pass. The actual release helper initializes MCP, lists tools and reads status through the authenticated pipe; idle revocation closes the client and the observed helper PID ceases. The actual edit test now uses that separate release helper and pipe before desktop approval, native verification and recovery. [Bridge evidence](agent-bridge-2026-10-11/README.md).

## Installed client verification

The [installed-client protocol gate](agent-client-protocol-2026-10-11/README.md) now verifies Codex 0.159.2's exact catalog, direct selected-component read and preserved tool refusal through its actual app-server. Claude Code 2.1.285's supported MCP configuration/health commands report the same connector connected using a private test home. These are no-model client protocol checks; no signed-in inference or natural language turn is claimed.

Pending: authenticated/model-driven turns and edits through Claude Code/Codex, embedded provider chat/streaming/permission handling, remaining engineering tools, general natural language reasoning, geometry/object discovery UI, live mutation cancellation/reconciliation evidence and native/installer acceptance.

Latest [shared FreeCAD/Arduino tool workflows](tool-workflows-2026-10-11/README.md) expand the catalog to six tools with capabilities, explicit units and project-bound proposals. Native generation/compilation through the release helper, one-use desktop approval and version-10 MCP provenance/recovery pass. This supersedes the earlier schematic-only surface while preserving its original evidence. Current regular suite: 82 passing, 27 separately scoped ignored entries.

October 12: [Claude streaming boundary](claude-stream-2026-10-12/README.md) adds restricted arguments, bounded stream/session/tool checks and an owned cancellable runner. The actual credential-free installed-client gate confirms Hazlo-only initialization and a terminal sign-in error. Current suite: 85 passing, 28 separately scoped ignored. Desktop/provider chat wiring and signed-in turns remain unfinished.
