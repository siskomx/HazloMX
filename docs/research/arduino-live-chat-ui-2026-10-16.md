# Arduino: visible chat edits, follow-up and native save

The actual Hazlo chat completes two separately approved edits in the same
open Arduino Agent sketch, then a separately approved native save. The
[sanitized receipt](arduino-live-chat-ui-2026-10-16/receipt.json) identifies
the tested debug executable. Raw profiles, conversations, registrations and
logs remain private under ignored `target/`.

## Observed sequence

The owned portable Arduino Agent 0.7.1 fixture first passes its native
companion/edit/undo checks and production host discovery without copying
registrations or overriding their discovery path. These setup operations
leave the sketch at a 100 ms delay with an unsaved note; they are separate
from the three Hazlo UI jobs below.

1. Choose the existing configured Ollama cloud route and Arduino Agent in
   Hazlo. Ask to change the open sketch's pause to a quarter of a second,
   preserving the note and leaving it unsaved. No tool names, document
   tokens or millisecond values are supplied in the request.
2. The agent reads the current buffer and prepares a readable 100-to-250 ms
   proposal. Independent native inspection confirms 100 ms before approval.
   Approve using Hazlo's actual button. Arduino visibly shows `delay(250)`;
   native readback confirms the same document, note and unsaved state.
3. In the same conversation, ask for half a second without saving. The
   proposal starts from 250 ms. Independent inspection confirms no early
   change; a separate UI approval visibly changes Arduino to `delay(500)`.
4. Ask to save the current sketch, including the note. Review and approve
   a separate save proposal. Arduino clears its unsaved indicator. An
   independent native read reports a clean buffer, and the actual disk
   bytes exactly equal that buffer, including the note and 500 ms delay.

Seven independent native inspections retain the same instance and document
token. Versions advance from 8 to 9 to 10 for the two edits; saving retains
version 10. Normalizing only the delay value leaves identical source across
all observations. The original disk bytes remain unchanged until save.
Two verified native-edit receipts and one native-save receipt correspond
to three completed schema-15 jobs. Automatic agent result reviews and all
three completed history entries are visible in Hazlo.

After closing both supervised test applications, a fresh Hazlo window on
the same project restores all three jobs. Its verified save-report button
reports dispatch to Windows; an external report viewer was not observed.
All seven project files remain byte-identical through recovery, excluding
the transient writer lock. This checks job/report history, not conversation
rehydration or restoring the IDE's previous process. The recovery window
is stopped afterward.

## Prompt handoff and limits

The earlier Windows Security prompt was handled by the person. This retry
reuses the same owned runtime and discovery-tool path, archives the previous
private attempt and starts fresh owned editor profiles. No security prompt
appears during this attempt; no permission or security setting is automated.
The existing 1 GiB memory floor, unlocked-desktop requirement and bounded
supervisor remain unchanged.

This is real debug-app/native-editor UI acceptance through the existing
Ollama route. It is not installed conversation acceptance, a Claude/Codex
conversation, compilation, hardware upload, arbitrary-sketch fidelity or
in-flight cancellation proof. No exact model-tool-call trace was recorded;
the evidence is the observed UI, independent native state and verified jobs.
The full multi-program goal remains unfinished.

## Installer follow-up

The [rebuilt candidate](arduino-live-chat-ui-2026-10-16/package-candidate.json)
from `5871c0a` includes the chat error-placement fix as well as the runtime
compatibility change used by the successful native conversations. Packaging,
unchanged tracked-input checks, app/helper profile-path scans and the actual
release helper's 33-tool protocol check pass. A retained installer copy is
bound by SHA-256 independently of the mutable build-output folder.

Two initial guest launches refuse before creation below the unchanged 3 GiB
memory prerequisite. Once memory recovers, a fresh guest starts above that
floor and passes the [scoped lifecycle check](arduino-live-chat-ui-2026-10-16/package-lifecycle.json):
previous-package installation and upgrade, same-version update, exact app
and helper identities, both installed/updated 33-tool transports, hidden
startup, uninstall and project/app-data preservation. Independent host
hashes match all three exported project files. The disposable guest is
stopped and its absence is checked.

This does not repeat engineering workflows, ordinary-user acceptance or
interactive conversation on the installed candidate. Earlier package UI
and backend evidence remains separately identified.
