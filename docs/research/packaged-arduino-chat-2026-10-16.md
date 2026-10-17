# Packaged Arduino chat and recovered result context

The owned Hazlo image is byte-identical to the guest-installed application
from `6b669f8`. With the existing Ollama cloud route and Arduino Agent 0.7.1,
its actual chat completes two separately approved edits and a separate save.
This checks the shipped image in an isolated host profile; it does not repeat
installation or establish the behavior of every AI provider.

Natural requests change an open sketch's pause from 100 ms to a quarter
second, then to half a second, preserving an unsaved user note. Independent
native reads confirm the old buffer before each UI approval and the new
buffer afterward. A separate approved save produces a clean buffer matching
disk. Seven native observations retain the same document; normalizing only
the delay leaves identical source. All three completed schema-15 receipts
match their recorded SHA-256 values and subsequent native observations.
Fixture setup and its backend approvals are separate from these chat jobs.

Restart restores all three completed jobs and verified-report access.
Both subsequent normal window closes preserve all seven project files.
The read-only report button also opens the verified native-save receipt in
the associated editor; it confirms a clean document and native save.

Two recovery limits are retained rather than reported as passes:

- The initial forced process shutdown loses the final automatic save
  confirmation from chat, despite preserving the completed job and receipt.
  A later user/assistant exchange survives a normal window close and restart.
  Forced-shutdown chat durability remains unresolved.
- Asked to describe the latest saved project result without opening the IDE,
  the old package's agent describes the preceding unsaved edit instead of
  the later successful save. Its workflow-status tool exposes neither
  timestamps nor a defined history order. The exact model tool trace was
  not retained, so that missing context is a contributing gap, not a proven
  complete explanation of the model's choice.

The shared tool now returns creation/update timestamps and sorts history
by latest update, with unknown legacy timestamps last. Its description
requires filtering by program and completed phase, reading the actual
verified report, and treating equal timestamps as ambiguous. This changes
only the tool response; it does not rewrite project records or add UI.
The recovered-project regression fails before the change and passes after
it, including conflicting UUID/creation/update order, ties, legacy records,
repeatable output and absence of private audit paths. Shared host-tool tests
pass (21 passed, 12 intentionally ignored), as does Clippy with warnings
denied. The corrected tool has not yet been tested in a new packaged real
conversation. The current installer still predates this source fix.

The [sanitized checkpoint](packaged-arduino-chat-2026-10-16.json) records the
tested image identities. Raw conversations, native registrations, profiles,
sketches and captures stay in ignored local test storage. All owned Hazlo
and Arduino test processes are stopped. No hardware or compilation was used.
