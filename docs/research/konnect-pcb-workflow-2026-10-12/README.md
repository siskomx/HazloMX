# Konnect PCB jobs, publication and recovery

October 12, 2026. The [prior PCB runtime gate](../konnect-pcb-2026-10-12/README.md) now connects to the shared backend through `konnect_pcb.rs`, `Workflow::propose_konnect_pcb`, `OperationInput::KonnectPcb` and project snapshot version 11. PCB creation/discovery controls and conversational proposals are not yet exposed in the desktop. This is actual backend and restored-history progress, not a completed PCB application workflow.

## Applied operation

An explicitly provided source folder/PCB is captured before a proposal. Reference, UUID, measured current X/Y, destination, units and identified Konnect/KiCad runtime selection are bound to the proposal. Stale positions are refused. The existing one-use approval, project checks, owned cancellation, reserved staging, whole-directory publication and recovery paths are reused. Version 11 is accepted only with the PCB input; downgrade to MCP snapshot version 10 is refused. Existing schematic snapshot version 9 and MCP history version 10 retain their scope.

Konnect's owned stdio session is shared with the schematic adapter, with separate exact schema/tool checks and operations. PCB dispatch uses only the identified `move_component` and `get_board_info` interfaces on a closed private copy. Native metadata and the complete board syntax tree are re-read after the effect; a client acknowledgment is insufficient. Runtime identities are checked and pinned before effects. Session shutdown is independently observed.

The edited copy then enters the existing KiCad captured-project check/export adapter. Its saved checking copy is compared against the edited copy for every footprint reference, UUID, library, X/Y and rotation. KiCad legitimately refills zones and serializes this checking copy; the exact Konnect edit and the native saved/checking output remain separately recorded. The published **KonnectEdited** artifact points to `Verification/Checked/<entry>`, the final native checking copy. DRC and export artifacts are exposed through the existing **KicadReport** and **KicadOutputs** formats, while **KonnectReport** contains the complete result and findings.

Recovery cross-binds request, source dependencies, edited bytes, verification input, units/export layers, native result, all output identities and the exact file set. It refuses missing/foreign output, forged identity, altered dependencies, changed units/layers and a summary that contradicts the native report. Existing `read_verified_result` recognizes a completed PCB result after the same project/artifact validation; new PCB discovery/proposal tools are not yet supplied.

## Actual gate and retained evidence

Final owned project: `C:/Users/USER\AppData\Local\Temp\hazlomexa-workflow-fcb86974-250c-4f56-9f3a-a8af2fe3313c`.

- An unapproved request is cancelled; its approval is unusable.
- A mils-report proposal moves J1 from `(200.8, 156.6)` to `(203.34, 155.33)` mm. Approval is consumed once, Konnect applies the exact move, KiCad reads/checks/exports/saves its checking copy, and the full result is accepted and published.
- The saved PCB re-reads at the intended position. DRC reports one missing-connection error and seven warnings, with disabled checks retained; successful editing does not mean readiness to manufacture.
- A second approved run reaches verified files but an occupied destination prevents publication. The pre-existing `preserve.txt` remains unchanged and the verified stage is retained.
- A request cancelled after `begin` and before execution remains cancelled without publication.
- Original PCB, schematic, project and worksheet files stay byte-identical. Project reopen returns four histories: one completed, two cancelled and one failed with verified stage files. The completed saved PCB artifact remains byte-identical after reopen.

[Status](status.json), [completed request](Completed/input.json), [completed result](Completed/result.json), [native DRC](Completed/Verification/Check.json), [failed verified stage](FailedStage/result.json), run snapshots and [file hashes](file-hashes.json) preserve the evidence. Only owned fixture/project data is retained; runtime profiles and connection configuration are excluded.

The actual existing schematic workflow also passes after the session refactor, including publication failure, cancellation and four-run recovery. Its separate owned project is `C:/Users/USER\AppData\Local\Temp\hazlomexa-workflow-757e5651-e75b-4b38-b8c0-469014f147b5`.

## Validation and limits

The regular Rust suite passes 107 tests with 31 separately scoped ignored entries. The new ordinary tests cover stale proposal binding, snapshot-version downgrade refusal, cancellation recovery, published native-summary reading and replay of the real saved artifact set with forged identity/summary/units/dependencies/layers. Desktop/custom-protocol Clippy, formatting and the TypeScript/Vite production build pass.

Desktop contracts, status/position descriptions, restored history, approval wording and artifact buttons recognize the PCB variant. The history points to the saved PCB, DRC and export actions instead of a schematic netlist. The existing summary command accepts the PCB result only after full output validation and displays native findings. The eight-workflow browser regression and added PCB history checks pass: actual backend snapshot rendering, position description, correct DRC action, no netlist action, and a plain findings dialog with the error and report units. IPC is mocked; this cannot establish native picker or installed behavior. Initial fixture timeouts came from injecting history while polling was idle and attempting a control behind its open result dialog; the test now enables the existing polling connection and closes the dialog before continuing.

Reproduce the real backend gates after provisioning the identified runtimes:

```powershell
rtk cargo test -p hazlomexa --lib actual_konnect_pcb_workflow_publication_and_recovery -- --ignored --nocapture
rtk cargo test -p hazlomexa --lib actual_konnect_workflow_publication_and_recovery -- --ignored --nocapture
```

PCB discovery, metric/imperial position-entry UI, chat/proposal tools, live in-flight cancellation/uncertain-write reconciliation, exact-board live IPC/unsaved-state checks, native/installed acceptance and the complete installer remain unfinished. The full application goal remains active.
