# CADCraft chat package and Windows checks

October 16, 2026. Candidate source is `18134bf`, containing the
[native CADCraft chat integration](cadcraft-live-chat-2026-10-16.md).
The updated installer understands its schema-16 job records. Older installers
still do not; earlier evidence keeps its original package identity and scope.

[Candidate identity](cadcraft-chat-package-2026-10-16/candidate.json): installer
SHA-256 `b36a164bb3e74d4222fd2660218a4fe05e419f1521f10825dc4e4b56e4e82e86`,
17,221,759 bytes. All 6,714 tracked build inputs remain unchanged during
the build. Builder-profile scans find zero matches in the shipped app/helper.
The helper passes the current 35-tool transport check, including the two new
native drawing tools. No engineering work or inference runs in that check.
The selected maintained CADCraft runtime is separate and is not bundled or
installed automatically.

## Release-image recovery

The [actual release-image check](cadcraft-chat-package-2026-10-16/release-recovery.json)
uses bytes matching the installer's expected installed executable and helper.
It opens the existing owned project through the native folder picker and
recovers both schema-16 jobs. All eight earlier chat messages restore in
exact order and text; no approval or native connection is revived.

After explicitly reconnecting the tested native runtime, one real cloud
request compares the latest historical report with fresh native state. The
old report records 3.54 mm; the freshly opened drawing is centered at the
origin with no pending changes. Hazlo distinguishes both rather than
replaying the old edits or treating a saved report as current geometry.
The report opens read-only through the release UI. The history now has ten
messages, the five existing project files remain byte-for-byte unchanged,
and no new job is created. Original drawing bytes and host CADCraft settings
remain unchanged. Both owned native processes are stopped.

This is a host release-image recovery/read-only test, not a host installation
registration test or a repetition of fresh native writes. The actual owned
chat approvals, metric/inch edits and native result window retain their
[separate source/debug acceptance](cadcraft-live-chat-2026-10-16.md).

## Fresh Windows guest

The [fresh disposable guest](cadcraft-chat-package-2026-10-16/lifecycle.json)
passes current-user installation of the prior retained package, upgrade to
this candidate, exact installed executable/helper identities, current helper
transport checks, app startup, uninstall and preservation. Three exported
files independently match their guest SHA-256 values. The driver reaches its
terminal state, the app is absent after uninstall, and the owned guest is
stopped and absent from inventory.

Both installed helpers transport the current fixture host's 35-tool catalog;
that does not claim the older app gained those tools. This guest run performs
no engineering workflows, native UI automation or signed-in conversation.
Earlier engineering/ordinary-user gates are not relabelled as tests of this
candidate. Full installed acceptance remains incomplete.

More CADCraft commands and native save/export, live schematic/slicer
interfaces and the outstanding provider/current-package gates remain in
[current native-flow status](../native-flow-status.md). The full application
goal stays active. Raw logs, model text, paths, profiles and captures remain
in ignored owned test directories; only sanitized identities and assertions
are retained here.
