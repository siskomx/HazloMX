# Reinspect native state after an uncertain outcome

Hazlo now requests one read-only current-state review when an approved live
FreeCAD, Arduino Agent or KiCad job fails, is interrupted or is cancelled.
Previously automatic review handled only completed jobs, and the cancellation
button discarded that review even for a write already in progress.

The existing project/folder, provider, program, revision and job-ID checks
still bind the review. It waits for a terminal job and an idle conversation.
Cancellation before approval and rejected dispatch do not start a review.
Saved-copy workflows keep their previous behavior. Duplicate terminal events
do not repeat inspection or approval, and a drafted next message is preserved.

The recovery request uses the corresponding native inspection tool, explains
that the outcome may be uncertain, and refuses to infer whether the operation
ran or was undone merely from the current state. It does not read an
incomplete job as a verified published result, retry a write or prepare a new
change. If native inspection is unavailable, the agent must say that state
is unverified. Completed jobs retain their verified-result review.

The shared Claude/Codex/Ollama instruction is updated too: verified-result
reading applies to completed jobs; uncertain native outcomes require current
native inspection. This removes the earlier conflicting general instruction.
Native execution, source binding, approvals and cancellation transports are
unchanged.

## Evidence and limits

The new browser regression fails before the fix: an approved native FreeCAD
failure starts zero inspections instead of one. The complete browser suite
passes afterward, including 72 new cases across four provider routes and
three native input types. Failed jobs carry a nonempty error. Each native
type covers failure, cancellation events, interruption, the running-job
cancel button, approval rejection and cancellation before approval: 48
terminal reviews and 24 cases with no review. The tests also check current-job
scope, preserved drafts, no approval token in prompts, no hidden request as a
user message, no automatic extra approval and no duplicate-event replay.

The initial expanded test also exposed a fixture sequencing error: it left
KiCad selected before an older FreeCAD-specific case. Restoring that explicit
fixture selection fixes the test without changing product behavior.

Production TypeScript/Vite, the custom-protocol desktop build, library Clippy
with warnings denied and 31 focused Claude/Codex/Ollama Rust tests pass, with
two existing ignored cases. The [sanitized source receipt](native-outcome-readback-2026-10-16/receipt.json)
records the tested inputs. Raw browser captures remain private under ignored
`target/`.

This proves frontend dispatch and scoping through IPC fixtures. It does not
prove a real model obeys the recovery request or establish a real in-flight
native UI cancellation. Those acceptance checks and packaging these source
changes remain open. Claude's real conversation test remains pending at the
person's request. The full multi-program goal stays active.

Follow-up: [actual FreeCAD Save As cancellation and Hazlo Stop recovery](native-recovery-ui-2026-10-16.md)
now pass through a real model and both native windows. That later evidence
is scoped to pending native dialogs; post-write/commit interruption and the
other integrations remain separate acceptance requirements.
