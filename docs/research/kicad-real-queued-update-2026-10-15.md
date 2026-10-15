# Real KiCad read succeeds; footprint transaction timing differs

The owned KiCad 10.0.7 PCB editor now opens its original board without the earlier setup prompt. Its isolated profile already enables the API; no security/privacy settings were changed. The native board window was observed directly. Restarting only this unchanged owned board with its private temporary directory gives the existing positive IPC gate the explicitly isolated endpoint it expects.

`actual_live_pcb_observes_owned_editor_without_saved_fallback` passes through the production host and embedded official-client worker. Selection and fresh refresh agree on native document/state hashes, components are present, source paths stay out of the response, the saved bytes stay unchanged and no job is created. This supersedes the earlier unrun positive-read prerequisite.

## Native move regression

The new explicitly ignored `actual_live_pcb_approved_owned_editor_moves` gate intends to run two approved production workflow moves on this disposable board, first in mm and then in inches, with source preservation and schema-14 recovery. It currently fails on the first move: the workflow reports Failed, not Completed. No native PCB writer is registered in the agent catalog.

A fresh native read after failure exactly matches the proposal's whole-board state. Read-only diagnostics confirm that the target is explicitly unlocked and that the host's before-move validator accepts the original state. A separately instrumented owned diagnostic, using a fresh observation each time and the unchanged production worker logic, locates the failure: BeginCommit and UpdateItems both succeed, but the whole-board snapshot taken before EndCommit is byte-for-byte unchanged. The after-move verifier therefore rejects it. The worker sends Drop and independently verifies the original snapshot again; the diagnostic returns rolled_back. Saved bytes are unchanged. Fixed diagnostic labels, rather than upstream exception/log text, were used; raw snapshots and protocol evidence remain private.

This matches [KiCad 10.0.7's native footprint update handler](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/pcbnew/api/api_handler_pcb.cpp): for footprints it queues removal of the old item and addition of the replacement in the current commit. Its [transaction handler](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/common/api/api_handler_editor.cpp) pushes those pending changes on EndCommit. The pre-commit serialized board is therefore not the proposed final board, even though the update response returns the replacement item. The controlled server's eager update behavior did not model this real-editor distinction.

## Next implementation requirement

The live worker must distinguish a queued native replacement from an already applied edit. It must continue binding the exact native instance/document/original state, independently validating the approved replacement, and refusing any unrelated concurrent change before commit. Final whole-board and component readback still need verification after the real commit; a lost reply or failed final verification must remain uncertain, with fresh inspection and no blind rollback/retry. Do not simply accept the unchanged pre-commit snapshot as successful movement or remove full-state verification to pass the test.

Actual PCB editing/follow-up, native Undo, concurrent-user changes, cancellation/reconciliation, chat writer registration and installed acceptance remain unfinished. The failed real-editor regression is retained explicitly rather than substituted with a passing controlled-server result. The full application objective remains unfinished.

Regular desktop validation still passes: 172 tests, with 56 environment-dependent tests ignored, and Clippy with warnings denied. Those checks do not supersede the explicitly run native move failure above.
