# CADCraft live control contract — October 15, 2026

The real pinned CADCraft 0.5.0 Windows desktop application passes a live native-protocol probe in a fresh owned profile. This establishes actual open-window editing capability and identifies a document-binding gap that the saved-copy adapter does not solve. No production native writer or new agent tool is enabled by this checkpoint.

## Actual desktop probe

`owned_cadcraft_live_probe` verifies the GUI executable SHA256 `339373dde0fa465f4bb3da23a7da48096329214587b4e363558f9931369d0982`, copies the public metric circle fixture into a fresh ignored directory, isolates preferences/logs, and launches the editor in the existing owned Windows process tree. It connects only to that test's loopback control port. Windows must remain unlocked. Requests and replies are bounded and matched by ID; an uncertain sent request is not retried. The process tree is stopped and joined even when a probe assertion fails.

The real editor passes:

- Current drawing inspection, native command discovery and a millimetre unit query.
- A 1 mm move and a 0.1 inch move of the inspected circle through `engine.execute`, followed by fresh native entity readback. The final centre is X = 3.54 mm; radius, Y/Z and other inspected entity properties remain unchanged.
- Two native Undo commands, each verified against the preceding entity projection, counts and layers.
- Closing and reopening the owned saved drawing, with restored native geometry and byte-identical saved source.

The dirty owned drawing and part of its circle were observed in the native window. A Windows firewall prompt from the preceding Arduino discovery service obscured the centre of the screen; it received no automation input. Unobstructed visible geometry and native UI approval are not proved. Raw snapshots, protocol traffic, profiles and logs remain private under ignored `target/`.

The first harness attempt refused before GUI launch because TEMP and TMP referred to the same newly created directory. Idempotent creation of the owned directories corrects that harness error. The later inspection and full move/Undo/reopen probes pass in separate fresh directories. The example compiles and passes Clippy with warnings denied. Production code is unchanged, so the regular backend/frontend suites were not rerun for this acceptance-only addition.

## Binding gap and next implementation requirement

The actual `ui.inspect` public document summary is identical before close and after reopening the same source: index, title, path, dirty flag, entity count, active flag and revision all repeat. Its internal document UID is not included. Native geometry can also be identical after a reopen, so hashing the public response cannot distinguish those document lifetimes.

The pinned [native command handler](https://github.com/storytold/cadcraft/blob/315751e3ec98e708bcc831dd7f80ca0433a2e3d0/crates/ui-egui/src/control.rs) executes against the current session; its request has no documented expected-document/state precondition. The [engine](https://github.com/storytold/cadcraft/blob/315751e3ec98e708bcc831dd7f80ca0433a2e3d0/crates/engine/src/lib.rs) retains an internal UID but omits it from the public summary. The [transport](https://github.com/storytold/cadcraft/blob/315751e3ec98e708bcc831dd7f80ca0433a2e3d0/apps/cadcraft/src/control_server.rs) binds loopback and queues requests without authentication. These are specific properties of the pinned interface, not a claim about every CADCraft version.

A live Hazlo approval needs a native document-lifetime identity and a check of that identity and expected state in the same native dispatch that applies the approved operation. A separate read followed by a write still permits a tab switch or reopened document between them. Process identity and a whole-drawing digest cannot alone close that gap. The connection also needs verified editor/process ownership; a loopback address by itself is insufficient.

Do not advertise this probe as a completed live chat flow or substitute the existing saved-copy operation for it. The current saved-copy/native-command tools remain explicitly scoped as before. Native binding/approved dispatch, result receipts, provider/Hazlo UI acceptance, broader geometry and current installer acceptance remain unfinished. The full multi-program goal stays active.

Reproduce after provisioning the pinned runtime: build the `owned_cadcraft_live_probe` example, then run its executable. It pauses up to thirty seconds for optional result observation before continuing with native Undo; the pause does not establish UI acceptance. No hardware or user editor is accessed.
