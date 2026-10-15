# Real KiCad commit, follow-up and native Undo

This resolves the transaction-timing failure recorded in [the earlier real-editor regression](kicad-real-queued-update-2026-10-15.md). It does not replace that retained failed-run evidence or establish completion of the full application goal.

## Worker correction

The pinned KiCad 10.0.7 footprint handler queues removal and replacement until EndCommit. The worker now verifies the original whole-board state and approved movement before beginning, checks that the native update response exactly matches the proposed replacement, and requires the board snapshot to remain unchanged while that replacement is queued. It then commits and independently verifies the actual final whole-board serialization with Hazlo's existing Rust movement verifier. Only the approved footprint's root X/Y may differ. A final native read must still match that verified state before a completed receipt is returned.

This follows the [native footprint handler](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/pcbnew/api/api_handler_pcb.cpp) and [transaction handler](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/common/api/api_handler_editor.cpp), rather than the controlled server's previous eager-update assumption. The controlled server now queues replacements and applies them on commit, with each transaction preserving its own original state for a confirmed drop.

A concurrent change before commit is uncertain and is not blindly dropped. A confirmed rejected queued replacement can be dropped only while the board still matches the original snapshot, and rollback is independently verified. Lost replies, unapproved final state, failed final verification and subsequent changes remain uncertain without retry or blind rollback over possible user edits. No saved-file fallback or save operation was added.

## Actual Windows editor evidence

The owned, already unlocked KiCad PCB window remains bound to its isolated profile, board and IPC endpoint. No security/privacy preferences were changed. The test uses a fresh Hazlo project under that owned fixture, retaining its earlier failed project and snapshots.

`actual_live_pcb_approved_owned_editor_moves` now passes two production workflow jobs in the real editor. The owned resistor moves from X 197 mm to 198 mm, then to 200.54 mm: one mm followed by 0.1 inch. The workflow verifies unchanged source before approval, consumes approval once, validates each native commit/readback, reads the verified receipt, preserves the saved board bytes and recovers both completed schema-14 jobs. Approval comes from backend fixtures, not a real model or Hazlo button click. The native PCB window visibly displays the moved resistor and unsaved state.

The owned canvas was then focused and native Ctrl+Z invoked once. `actual_live_pcb_owned_native_undo_readback`, step 1, passes through the production reader and restores the exact whole-board hash after the first job. After a second observed Ctrl+Z, step 2 restores the original native instance/document/whole-board hashes. The original saved bytes remain unchanged after both native Undo operations. This proves native transaction boundaries and restoration, not just a visual approximation of the component's position.

All 14 worker tests pass, including the actual SDK, existing Rust verifier and frozen executable over NNG. The tests cover queued behavior, stale/invalid bindings, partial replies, pre/post-commit unrelated changes, lost replies, failed rollback and private diagnostic output. The frozen helper rebuild verifies its current source, locked dependencies and self-check before embedding.

## Remaining acceptance

The live PCB agent writer remains unregistered. Actual concurrent-user and cancellation/reconciliation scenarios, real-agent conversation, Hazlo's native approval UI and current installed-package acceptance remain unfinished. Arduino, FreeCAD and the other named integrations retain their separate remaining work. Raw models, profiles, captured windows, native requests and logs remain in ignored build output.
