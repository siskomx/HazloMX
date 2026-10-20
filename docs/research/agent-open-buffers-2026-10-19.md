# Arduino native open-buffer context

This checkpoint extends the existing chat inspection tool. It does not complete Arduino native working versions or the overall simple-chat plan.

## Implemented behavior

`inspect_open_agent_documents` accepts `include_open_buffers: true`. After selecting the only responding editor or a specific native instance, the host binds a read to its freshly observed active document token, revision and content hash. The companion reads the native workspace's open local text buffers synchronously, including unsaved headers, C++ files and additional sketch tabs. It refuses a changed active binding, a workspace that is not the active sketch's own directory, duplicate paths, incompatible line endings, malformed content or exceeded limits. It never reads a saved file instead of an unavailable native buffer.

The read admits at most 256 native document entries, 32 buffers in the selected workspace, 65,536 UTF-8 bytes per buffer, 131,072 combined source bytes and 393,216 encoded response bytes. Rust independently checks native identities, revision ranges, content hashes, line endings, canonical relative paths, local parent lineage and aggregate content bounds. The workspace root is used locally for validation and omitted from the chat-tool projection; connection tokens and pipe names remain private.

The returned scope is explicitly `native_open_buffers_only`, with `complete_project: false`. Closed files, on-disk dependencies, compilation, physical behavior and saved working-version reopen are not checked. Reading does not save, edit, compile, upload, change the selected document or repeat an earlier native attempt. It is also refused during a companion-owned edit/save. Existing active-tab inspection and retained-attempt recovery remain compatible. A running older companion can continue ordinary active-tab reads; expanded context requires preparing the updated owned companion and restarting its host when safe. Setup recognizes the exact previous three-file bundle, preserves its predecessor and refuses unknown or locally changed plugin contents.

## Evidence boundary

The expanded result uses the existing task/service evidence path. Native source, relative names, errors and revealing derivatives are retained locally. No field in this result is a safe declassification projection. A sharing regression checks that approving one exact snapshot does not release a later manual change, revocation prevents future reads, and changing provider does not inherit the original sharing scope. No model is downloaded or contacted by these tests.

## Native reader investigation

The installed Arduino CLI 1.5.1 was run with an isolated configuration, profile, data, downloads and temporary directories on a disposable multi-file sketch. `sketch archive` needed no selected board, core installation or compilation. It preserved exact bytes for seven synthetic files: the main sketch, header, extra C++ source, nested header, data reference, `.theia/settings.json` and `.git/config`. It omitted the synthetic `build` entry. Originals were unchanged. The [bounded native probe record](agent-open-buffers-2026-10-19/acceptance.json) contains no user paths or source bodies.

This establishes archive-reader behavior for that fixture. It does not establish a complete dependency manifest, capture unsaved native buffers, demonstrate GUI reopening, compile the sketch or certify physical function. In particular, hidden project configuration is retained by the CLI; archive success cannot be used to justify executing startup configuration. A checked whole-project working version still needs a faithful native-buffer/disk capture, dependency accounting, a safe independent native check and durable capture/check anchors after the separately retained original edit receipt.

## Validation scope

The controlled editor tests use the actual bundled companion and Windows named-pipe entry with an owned editor API fixture: all 29 Node tests pass. The rebuilt Rust-host acceptance uses that real authenticated pipe and verifies unsaved header context, original-file preservation, private-path omission and existing approved edit/readback/follow-up/save/recovery behavior. The desktop-enabled library/binary suite passes 372 tests with 76 environment cases skipped when run serially. Two existing five-second process timing tests exceeded their limits during a parallel run under load; the serial suite passes without relaxing those limits. These are protocol and host tests, not a fresh Arduino Agent window or real-provider conversation.

The owned actual-program probe now prepares a separate saved header, opens it through the native workspace API, changes its disposable buffer without saving and checks the expanded companion context before continuing the prior edit/Undo/follow-up checks. The production Rust host then checks that same header context and private-path projection. Its attempted run found Windows locked and skipped program launch before any native edit. Actual-program acceptance remains pending an unlocked desktop; preparation and syntax checks are not a pass. It never uses input automation to bypass the lock.

All-target desktop Clippy and the frontend production build also pass. Publication privacy and staged whitespace checks pass; private native logs and profiles are excluded from publication.

The full plan remains active. Whole-project working versions, remaining integration/concurrency and task-outcome requirements, actual native-window checks and the Windows release lifecycle remain outstanding. The installed app and installer are unchanged. KiCad 10 live schematics remain pending.
