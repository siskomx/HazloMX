# Arduino Agent default-profile acceptance — October 15, 2026

The owned Windows editor now passes native session discovery without overriding the companion's registration directory or copying a connection record into Hazlo's discovery location. This strengthens the earlier [companion setup gate](arduino-companion-setup-2026-10-15.md); it does not change production application behavior.

The test supervisor installs the embedded production companion into the fresh owned `.arduinoIDE/plugins` directory and launches the pinned Arduino Agent 0.7.1 runtime with its isolated user profile. The companion creates its own `.arduinoIDE/hazlomexa-editor` registration. The native probe authenticates there directly. Once that probe passes, the launcher immediately runs the production Rust host acceptance example against the same profile, while the owned editor is still live. The host no longer copies or deletes a real editor's registration.

## Passed evidence

- Actual native probe: unsaved source inspection, authenticated edits, native Undo, follow-up, saved-byte preservation and retained earlier unsaved note.
- Actual shared host tools: direct default-directory discovery, fresh current-state inspection, two separately approved native jobs, verified receipt/readback, stale-request refusal and two completed schema-15 jobs after recovery. Approvals are backend fixtures.
- Observed editor: final `delay(100)` and the retained unsaved note are visible, with the document marked dirty. An unrelated native firewall permission prompt was present and received no automation input. This is result observation, not Hazlo approval UI acceptance.
- Explicit owned stop after successful checks; the launcher and supervisor finish successfully. The usual memory/lock monitors remain enabled throughout.
- Both changed Rust examples compile and pass Clippy with warnings denied. Fourteen companion/transport checks, including the actual compiled host against a controlled IDE API, and two default-profile/redirected-profile checks pass. Launcher syntax and whitespace checks pass. Production source and frontend are unchanged, so their previously passing suites were not rerun for this acceptance-only change.

Raw registrations, private profiles, captures, source observations and logs remain under ignored build output. The launcher now fails if the native probe never reaches the additional host check, if that host check fails or times out, or if its supervisor fails; a successful supervisor exit alone cannot establish success.

This closes actual default-profile registration/discovery in a disposable profile on the host. It does not prove the installed app under an ordinary user, a real-provider conversation, Hazlo's native setup/approval interaction, live save/compile or hardware use. The current installer and the full multi-program goal remain unfinished.
