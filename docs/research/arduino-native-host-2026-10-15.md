# Arduino native editor host transport

The Rust application can now communicate with the [native buffer companion](arduino-native-buffer-2026-10-15.md) through its actual authenticated Windows pipe. This is an internal transport checkpoint; no live Arduino chat tool or approval bypass is registered.

The host loads a bounded private discovery file using the existing local-path and pinned-directory checks. Connection credentials cannot be serialized or printed through the connection type. Only the companion's local, instance-specific pipe namespace is accepted. Replies have a strict schema, a 512 KiB limit and a fifteen-second deadline. Source text remains explicitly untrusted data.

Inspection verifies the instance, document identity, native revision, line ending, bounded source and independently recomputed source digest. An edit request carries the exact prior instance/document/revision/digest. A completed reply must bind the job, prior digest, same document/name/line ending, increased revision, exact requested source and a native-edit confirmation. Dirty state is read from the IDE; the host does not promise that the IDE's own autosave is disabled.

Cancellation before dispatch refuses the request. After an edit write is attempted, cancellation, lost replies, malformed success, missing confirmation and conflicting readback remain uncertain. There is no automatic resend, rollback, file write or reload. A confirmed native refusal remains distinct from an uncertain edit. The caller still must obtain the existing human approval bound to the exact observation before dispatch; that product integration is unfinished.

## Evidence

- Seven Rust tests pass against actual Windows named pipes: strict observation and receipt binding, authenticated framing, source-preserving edit/readback, stale response handling, pre-dispatch cancellation, cancellation after delivery, held/oversized replies and bounded discovery. An initial test lacked a Tokio reactor; running that pipe test inside its required runtime fixes the test harness, with no production workaround.
- The compiled `owned_agent_editor_host` example passes against the actual companion entry point in the Node transport fixture. The Rust client inspects a dirty controlled buffer, applies a timing change, confirms stale-request refusal, rereads it, applies a follow-up and rereads the final state. The controlled editor receives exactly two additional edits with native undo-stop options; the prior unsaved note remains present. All eleven Node buffer/transport tests pass in this mode.
- Library and example Clippy pass without warnings. Source discovery credentials, fixture directories and raw evidence remain private; the example is limited to a marked owned fixture under ignored `target/`.
- The full serial desktop-feature Rust library suite passes: 164 regular tests, 55 ignored integration tests. This retains the existing application checks alongside the seven new host tests.

Build the example with `cargo build --manifest-path apps/desktop/src-tauri/Cargo.toml --example owned_agent_editor_host`. Set `HAZLOMEXA_NATIVE_EDITOR_HOST` to that compiled example when running `node --test tests/arduino/native-editor.cjs tests/arduino/native-editor-transport.cjs` to include the actual Rust-to-companion exercise. Without that explicit executable, the ordinary Node suite does not claim Rust host coverage.

These checks use the real transport and production parsing/verification with a controlled IDE API. They do **not** establish actual Arduino Agent editor behavior. The latest owned IDE run is terminal after its memory-floor cancellation; it is not a live wait. Actual visible IDE edit/Undo/readback, installed-session discovery, approved application job routing, recovery and chat/installer acceptance remain required. The full application goal stays active.
