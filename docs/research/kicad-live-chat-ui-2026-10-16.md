# KiCad: visible chat edit, imperial follow-up and native export

The actual Hazlo chat now completes two approved edits in the same open KiCad
PCB, cancels an export proposal, prepares a fresh export and verifies its
result. This closes the follow-up/export gap left by the earlier
[one-move checkpoint](kicad-native-ui-checkpoint-2026-10-16.md).
The [sanitized receipt](kicad-live-chat-ui-2026-10-16/receipt.json) binds the
tested debug executable. Raw projects, conversations, paths and logs remain
private under ignored `target/`.

## Actual conversation and native state

The owned fixture uses KiCad 10.0.7 and the existing configured Ollama cloud
route. The ordinary connection UI and native file picker select the exact
open board. Requests describe the topmost 10k resistor without supplying its
reference, coordinates or other technical identifier.

1. Request a 2 mm move right. The agent identifies R1, prepares a readable
   before/after proposal and waits for the actual Hazlo approval button.
   Independent native inspection confirms no change before approval.
2. Approve in Hazlo. KiCad visibly moves R1 from `(197, 141)` to `(199, 141)` mm.
   The native commit receipt and an independent live inspection agree.
3. Ask to move that same resistor 0.1 inches down from its current position.
   The new proposal starts at `(199, 141)` mm and converts the distance to
   2.54 mm. A separate UI approval moves it to `(199, 143.54)` mm, visible in
   KiCad and independently read back.
4. Request an editable export, then cancel its proposal in Hazlo. The job is
   cancelled, no artifact is published and independent native state is intact.
5. Request a fresh export and approve that new proposal. KiCad serializes its
   current unsaved document to a verified new editable PCB in the project.
   The source file and open native state remain unchanged by export.

There are three completed schema-14 jobs and one cancelled job. All other
components retain their full native projections throughout. Seven independent
live inspections cover preparation, both approvals, cancellation and export.
The original saved PCB bytes stay identical. The export's SHA-256 matches its
receipt; an independent KiCad CLI placement export in millimetres confirms
R1's final position, accounting for that format's opposite vertical sign.

Automatic agent result reviews are visible after each completed job. Hazlo's
history shows the three completions and cancellation, and the verified export
report opens read-only. After closing the supervised test, restarting Hazlo
on the same project restores all four history entries and report access.
All eleven project files remain byte-identical through recovery. This restores
reports and the exported copy; it does not restore KiCad's discarded unsaved
buffer. Both owned test applications and the recovery window are closed.

## Compatibility defect found and fixed

The initial attempt stopped before any mutation: installed Ollama had updated
to 0.40.5 while chat required exactly 0.35.1. Independent inspection proved the
board unchanged and zero jobs. The existing model digest, cloud route and
completion/tool capabilities still matched.

Chat now validates bounded runtime metadata and retains the existing model
digest, routing, capabilities, response parsing and host-tool checks. Exact runtime
version equality no longer rejects an otherwise compatible chat API. This is a
Hazlo compatibility policy; the [official Ollama API reference](https://github.com/ollama/ollama/blob/main/docs/api.md)
documents version metadata separately from model information and chat tools.
No provider, model, profile, dependency or download was added. The separate
legacy generation route retains its audited 0.35.1 runtime requirement and
historical audit validation.

The 0.40.5 cloud-chat regression fails before the fix and passes afterward.
Ollama-scoped Rust tests pass 18 tests with one existing ignored test, including
invalid runtime metadata and missing tool capability refusing before prompt
transmission or tool dispatch. Library Clippy with warnings denied, production
frontend, the four-route browser chat suite and the custom-protocol native
build pass. The real native sequence above then passes on 0.40.5.

## Limits

This is an owned debug/native UI test, not an installed signed-in conversation
or proof of every provider. Exact model HTTP/tool-call sequences were not
recorded for these UI turns. Cancellation is before execution; interrupting an
in-flight write is a separate gate. There is no rerouting, DRC, electrical or
manufacturing validation. The prior tested installer predates this runtime
compatibility fix and needs a later rebuild. Arduino's full visible flow and
the other integration limitations remain unfinished; the overall goal is active.
