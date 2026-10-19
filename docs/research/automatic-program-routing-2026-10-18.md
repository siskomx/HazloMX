# Agent-selected native programs

Hazlo exposes its ten supported program groups through `get_design_capabilities.available_programs`, including their native discovery methods and upstream repositories. Supported programs are distinguished from detected installations: unchecked programs say `not_checked`, while an attempted connection reports `connected` or `unavailable` with its reason.

The shared `select_program` agent tool accepts only a supported program identifier. It reuses the desktop's existing automatic detection, remembered locations, generated configuration and bundled Rea runtime. It takes no executable path, commands, MCP endpoint or installation option. Selecting a program does not download software, select an analysis target, edit a document or authorize a capture.

The initial chat now exposes this tool and the native discovery/proposal/readback tools. The agent can select or switch programs and inspect native schemas in the same turn. The desktop follows the resulting program and connection without restarting the conversation. The former chat button asking the person to select the agreed program is removed; connection settings remain available as an override. Missing native setup does not silently enable legacy operation wrappers as a fallback.

Selection remains bound to the current project UUID and folder. A switch is refused while a job awaits approval or is running. Native changes and diagnostic queries retain their existing review, guard and readback requirements. Downloads remain explicit and on demand.

## Validation

- Regular Rust suite: 222 passed, 64 ignored. Targeted selection checks also covered invalid program names, extra path/download arguments, cancellation, missing runtime, no implicit downloads and copied-project rejection.
- Owned native MCP fixture: seven program routes selected from the initial chat, with catalog/schema discovery, approval, guarded edits, result readback and refusal to switch while a proposal awaited approval. This used a fixture server, not seven physical program windows.
- Four-provider browser fixtures covered agent-selected connection state, missing setup, continuing the same conversation and manual connection recovery. The existing native approval/review matrix also passed. These fixtures do not establish live provider or hardware behavior.
- Production frontend build and desktop Clippy passed.

## Real-account chat observation

A fresh candidate app and local project were opened through the Windows UI with the existing Codex account. The request supplied the Roborock Q7 M5+ goal, local Wi-Fi but no cloud, and no capture, firmware or robot/router access. It asked Hazlo to check its actual tools without naming a program or a solution. The operator did not touch the program selector or connect a native program manually.

In one user turn, the agent read Hazlo's capabilities, selected TShark, received the missing-installation result, then selected bundled Rea and requested its native `current_document` and `inspect_pcap_client` schemas. The visible chat followed both program changes and ended with Rea connected. The agent researched public sources and kept physical compatibility and offline behavior unverified. It did not analyze a nonexistent artifact, download a tool, propose a device operation or claim a repair.

The retained conversation has one user message and one assistant response. Raw provider calls, local paths and receipts remain in ignored evidence folders. No robot, router, firmware, capture driver or physical port was changed. Actual cloud-free robot operation and the optional-download setup flow remain outside this observation.
