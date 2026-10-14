# CADPilot preview package: installed Windows verification

Installer `07ac4126473b67e3d83fe9a1afc3ac08d306097b8a2ab6ff15582f7d987c30b5` passes a fresh Windows Sandbox guest's package and engineering gates. The [native host preview/edit evidence](../native-cadpilot-preview-2026-10-13/README.md) separately covers the actual chat, measurement selection, corrected current-to-proposed preview and explicit UI approval.

## Installed checks

- Per-user installation of the preceding `1c4ea36a…` build, exact binary identities, same-version update to `07ac4126…`, preserved project, seventeen-tool transport through the current linked test harness, hidden app startup, uninstall and external project/app-data preservation.
- Twenty-one native engineering jobs: Arduino Agent 2, Arduino CLI 2, KiCad 2, Konnect schematic/PCB 4, CADCraft drawing 4, CADPilot 3 and OrcaSlicer 4. Fixtures use the installed connector, shared backend approval driver, actual pinned runtimes and saved-result/recovery checks. They do not exercise native approval buttons.
- A separate ordinary account, outside the Administrators group, passes per-user install/update, exact installed binaries, installed transport, two native KiCad jobs/recovery, hidden startup and project/app-data-preserving uninstall.
- Captured source/test inputs and every prelaunch guest input remain unchanged. Exported engineering files independently match hashes computed inside the guest; exact counts are in `scope.json`. The guest is stopped after verification.

The NSIS bundler changes the app's single Tauri bundle marker from `UNK` to `NSS`. Package identity records the resulting installed hash and the raw build hash; the installed helper matches the build exactly. The [host evidence](../native-cadpilot-preview-2026-10-13/README.md) documents the byte comparison.

## Fixtures and initial session attempt

Use `apps/desktop/tests/prepare-current-package.py --online` with the captured runtimes, current release/helper and linked harnesses. Supply the previous tested installer plus its matching package identity for an upgrade. Omit the two WebView debugger startup fixtures when running the backend gate. Run `sandbox-current-package.ps1` only inside its fresh owned WDAG guest. Prepare the read-only ordinary-user inputs, including the verified Microsoft runtime DLL, and use the existing `sandbox-nonadmin-driver.ps1` / `sandbox-nonadmin-user.ps1` after the main gate passes. Their accounts, ACLs and passwords are confined to the disposable guest; no host accounts change.

The first headless start had no existing logon session. An explicit probe launch returned Windows error `0x80070520` before any probe ran. Connecting the owned Sandbox established its desktop session and the configured logon command started the fixture automatically; the failed explicit launch was not retried. No app failure or engineering result is attributed to that setup attempt.

## Limits

This guest gate does not automate desktop controls, authenticate Claude/Codex conversations, run model inference, edit live unsaved documents, generate a separate new FreeCAD solid, upload firmware or control a printer. The seventeen-tool catalog is hosted by the current linked harness through the installed helper. Native host UI proof has its own narrower scope. Remaining workflow UI choices, ordinary GUI-saved CAD fidelity and live editing remain unfinished in the application goal.

Only whitelisted receipts, hashes, versions and counts are public. Profiles, raw logs, authentication material, captures, runtimes and engineering projects stay in ignored owned fixtures.
