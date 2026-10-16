# KiCad native chat UI: one approved move and report recovery

The actual owned Hazlo window, existing Ollama Cloud route and pinned KiCad
10.0.7 editor completed one native PCB move through the production chat and
approval UI. This advances the earlier backend-fixture conversation evidence;
the complete visible follow-up/export gate remains unfinished.

## Observed and checked

- Selected the existing cloud route and KiCad in Hazlo, entered the owned
  editor's IPC endpoint, and selected its exact disposable PCB in the native
  file picker. No security preferences or API permissions were changed.
- Asked in ordinary Spanish for the topmost 10k resistor, without supplying
  its reference or coordinates. The agent identified R1 at (197, 141) mm and
  asked for the missing movement distance. It prepared no change at that step.
- Requested 2 mm to the right. Hazlo displayed the actual target, old/new
  positions, unsaved-document scope, track limitation and human approval.
  A separate native worker read before approval matched the entire initial
  board-state hash, with R1 still at (197, 141) mm.
- Approved using the owned Hazlo button. The schema-14 job completed, with a
  native commit receipt locating R1 at (199, 141) mm. Every other component's
  complete recorded projection matched the initial observation. The receipt
  reports `native_commit_confirmed: true` and `saved: false`.
- Viewed KiCad's own PCB window unobstructed: the document was dirty and the
  edited footprint was visible. Hazlo's automatic agent explanation described
  the completed movement and unsaved/track limitations. A separate HTTP/tool
  trace was not recorded for these UI turns, so that explanation alone is not
  independent evidence of particular model tool calls.
- Reopened the same Hazlo project after the owned apps stopped. Its completed
  native history entry and verified read-only receipt opened through the real
  UI. The application's recovery/artifact validation ran; all project file
  hashes remained identical before and after this recovery check. This proves
  report/history recovery, not restoration of KiCad's unsaved buffer.

## Limits and stop

The existing memory monitor stopped both owned applications at its 1 GiB floor
after the first completed move. A subsequent independent native inspection
could no longer reach the editor and is not counted as a successful post-job
read. The job's own native readback and published receipt remain verified.
The saved PCB bytes stayed identical to the original disposable source.

Earlier attempts also stopped at the same floor before creating PCB jobs.
The Windows Security prompt had been handled by the user before these tests.
Docker Desktop was shut down normally after confirming no containers were
running. Memory thresholds were not lowered.

The later imperial follow-up, second UI approval, export UI, cancellation UI,
other real AI providers and current installer acceptance are still pending.
The earlier seven-turn backend conversation remains separate evidence and
does not close these UI gates. No hardware or user engineering document was
accessed. The full multi-program goal remains unfinished.

The tested source includes `1194b13`; the debug application SHA256 is
`be213d0515418fe21d0803fcc6647233519dfea67f220ccd72c5982c173b32b2`.
Raw connection metadata, profiles, conversations, screenshots and receipts
remain private under ignored `target/`; none are published with this note.
