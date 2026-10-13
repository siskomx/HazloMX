# PCB discovery and desktop movement workflow

October 12, 2026. The desktop now has a ninth workflow, **Mover componente · PCB**, backed by the [verified PCB job/approval/publication/recovery adapter](../konnect-pcb-workflow-2026-10-12/README.md). This source build gathers selected-board context and prepares an approved copy move. It does not complete natural-language agent proposals, live editing or installer acceptance.

## User flow

Open the project, choose the PCB workflow and use **Abrir PCB y ver sus componentes** to select the source folder and saved PCB file. Discovery works before Konnect/KiCad runtime selection. The list shows native values, footprint descriptions, board side and measured positions; standard resistor/capacitor/test-point footprints receive Spanish type labels. Internal UUIDs supply selection values without becoming an input the user must know.

Choose the component, see its current position, then confirm a destination from the PCB origin in millimeters, inches or mils. Destination fields start blank. Changing units preserves an entered physical destination, and current positions are displayed in the chosen units. Choose the identified Konnect/KiCad runtimes and prepare the before/after proposal. Approval wording includes zone refill, native saved-copy checking, DRC/export and the risk of disconnected tracks. The app preserves the original and exposes the final saved PCB, DRC findings and exports separately.

The chat begins with the user's intention and encourages discovery before identifiers or measurements. Its guided PCB replies currently gather context and ask for missing position confirmation; they do not interpret arbitrary placement requests or infer coordinates. The controls disclose that limitation and that discovery reads the saved board, so open KiCad changes must be saved first. Agent-driven placement and unsaved/live state remain unfinished.

## Bound selection and commands

`DiscoveredPcb` stores a random selection ID, project ID, source name/hash and literal footprint inventory. Source paths stay in the desktop-owned `HostTools` object and are omitted from serialized discovery. A source must be a regular selected `.kicad_pcb` file inside the chosen, pinned folder. Discovery refuses a serialized inventory above 1 MiB rather than sending an unbounded list to the frontend.

`prepare_pcb_move` accepts only the selection/component identities, typed units and numeric destination. `HostTools` supplies the reference, UUID, measured original position and source paths from the selected inventory. The shared Workflow verifies project binding and the captured PCB hash before minting an approval. Re-selecting a board invalidates the earlier selection ID, changing projects invalidates its context, and a changed saved board must be inspected again. Forged component IDs are refused.

The two new commands, `discover_pcb` and `prepare_pcb_move`, are registered in the native Tauri handler, build manifest and bundled-main-only capabilities. They are not exposed as general agent filesystem tools. The current ten-tool MCP catalog does not yet provide PCB discovery/proposals.

## Evidence

The actual Konnect 0.13.0/KiCad 10.0.7 backend gate now starts from `HostTools::select_pcb`, verifies J1's actual value and board side, and prepares the move through the same discovery method the desktop command uses. A mils destination is converted to `(203.34, 155.33)` mm. Native application/check/export/readback, denial, one-use approval, occupied-destination preservation, pre-dispatch cancellation and four-run recovery pass again. Final owned project: `C:/Users/USER\AppData\Local\Temp\hazlomexa-workflow-0c177e8b-4c1b-45a6-b684-c79429f5738d`. [Status](status.json), [completed result](Completed/result.json), run snapshots and [file hashes](file-hashes.json) retain that proof. Originals remain byte-identical.

The regular Rust suite passes 108 tests with 31 separately scoped ignored entries. New checks cover literal inventory/value/side reading, hidden paths, invalid and stale selection/component identities, changed project and stale captured board hash. Replay of the final native checked board still passes with the expanded value/layer inventory comparison. Desktop/custom-protocol Clippy, formatting, production TypeScript/Vite and desktop/helper builds pass.

The headless browser regression passes all nine workflows. PCB cases cover starting without known identifiers, runtime-independent discovery, type/value/side descriptions, literal injected text, blank destinations, inch-to-mils conversion without changing physical location, concrete before/after review, disabling edits while approval is pending, DRC warnings and saved-board artifact actions. Restored PCB history and plain native-summary display also remain covered. IPC is mocked. The retained [browser view](pcb-workflow.png) was visually inspected; it is not native-window acceptance.

Native project/source/runtime picker acceptance for this ninth workflow, agent PCB tool access and natural-language placement, live IPC/unsaved-state/busy/interruption/uncertain-write handling, updated installer and second-Windows tests remain unfinished. The full application goal stays active.
