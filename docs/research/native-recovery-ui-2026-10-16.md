# Real chat recovery after native save cancellation

The actual Hazlo/FreeCAD windows now verify the recovery added in
[`601c61f`](native-outcome-readback-2026-10-16.md), through the existing
configured Ollama cloud route. This adds real model/native UI evidence to
the earlier browser dispatch tests. The [sanitized receipt](native-recovery-ui-2026-10-16/receipt.json)
identifies the tested app and native editor; raw conversations, profiles,
native task outputs and paths remain private under ignored `target/`.

## Two actual cancellation paths

A fresh owned FreeCAD 1.1.4 profile creates one unsaved 25.4 x 20 x 10 mm box
labelled Carcasa. An independent native read establishes its document token,
complete object projection and geometry fingerprint.

1. In Hazlo chat, request Save As for the open enclosure without changing
   its shape. The agent inspects the document and prepares a separate,
   pathless save proposal. Approve with the actual Hazlo button, then cancel
   FreeCAD's own Save dialog. Hazlo records a failed, incomplete save and
   automatically asks the agent to inspect current native state. Its visible
   answer correctly describes the unchanged unsaved box and limits what can
   be inferred about the preceding interaction. It does not retry saving.
2. Explicitly request another Save As in the same conversation and approve
   the fresh proposal. Once FreeCAD's native dialog is visibly open, click
   Hazlo's running-operation Stop button. Hazlo records cancellation and
   starts automatic recovery. The FreeCAD dialog remains open, demonstrating
   that cancelling Hazlo's wait does not dismiss the native operation. Cancel
   that still-open dialog in FreeCAD. The agent then describes the current
   unsaved document, the incomplete attempt and its verification limits,
   without preparing another save.

Five independent native reads retain the same document token, state digest
and full object projection. The document remains modified and has no saved
filename; no FCStd is created anywhere in the owned test root. Native task
records contain the two `native_save_not_completed` refusals. They also show
two current-document inspections after the second attempt, only one of which
is the independent host test read, establishing an additional native read
by Hazlo during recovery. This is not an exact model-tool-call trace.

There are exactly two schema-13 jobs: one failed and one cancelled, both
approved through the UI. Neither publishes a completed artifact. Restarting
Hazlo on the same project restores both terminal history entries without a
new approval. Independent hashes match all three project files, excluding
the transient writer lock. Both supervised native applications and the
separate recovery window are stopped.

## Harness correction and limits

The first harness launch assumed the top-level FreeCAD launcher remained
the editor process. It handed off and exited. The owned child was identified
by its exact test macro argument and matching native binary hash, then
stopped. The successful retry launches the actual editor executable directly
and supervises both apps with the existing unlocked-desktop check, 1 GiB
physical/commit memory floor and a bounded deadline. No permission request
or security setting is automated.

This verifies cancelled native Save As and cancellation of Hazlo's wait
while its native dialog is pending. It does not prove cancellation after a
file write or geometry commit, lost-acknowledgement recovery for every
integration, installed conversation acceptance, or another provider's model
behavior. The current installer predates these source changes. The full
multi-program goal remains unfinished.
