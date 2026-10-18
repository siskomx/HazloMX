# Native investigation recovery and portable chat history

The earlier vacuum investigation exposed generic coordination failures. This
change addresses them in the shared native workflow, rather than introducing
device-specific tools, compatibility decisions or conversation branches.

## Behavior

- MCP replies keep their existing 256 KiB per-call bound. A reviewed batch can
  retain a compact report up to 5 MiB; its limit remains sixteen change calls
  and a final readback. Publication, receipt hashing and later validation use the
  same bound. Existing reports retain their original byte identities.
- The verified-result tool returns UTF-8 pages of at most 64 KiB, with a next
  offset, total size and whole-report hashes. Guard information still refers to
  the complete original native readback. Partial preview text is explicitly
  marked; it is not represented as a complete analysis. Retrieving pages does
  not repeat the native operation.
- An approved native job that fails or is interrupted requests one agent
  explanation automatically. Its bounded diagnostic is untrusted context, not
  instructions. The follow-up forbids repeating calls or proposing another
  change, explains uncertainty and asks only for input needed to continue.
  Cancellation remains a stop, and duplicate terminal events do not repeat the
  explanation. User drafts are preserved. Native diagnostics remain in history;
  the main chat status uses plain language.
- Shared agent guidance distinguishes a connected tool catalog from a selected
  target and available analysis engine. It asks for a relevant application or
  artifact when necessary, prefers focused native queries, and allows useful
  public research to continue. It does not fabricate an artifact or promise
  compatibility with an unknown device.
- Desktop conversations are stored atomically in the selected project, separated
  by AI provider. Reads/writes bind both the active project ID and folder, accept
  bounded conversation data only, and expose no arbitrary file reader. Existing
  browser history migrates when opened in its original profile; cached history
  cannot overwrite an existing project conversation. The original database is
  retained. A fresh browser profile or another build can then open that project
  without a private database-copy workaround.

Hazlo remains a simple chat. The connected program owns its operations and native
readback. These changes add neither hardware control nor a compatibility claim.

## Checks

- 218 regular Rust tests pass with serial execution; 64 environment-dependent
  tests remain ignored. Two process-timing assertions exceeded their five-second
  bound during concurrent UI startup; the serial suite passes without changing
  those assertions. Desktop Clippy and frontend compilation pass.
- The native transport fixture passes all seven program scopes with a retained
  Unicode report larger than 64 KiB, exact reconstruction through verified pages,
  whole-report hash agreement, invalid-offset refusal, fresh-state guards,
  one-use approval, cancellation and recovery. This is protocol evidence, not
  live engineering acceptance.
- The real bundled Rea runtime, using automatic setup's compact input schema and
  disabled output-schema profile, exposes 207 tools. Its broad session query now
  retains and validates a 127,185-byte report. An initial test configuration
  omitted those schema-profile settings and correctly hit the catalog limit;
  that configuration was corrected to match actual automatic setup. No runtime
  or catalog limit was relaxed to bypass that check.
- Chat IPC tests cover all four AI choices and seven program routes, including
  automatic failure explanations, duplicate-event refusal, retained drafts and
  no raw diagnostic in the main status. Existing approval, cancellation,
  provider/project scope and result-review cases continue to pass.
- Browser storage tests cover strict legacy commit acknowledgment, failed-write
  preservation, migration to project storage, and recovery in an empty profile
  at a different origin. Backend tests cover provider isolation, reopen, corrupt
  file preservation, invalid/oversized input and foreign project/folder refusal.
- The actual original ten-turn chat was opened in its original test profile with
  the new desktop. It migrated into the project and displayed its original final
  question. No new user answer or device observation was invented.

## Release scope

The local Windows 0.1.0 installer is built from 697b162. Installer SHA-256:

`4ed3f0a13c67ee1f73a917caeb269366783e4a7e6b00fe87d735b977975fdb29`.

All 6,757 tracked inputs stayed unchanged during packaging. Builder-profile path
checks report zero matches in both release binaries, and the release helper
passes its 38-tool protocol check.

A fresh disposable Windows guest installs the previous a843549 build for the
current user, then updates to 697b162 at the same application version. New binary
identity, connector checks, process startup and uninstall pass. The project
conversation file survives update/uninstall unchanged. Four exported project
files independently match their hashes. The guest
was stopped. Networking was enabled for installer prerequisites; no accounts or
engineering workflows were exercised there, and no guest visual check is claimed.

Local update verifies the installed app/helper and bundled Rea against this
package, preserves five existing project/catalog files, and starts the updated app.
The actual production window reopens the original ten-turn conversation in its
normal installed profile using the project history, without a database copy.
The existing Codex account connects, Rea automatically exposes 207 native tools,
and the empty message box is enabled. No new answer was invented or submitted.
The simple chat is left open at its original next question.

This is a local installer candidate and installation; the public alpha.5 release
is unchanged. Physical device behavior, cloud-free scheduling and compatibility
with an unspecified firmware
are not established by these tests. KiCad 10 live schematic editing remains
deferred.
