# KiCad native chat proposals — October 15, 2026

The shared chat catalog now prepares approved footprint moves in the explicitly connected, open KiCad PCB. Hazlo remains the chat; KiCad owns the document, visible changes and Undo. The agent resolves component identities and coordinates internally and asks about the intended piece, direction, distance and units in ordinary language.

## Implementation

`propose_selected_live_pcb_move` is available only in the KiCad program scope. It requires the selected connection and freshly inspected instance, document and whole-board fingerprints. The host rereads and compares those bindings before preparing the existing one-use human approval. It accepts millimetres, inches and mils, validates conversion precision before normalizing floating-point noise to the native nanometre grid, and uses the existing native transaction and independent final whole-board verifier. It grants no arbitrary path, automatic approval, save, routing or hardware access.

The chat advertises the native connection accurately and includes only a native PCB job as its current job in that context. Shared provider guidance requires current native inspection and readback. Approval shows the relative movement in ordinary language, exact before/after positions, and that tracks stay where they are.

## Actual Windows editor evidence

The owned KiCad 10.0.7 editor, pinned official Python client and frozen production worker passed:

- Three actual shared-tool proposals: 1 mm, 0.1 inch and 10 mils to the right, with no edit before approval, verified receipts, fresh native readback, refusal of stale requests and three recovered completed jobs.
- Three observed native Undo actions, each followed by the production reader comparing the whole-board fingerprint. The final state matches the pre-test unsaved board; the saved source bytes remain unchanged. One initial keyboard attempt had no effect because the canvas lacked focus; a fresh independent read confirmed that before refocusing and retrying.
- An intervening approved edit by a separate native client causes the older proposal to fail without overwriting that client's edit or publishing a successful receipt. This tests another client, not a human mouse edit.
- Cancellation after observing a real worker process stops it, publishes no successful receipt and preserves the intervening edit. The test does not establish which mutation delivery phase had been reached. Failed, completed and cancelled jobs recover in their correct phases. A native Undo restores the intervening test edit before subsequent move testing.

Approvals in these editor gates use the existing backend fixture. Raw native observations, private profiles, captures and logs remain under ignored `target/`.

## Regression checks and limits

All 173 regular desktop Rust tests pass serially; 58 native/integration tests remain opt-in. The initial parallel run passed 171 but exceeded five-second limits in two existing process lifecycle tests. No timeout was weakened. Desktop library Clippy, the production frontend build, natural-language approval facts and the four-provider chat handoff fixtures pass. Browser handoffs use IPC fixtures, not real provider conversations. The native worker source is unchanged from the separately tested fourteen-case queued-transaction correction.

Real-provider conversations, Hazlo's native approval UI for this flow, late-delivery cancellation, manual concurrent editing, broader native operations and the current Windows installer remain unfinished. Moving a footprint does not reconnect tracks or establish DRC correctness. This checkpoint enables the verified native proposal route; it does not complete the application goal.
