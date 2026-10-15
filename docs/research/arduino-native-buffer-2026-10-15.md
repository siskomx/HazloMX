# Arduino Agent native buffer companion

The simple chat flow needs to edit the IDE's current buffer, including unsaved work. The existing Agent saved-copy integration remains available; this checkpoint does not enable a live Agent chat tool.

## Native interface decision

Research is pinned to Arduino Agent 0.7.1, commit `fea5bb6a28d689a2ddbd1c635e2e187703c22549`. Its [MCP server](https://github.com/mixelpixx/Arduino-Agent/blob/fea5bb6a28d689a2ddbd1c635e2e187703c22549/arduino-mcp-extension/src/node/mcp-server.ts) reads and writes sketch content through the filesystem. Its [frontend file-change handler](https://github.com/mixelpixx/Arduino-Agent/blob/fea5bb6a28d689a2ddbd1c635e2e187703c22549/arduino-mcp-extension/src/browser/mcp-frontend-contribution.ts) reopens and reverts the editor after that write. This is unsafe as a live edit of a buffer containing unsaved user changes. The server's selected-sketch override also cannot establish the currently active buffer.

The bundled IDE uses Theia 1.57.0 with VS Code extension support. A small companion uses its native `TextEditor.edit` API. The pinned [native implementation](https://github.com/eclipse-theia/theia/blob/v1.57.0/packages/plugin-ext/src/main/browser/text-editor-main.ts) checks the model version and supports undo stops. Theia documents [unpacked extension deployment](https://theia-ide.org/docs/authoring_vscode_extensions/); the Agent bootstrap selects the profile's own plugins directory.

## Implemented scope

- Inspect the active local `.ino` buffer through the IDE API. Return source, dirty state, line ending, native revision, content digest and opaque instance/document identities; omit its private URI.
- Apply a bounded source replacement only when the instance, open-document incarnation, native revision and digest still match. Reject no-ops, incompatible line endings, oversized inputs, arbitrary paths and unrelated actions. Native undo stops surround each edit.
- Confirm the exact source and greater native revision after editing. Lost or conflicting replies remain uncertain, without automatic retry or rollback over user work. There is no file save, reload, compilation or hardware action in the companion.
- Authenticate a bounded, one-request Windows named-pipe connection before accessing the buffer. Discovery and its random bearer token stay in an explicitly supplied private launch directory. The companion is dormant without that directory. Installed-session discovery and the Hazlo approval/job/chat route remain unfinished.

The implementation uses the existing Node runtime and standard library. It adds no editor, model provider or general execution interface.

## Verification

`node --test tests/arduino/native-editor.cjs tests/arduino/native-editor-transport.cjs` passes all eleven cases. These cover a dirty buffer, edit/follow-up/undo, stale and reopened documents, native revision races, concurrent edits, uncertain replies, line endings and the actual authenticated named-pipe entry against a controlled editor API. These are controlled tests, not actual IDE acceptance.

The owned Windows supervisor builds and passes Clippy. It reuses Hazlo's existing process-tree ownership and cancellation, verifies the official runtime executable, preflights the private CLI configuration, bounds the run and stops its own descendants on timeout, explicit stop, probe failure or memory pressure. The preparer verifies the archive and creates a disposable portable runtime/profile under ignored `target/`; it changes no user IDE profile. The probe is designed to use the actual companion pipe, preserve a preexisting unsaved note, verify native Undo and make two read-back edits while saved bytes remain unchanged.

## Actual IDE observations and remaining gate

The first fixture lacked the isolated profile's Documents directory and encountered repeated upstream CLI startup failures. The preparer now supplies that directory; the CLI configuration preflight passes. The IDE reports deployment of 25 plugins, compared with 23 bundled plugins, but no companion-ready record or native API result was produced. A default-port collision occurred in the first supervised run; no unknown process was stopped. Capture initially failed with desktop composition disabled. Subsequent capture returned an empty black application surface, including a fixture-only software-rendering attempt. That attempt was stopped by the supervisor's 1 GB free-memory floor. The native edit test has **not passed**.

All captures, private logs, profiles, tokens and runtime copies remain ignored. The full goal stays active. Next acceptance requires an actual ready companion, visible native sketch edits/readback/follow-up/Undo, then approved host dispatch, cancellation/recovery and chat integration. This checkpoint grants no live Arduino capability to the product catalog and claims no current installer acceptance.
