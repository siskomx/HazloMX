# First workflow: describe, review, check

Hazlo Mexa is a chat for working through your engineering programs. Describe what you want to create or change and what it is for. The AI inspects the selected document, explains what it found and asks for missing information. The engineering program performs the approved operation; Hazlo checks its saved result.

## Connect and make a change

1. Open **Conexiones** and create or open a local NTFS project folder.
2. Choose your configured AI. Codex and Claude Code use their installed clients and your own sign-in; Ollama uses an already configured supported local/cloud profile. See [agent setup](agent-tools-setup.md). Cloud choices send selected context to that provider.
3. Choose the program and its installed runtime. For an existing design, select its saved document. Use a small owned copy for your first workflow.
4. Describe your intent in chat. You do not need internal object names, component UUIDs or tool parameters. Let the AI inspect the source and identify the target. Give missing measurements with explicit units. If you do not know a dimension, say so; an image or a guess is not a verified measurement.
5. Ask it to prepare the change. Review the actual old/new values, units and output scope in Hazlo. Choose the approval button only when these match your intent. Preparing a proposal does not approve or execute it.
6. After the program finishes, Hazlo checks the saved files and asks the selected AI to read the verified result. Review any native findings. Ask a follow-up in the same conversation; the AI must use the completed result rather than silently reverting to the original.
7. Use **Resultados** to inspect the outputs. Reopen the project to recover completed operations. A changed program/source requires fresh inspection; conversation history is context, not approval or proof of current document state.

## Example conversations

| Program | Start with | Then clarify |
|---|---|---|
| FreeCAD / CADPilot | “Explain the objects in this saved part. I want the main box taller.” | Choose the observed object and confirm the new height, such as 2 inches. Preserve its measured width and length. |
| CADCraft | “Explain this drawing. Move its horizontal line to the right.” | Identify the line from actual geometry and confirm a distance, such as 1 inch. See the [drawing demo](drawing-workflow-demo.md). |
| Arduino CLI | “Make the built-in LED blink. Ask me which board and rhythm.” | Confirm the supported board and timing. The current compiler supports Uno; compilation does not upload or test physical behavior. |
| Arduino Agent | “Read this sketch and explain its timing. I want it twice as fast.” | Confirm the intended on/off times. Connect the running authenticated Agent first. It edits a saved copy without compiling or uploading. |
| KiCad / Konnect | “Explain the connectors on this board. I want to move one.” | Choose an observed connector and confirm its destination and units. Moves can disconnect tracks; review native findings. Schematic value edits and ERC/DRC/export checks are separate workflows. |
| OrcaSlicer | “Explain this model and the selected printer/material profiles.” | Confirm the STL's units, bed and temperature. Hazlo produces checked 3MF/G-code files without printing or controlling a printer. |

These are examples of intent, not a special command grammar. The AI can ask another question when the target or measurement is ambiguous. See [verified combinations](supported-combinations.md) for operation and runtime restrictions.

## Stop and preservation

Before approval, cancel a proposal to leave it unexecuted. During supported source inspections, use **Detener inspección**; Windows pickers have their own Cancel button. During chat or a native job, use its Stop/Cancel control and wait for the final state. A cancellation request that loses a publication race cannot undo a completed output. Reconcile failed or uncertain writes before another edit; an interrupted conversation does not prove success.

Original saved sources and prior published results are preserved by the supported copy workflows. Open or edit the resulting files in their own programs. General live/unsaved editing remains unfinished; optional KiCad IPC reading grants observation only.

## Current acceptance limits

The October 14 installer candidate is **6a2f7ef9…**, unsigned version 0.1.0. It builds with 26 tools, passes binary privacy scans and release-connector host checks, and has a prepared Windows guest test that the user currently keeps pending. Its unbundled release passes a [real native Arduino chat workflow](research/chat-release-native-2026-10-14.md), including approvals, denial, result review and restart recovery. Current installed lifecycle/UI acceptance has not run. WebView2 is required; a missing prerequisite requires network access during installation. Engineering runtimes are separate prerequisites.

Real existing-cloud backend conversations pass for FreeCAD, CADCraft, Arduino CLI, KiCad schematic/PCB and Orca file slicing. These use backend fixture approvals and do not establish native approval or other-provider parity. Arduino Agent's real conversation gate is built but awaits execution. The [implementation gates](implementation-gates.md) retain exact evidence and unfinished work; historical installer receipts certify their own packages only.
