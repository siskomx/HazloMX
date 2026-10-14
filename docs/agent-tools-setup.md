# Connect your AI to Hazlo Mexa

## Current chat setup — October 14, 2026

1. Open **Conexiones** and create/open a local project folder.
2. Choose your AI, then select its installed client or already configured Ollama profile using the controls below.
3. Choose the engineering program, its runtime and the saved document you want to work on. Arduino CLI can prepare a new sketch without an original document.
4. Describe what you want to build or change. The AI inspects available capabilities and selected content, resolves technical IDs internally and asks for genuinely missing information in ordinary language.
5. Review the proposed change in Hazlo and approve it explicitly. The program performs the operation; Hazlo checks the actual saved files and requests a verified-result explanation from the selected AI. Follow-ups require fresh result inspection and approval.

The current screen has one chat and connection controls. See the [first workflow](first-workflow-demo.md) and [drawing example](drawing-workflow-demo.md).

## AI connections

| Choice | Setup | Current limits |
|---|---|---|
| Codex instalado | Select the identified installed Codex client. Use **Conectar mi cuenta**, then **Abrir inicio de sesión** if offered. Complete sign-in personally through the official link. | Client/protocol and browser fixtures pass; real signed-in conversation and current installed/native UI acceptance remain open. |
| Claude Code instalado | Select the identified installed Claude Code client and sign in through that client. Hazlo uses its existing sign-in. | The last real conversation attempt reported signed out with expired OAuth. User sign-in is pending verification; no credentials belong in chat. |
| Ollama local | Use the existing Ollama 0.35.1 daemon and supported installed qwen3:4b profile. | Identity/routing/tool support are checked before sending. Protocol/browser fixtures pass; local model quality remains experimental. |
| Ollama cloud | Use the existing daemon and kimi-k2.7-code:cloud profile connected to your own Ollama account. | Real backend conversations pass for FreeCAD, CADCraft, Arduino CLI, KiCad schematic/PCB and Orca. Approval uses backend fixtures; direct desktop/installed parity remains open. |

Hazlo does not download models. Installed-client identity checks currently target Codex 0.159.2 and Claude Code 2.1.285; other versions are not established by the recorded tests. Sending through a cloud route can consume your account plan and sends selected context to that provider. Claude and Codex manage their own authentication; do not paste credentials into Hazlo messages or publication artifacts.

All routes use the same **26-tool catalog** and project-bound validation. The AI has no approval tool. It can prepare a proposal, but only the user can authorize execution through Hazlo. A rejected, cancelled, failed or unverified result cannot become a follow-up source.

## Program connections and result scope

| Program | Connections | Actual workflow |
|---|---|---|
| FreeCAD / CADPilot | **Elegir FreeCAD…**, then **Seleccionar documento guardado e inspeccionar…** for an existing part. | Measured saved-document primitives and generated solids; reviewed native edits produce separate FCStd/STEP files. Broader arbitrary modeling and live/unsaved editing remain unfinished. |
| CADCraft | **Elegir CADCraft…**, then **Seleccionar dibujo…**. | Native catalog/entity inspection, reviewed geometry plans and saved-copy conversion. Supported plans are bounded; complex fidelity and broader parameter coverage remain open. |
| Arduino CLI | **Elegir Arduino CLI y su entorno…**. | Uno compilation with the provisioned AVR core; verified sketch/HEX/ELF/compiler report. No upload or physical behavior test. |
| Arduino Agent | Start and authenticate the separately installed Agent, then **Conectar Arduino Agent** and **Seleccionar sketch…**. | Selected-sketch copy editing and native readback. No compilation/upload. The real conversation fixture is built but unexecuted. |
| KiCad / Konnect | **Elegir KiCad…**, **Elegir Konnect…**, then **Seleccionar esquema…** or **Seleccionar PCB…**. | Schematic value edits, PCB moves and ERC/DRC/export copies. Moves can disconnect tracks; findings require review. |
| OrcaSlicer | **Elegir OrcaSlicer…**, then **Seleccionar modelo y perfiles…**. | Confirm the STL scale, selected profiles, bed and temperature. Generate checked 3MF/G-code files without printer control. |

A saved source is not an observation of unsaved editor changes. Optional **Leer PCB abierta en KiCad…** requests a supported authenticated IPC observation; its endpoint/token are connection configuration, not chat content. This path is read-only, refuses saved-file fallback and does not grant a live edit. Positive current native-session acceptance and live mutations remain unfinished.

For KiCad initial checks, the AI inspects the selected source and complete supported dependencies before proposing ERC/DRC/exports. Confirm report coordinate units; these do not rescale the design. PCB checking can refill zones and save a checked copy. Other follow-ups bind the completed job/report hash, inspect the updated saved source and require new approval.

Verified PCB report responses redact the internal source folder. The result hash still binds the actual saved report; the returned-text hash identifies the redacted projection. Original reports/native artifact validation remain intact. Changing source, project or copied project folder requires fresh context rather than reusing an old capability.

## Conversation, cancellation and recovery

Visible messages are saved separately by project folder and AI choice. Restarting can carry those messages into a fresh client session; it does not restore an approval, pending question or authoritative document snapshot. The AI must inspect current tools/results. **Nueva conversación** clears the selected conversation; older supported histories are available for explicit message-only copying rather than automatic replay.

Use **Detener** for chat, **Detener inspección** for supported source readers, and the running-job cancellation control for native operations. Windows pickers have their own Cancel button. Wait for a terminal state before another edit. Completed publication can win a cancellation race; uncertainty requires reconciliation.

## External tool protocol and evidence

The bundled hazlomexa-tools connector also supports restricted MCP over an authenticated local Windows named pipe. The helper must match the desktop build and sit beside it. A connection is tied to one project/folder; closing or switching the project revokes it. Temporary connection configurations/tokens must remain private. The current simplified chat uses the integrated provider routes; the old expanded external-configuration panel is not a current setup step.

See [client protocol evidence](research/agent-client-protocol-2026-10-11/README.md), [Codex readback](research/codex-readback-2026-10-12/README.md), [Claude conversation prerequisite](research/claude-real-schematic-chat-2026-10-13.md) and the [current gate ledger](implementation-gates.md). Older receipts retain their exact versions, tool counts and package identities.

The rebuilt unsigned installer candidate **6a2f7ef9…** passes build, privacy scans and host release-helper checks; its fresh Windows guest package is prepared, unlaunched. Current native approvals, installed lifecycle/ordinary-user checks, Arduino Agent conversation, other-provider parity and live editing remain unfinished. Historical installed successes do not certify this package.
