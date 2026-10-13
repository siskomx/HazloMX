# Current installed backends, upgrade and lifecycle

Installer `ab43ac2a00ec0d231038dca58a3b0011e6fd77378117dbc047899b2b6a598c8e` passed the resumed scoped connector/backend and lifecycle run on an owned Windows Sandbox, build 26100, after WebView2 prerequisite completion. The unsigned 0.1.0 package upgraded verified installer `33e45402…`; installed desktop/helper bytes matched the identified build, and both fifteen-tool checks passed. Startup and uninstall preserved project files and application user data. The guest is stopped; the environment list is empty.

The initial online attempt failed: the fixture stopped the previous installer at its 120-second budget while WebView2 setup continued. `guest/initial-install-timeout.json` preserves that result. Read-only process inspection later confirmed both installers had exited, WebView2 155.0.4283.45 was present, and Hazlo was absent. The retained recovery script checked the Microsoft signature, verified and installed the previous build, then resumed the original driver with its explicit `ResumeInstalled` parameter. `guest/bootstrap-resume-receipt.json` records those checks. The first online bootstrap attempt is not claimed as passing.

All 21 completed native jobs passed across eight included workflows:

| Workflow | Completed jobs | Scope |
|---|---:|---|
| Arduino Agent 0.7.1 | 2 | Authenticated copy edit/readback, denial, occupied destination, reconnect invalidation and recovery |
| Arduino CLI 1.5.1 / Uno AVR 1.8.8 | 2 | Compilation and saved-sketch revision; compiler failure, denial and recovery |
| KiCad 10.0.7 | 2 | Checks/exports and inch-to-mils follow-up; two actual ERC errors remain explicit |
| Konnect 0.13.0 / KiCad 10.0.7 | 4 | Two schematic edits and two PCB moves with saved-result follow-ups and inch/mils coordinates |
| CADCraft 0.5.0 | 4 | Metric/inch DXF/DWG edits and follow-ups |
| CADPilot 0.7.7 / FreeCAD 1.1.4 | 3 | Selected-document inspection, metric box edit and imperial cylinder modeling |
| OrcaSlicer 2.4.2 | 4 | Metric/inch slicing and settings follow-ups with selected profiles |

Originals, earlier results and restart recovery passed for every workflow. Konnect's missing-runtime refusal passed before the identified signed app-local Visual C++ prerequisite was supplied. Agent raw console logs remained private; exported logs redacted the actual token and passed the token-absence scan, and its owned service stopped. Package identities, scripts, calls, projects, reports and failures are retained here. The first retention check verified 740 files; `file-hashes.json` also covers the added documentation and reproducibility files.

The preparer omitted a pre-launch source manifest. `source-reconciliation.json` explicitly records the post-run reconciliation: seven cataloged inputs unchanged from the previous gate, four changed inputs matching retained frontend/backend gate hashes, and the additional verified host-tools source. The preparation script used for this run is archived byte-for-byte against its recorded hash. This is not a pre-launch source capture. The shared preparer now writes and checks its manifest before returning; an isolated self-check verified 134 source entries. A subsequent full preparation correctly refused insufficient memory without creating or starting another guest.

Approvals ran through the shared-backend test fixtures and actual installed stdio helper. Native UI input remained stopped; no initial-UI driver, project/runtime/source picker interaction, engineering GUI approval, artifact-opening interaction or signed-in chat ran. New-solid FreeCAD execution retains separate prior evidence. Ordinary-user acceptance, broader fidelity, live editing and remaining cancellation/deadline gates are unfinished. No model inference or physical-device/printer control ran. The full application goal remains active.
