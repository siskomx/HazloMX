# Arduino Agent native save — October 15, 2026

Hazlo can prepare a separate approved save of the current open Arduino Agent sketch. The person requests it through chat; the program owns the save operation and existing destination. The source code is not replaced as part of saving, and no Save As path, compilation or hardware operation is exposed.

## Implementation

`propose_open_agent_save` is restricted to the Arduino Agent tool scope and requires fresh native instance, document, version and source fingerprints plus a readable description. It accepts neither source code nor paths. The host rereads the current buffer before preparing the existing one-use approval. A clean buffer cannot create a redundant save job. The common native job pipeline carries a save flag; existing inputs and receipts omit false flags and retain their old edit semantics.

The authenticated companion calls the native [TextDocument.save API](https://code.visualstudio.com/api/references/vscode-api#TextDocument) once. It requires a matching dirty buffer, then verifies the save acknowledgement, unchanged document/source/revision/line endings and a clean final state. A bounded wait tolerates delayed dirty-state notifications without issuing another save. Changed source, save participants, uncertain replies and failures produce no successful receipt and are never automatically retried. Saving is a separate effect from native editing and creates no editor replacement or code Undo step.

The Rust host independently validates the typed save receipt and its source digest. This production receipt records native acknowledgement and final buffer state; it is not an independent raw disk-byte digest. Approval explains that saving includes the person's own unsaved changes, uses the current destination and follows IDE save preferences. Result text and report labels distinguish save from edit. Shared provider guidance asks for a separate save proposal only after an explicit request and fresh inspection, and rereads the editor after the outcome.

## Evidence

The real pinned Arduino Agent 0.7.1 editor, using the production-installed companion and its normal profile registration, completes two native edits and a separate approved backend save. Native readback confirms the same source/revision and clean state. The acceptance driver independently reads the saved UTF-8 fixture bytes: the final `delay(100)` and earlier unsaved note match exactly. Three completed schema-15 jobs recover. The owned process tree then stops successfully. Approval uses a backend fixture; no provider conversation or native Hazlo approval UI is proved by this run.

The registered shared save tool separately passes the actual compiled Rust host and authenticated named pipe against a controlled IDE API: preparing the save preserves disk bytes, approval saves once, native/disk readback matches, a redundant clean save refuses, and all three jobs recover. The native test driver now uses that registered route for subsequent real editor runs.

Controlled checks cover stale/path-bearing requests, delayed clean notifications, competing edits while a save is busy, failed saves, concurrent edits/renames, privacy of errors, forged receipts, and rejection of changed or already-clean save inputs. Twenty companion/profile/transport checks pass, including the approved save route. All 175 regular desktop Rust tests pass serially; 58 integration/native tests remain opt-in. Library/example Clippy, frontend build and focused approval facts pass.

The browser gate additionally exercises save review, details, approval/result handoff and completion text for all four configured provider choices using IPC fixtures. An intermediate run was interrupted by a source edit triggering Vite reload; it is not counted as passed. The final fixed-source browser run passes, including all four save handoffs. A fresh real-editor run also passes the registered shared save tool, exact disk readback, redundant-save refusal and three-job recovery; its owned supervisor stops successfully.

## Limits

Save As, compile/upload, hardware, other file encodings, save-participant transformations, real-provider conversations, Hazlo native approval UI, ordinary-user installed acceptance and the updated installer remain unfinished. A changed or older companion is preserved by setup rather than silently overwritten; that profile requires review before installing the updated companion. The pending firewall prompt has received no automation input. Raw profiles, source observations, protocol records and logs remain private under ignored build output. The full multi-program goal stays active.
