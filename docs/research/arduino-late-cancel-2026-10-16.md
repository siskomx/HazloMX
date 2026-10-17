# Arduino late-delivery cancellation checkpoint

The production Rust workflow and authenticated companion pipe now have a controlled test for an edit that finishes after Hazlo stops waiting. No production editing or cancellation behavior changes.

The existing `owned_agent_editor_host` example prepares a third native edit after two completed edits. A test-only editor API holds the native edit promise before changing the buffer. After the companion receives it, the host cancels the approved job and waits for execution to return. Only then does the fixture release the edit.

The passing assertions establish:

- Before release, a fresh native inspection still reads the previous source.
- Cancellation returns a non-busy Cancelled job with the existing warning that cancellation does not undo a native edit. The consumed approval cannot execute again.
- After release, a fresh inspection through both the Rust reader and shared chat tool observes the new source in the same instance and document, with exactly one version increment. The earlier unsaved note survives and the buffer stays dirty.
- The cancelled job has no accessible verified artifact before or after late delivery. Reading it as a completed result is refused.
- The original saved sketch remains byte-identical, the stale pre-edit proposal is refused, and the controlled editor records exactly one additional edit with no save or retry.
- Reopening the project recovers two completed jobs and the third Cancelled job using schema 15.

Validation passes for the existing edit/follow-up host mode, the separate native-save mode and the new delayed-cancellation mode. All 20 regular companion/profile/transport Node tests pass, and the owned host example builds and passes Clippy with warnings denied.

To reproduce the additional mode, build the existing example, set `HAZLOMEXA_NATIVE_EDITOR_HOST` to its compiled executable and `HAZLOMEXA_NATIVE_CANCEL_TEST=1`, leave `HAZLOMEXA_NATIVE_SAVE_TEST` unset, then run `node --test tests/arduino/native-editor-transport.cjs`. Without the host executable, the ordinary Node transport test does not exercise Rust cancellation. The delayed-delivery mode is refused for a real IDE fixture or combined save mode.

Raw output, fixture profiles, connection records and sources remain in ignored `target/`. This checkpoint uses the production workflow and companion transport with a controlled IDE API. It does not prove real Arduino UI cancellation, model recovery after a late edit, native Undo after cancellation, installed acceptance or other providers. Those gates remain open; the [actual Arduino chat edit/follow-up/save evidence](arduino-live-chat-ui-2026-10-16.md) retains its original scope. Claude model testing remains pending at the user's request.
