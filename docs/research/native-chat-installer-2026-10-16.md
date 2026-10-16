# Current chat installer: Windows backend and ordinary-user acceptance

The NSIS package built from `542591f` passes a fresh owned Windows guest's
backend/lifecycle gate and a separate nonadministrator account in that guest.
It includes the native-PCB reconnection context fix. Installed interactive UI
acceptance remains pending; these results do not complete the application goal.

## Package and identities

The normal privacy-remapped package command and all eight installed-connector
harness builds pass. The current release helper passes the real 33-tool
protocol check. All 179 recorded source inputs remain unchanged after testing.
Raw profile-path scans pass for the desktop executable, helper and frozen
KiCad worker, in UTF-8/UTF-16 with both path separators.

The [package identity](native-chat-installer-2026-10-16/package-identity.json)
binds the installer, expected/verified installed app, unbundled app and helper.
The guest started with 5,393 MiB free host RAM, above the unchanged prerequisite.
Runtime copies were allowed to finish; no memory or operation thresholds were
lowered to obtain a pass.

## Main guest

The [backend receipt](native-chat-installer-2026-10-16/windows-backend.json)
records exact installed binary checks, current-user registration, upgrade from
the previous package, same-version update, startup remaining alive, uninstall,
and preservation of external projects/app-data markers. Both installed and
updated helper transports expose the expected 33-tool catalog.

All seven engineering harnesses pass recovery and complete 21 native jobs:
Arduino Agent 2, Arduino CLI 2, KiCad 2, Konnect schematic/PCB 4, CADCraft 4,
CADPilot/FreeCAD 3 and OrcaSlicer 4. The Konnect missing-Visual-C++-runtime case
also refuses without publishing a result or changing its sources. Controlled
denial/failure cases remain included in the corresponding harness receipts.

Approvals use backend fixtures. These are explicitly saved-copy, compile,
export and slice workflows; they do not prove live editing in an open GUI.
No real AI inference, hardware upload or printer control is involved.

## Ordinary nonadministrator

The [ordinary-user receipt](native-chat-installer-2026-10-16/ordinary-user.json)
verifies absence of administrator-group membership, per-user install/update,
exact app/helper identity, installed helper protocol, two native KiCad jobs
with recovery, hidden startup, uninstall and preserved project/app-data files.
It does not establish ordinary-user interactive UI or all-program acceptance.

The guest-only [launcher](../../apps/desktop/tests/sandbox-nonadmin-driver.ps1)
is byte-identical to the passing fixture and refuses an existing test account
or directory. It generates a fresh password in guest memory and runs the
existing [user test](../../apps/desktop/tests/sandbox-nonadmin-user.ps1) with
that account. It requires a passing main lifecycle and the prepared read-only
input share; it is not a host-account setup command.

An independent post-uninstall manifest matches all
[803 exported project files](native-chat-installer-2026-10-16/export-preservation.json):
736 from the main run and 67 from the ordinary-user run. Raw projects, account
setup, paths, credentials and diagnostic logs remain private under ignored
`target/`; only sanitized receipts are published.

## Remaining acceptance and handoff

A new `mdns-discovery` Windows Security prompt was observed inside the guest
after the Arduino Agent test. No action was taken on it. The host desktop then
reported locked, and native UI work stopped. The guest is retained for the
human unlock/prompt handoff and the installed-app UI check.

This supersedes the prior package's prepared-only Windows backend status.
The [one-move native KiCad UI checkpoint](kicad-native-ui-checkpoint-2026-10-16.md)
remains separate evidence. Its visible imperial follow-up/export/cancellation,
current installed interactive UI, broader ordinary-user/provider/native flows
and unsupported integration capabilities remain unfinished.
