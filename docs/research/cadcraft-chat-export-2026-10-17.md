# CADCraft: atomic native reads and chat-approved DXF copies

Hazlo's existing drawing chat tool now accepts a separate `export_dxf`
proposal. CADCraft serializes its current open drawing, including unsaved
geometry. Hazlo creates a new `Drawing.dxf` artifact and reopens it with the
identified sibling CADCraft CLI to compare complete entity data and native
units. Export preserves scale, source name and dirty state; it creates no
native Undo step. The existing chat approval, cancellation, result review
and project recovery paths handle this operation. No additional editor,
operation form, provider or model is introduced.

The operation accepts no destination or native command. It requires the
fresh drawing's units, refuses incomplete geometry or serialization loss
warnings, and bounds the DXF to 128 KiB and the drawing to 128 entities.
The verifier runs with an isolated profile through the existing owned
process runner. Its executable and lineage remain pinned while it runs;
timeouts, cancellation, stderr warnings and truncated output fail the job.
The receipt binds the new artifact's exact name, size and SHA256. Recovery
and artifact access check those bytes again. The open source is never used
as an output destination.

## Exact document readback

The previous adapter obtained its binding, entities and units in three
requests. The maintained native patch now offers `document.observe`, which
returns those values and an open-document lifetime ID in one GUI-thread
operation. Hazlo salts and hashes the ID within its connection, then
requires the same identity in the post-operation report. An identical
drawing closed and reopened cannot pass as the original. Older receipts
without the added field remain readable; they cannot revive a connection
or authorize a new operation.

Apply the existing [binding](cadcraft-native-binding-2026-10-16/native-binding.patch),
[profile isolation](cadcraft-native-binding-2026-10-16/gui-profile-isolation.patch)
and [read-only units](cadcraft-native-binding-2026-10-16/readonly-units.patch)
patches, then the incremental
[atomic observation patch](cadcraft-atomic-observation-2026-10-17/atomic-observation.patch).
The fourth patch also lets checked, read-only `document.bytes` serialization
run with Undo disabled or grouped; engineering edits keep their existing
Undo requirement. The dispatch regression installs a test I/O hook; actual
DXF serialization and reopening are checked separately against the native CLI.

The updated maintained GUI has SHA256
`bea64cc9f40935947bde7c37221b8ddf6e24e71d6bdf2a54b6156e4e04551bf5`.
It needs its sibling `cadcraft-cli.exe`, SHA256
`5cc67bb2265a8eae4918379ff139888e3b1b317692798982cf5d109534ba14a7`,
for export verification. Both use the pinned 0.7.0 engine source. The
previous GUI runtime remains available for the previous installer; the
installed CADCraft and default compiler remain unchanged. No runtime is
built or downloaded by the application.

## Verification and remaining acceptance

- The new native regressions first reproduce three failures. After the
  patch and test-host I/O registration, 17 binding tests and four existing
  protocol tests pass. The four-patch sequence reproduces all five affected
  source files from the pinned upstream commit.
- Actual metric and inch CLI exports reopen with identical units and entity
  data. A separately selected integration test also passes both through
  Hazlo's production verifier and owned-process runner.
- Regular Rust tests, desktop Clippy, frontend build, approval descriptions,
  bounded conversation context and all four provider choices in the chat
  IPC fixtures pass. The context check caught expanded instructions
  displacing a user constraint; shorter CADCraft guidance restores the
  existing history-retention behavior without increasing its limit.

The [checkpoint](cadcraft-atomic-observation-2026-10-17/checks.json)
records exact counts and scope. Windows reports a locked desktop, so the
updated actual native-window edit/export/reopen sequence and real approval
buttons remain pending. The extended owned acceptance example is ready;
the CLI test and IPC fixtures do not substitute for that acceptance. A new
real-provider conversation and installer are also pending. The previous
[installer's acceptance](cadcraft-chat-package-2026-10-16.md) predates these
changes. Native source Save/Save As, more editing commands and the other
program gaps remain unfinished. Full upstream CI and upstream publication
were not performed. The full application goal remains active.
