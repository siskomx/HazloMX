# Current chat installer: Windows acceptance — October 14, 2026

Installer **6a2f7ef9…** passes a fresh Windows build-26100 guest, a separate ordinary nonadministrator account, and an upgrade from the preceding 24-tool chat candidate. The owned guest is stopped after verification. Raw projects, profiles, logs and captures remain private under ignored `target/`; the [sanitized receipt](chat-installer-windows-2026-10-14.json) contains the public scope and identities.

## Package and lifecycle

| Artifact | SHA-256 |
|---|---|
| Current installer, unsigned 0.1.0 | `6a2f7ef9cbe6778f916d28cda2d2ac22cde2fe2c09acffbf5f5bc3ad8a61fdd5` |
| Installed NSIS app | `201c429eb19e92b86445cd9ff23acb6311877b7e1ce20a4ad761c7e702d42816` |
| Bundled connector | `a9546dc3da0fbe5100e596a6cd821a54a1593323f4edea4a36beca757652dad8` |
| Previous chat installer | `c0854251f329bf876a76373c7c45bc527ebaead2bcf1aeec9a0c75b18404b027` |

The fresh main pass verifies current-user registration, exact installed binaries, same-version update, installed/updated 26-tool transport, startup, uninstall and preservation of project/app-data markers and all captured engineering files. The connector cannot approve jobs and refuses unknown selections. Its catalog is hosted by the current linked acceptance harness; engineering approvals use explicit backend fixtures.

The later old-to-new pass verifies the previous installed app/helper identities and startup, upgrades to the exact current app/helper, tests the current connector against all 26 tools, checks upgraded startup, then uninstalls. The owned project files and app-data marker remain byte-identical. This checks package replacement and preservation; it does not certify every historical conversation schema or a real signed-in migration.

## Actual native workflows through the installed connector

| Integration | Completed jobs | Tested scope |
|---|---:|---|
| Arduino Agent | 2 | Authenticated native copy edits, readback, denial, occupied-output failure, reconnect invalidating approval and recovery |
| Arduino CLI | 2 | Uno compilation, saved sketch/HEX/ELF, failure, denial and recovery; no upload |
| KiCad checks | 2 | Native schematic/PCB checks and exports, actual findings and recovery |
| Konnect / KiCad | 4 | Two schematic edits and two PCB moves, inch/mil placement, saved-result follow-ups and recovery |
| CADCraft | 4 | Metric/inch drawing copy edits, native saved-result readback and recovery |
| FreeCAD / CADPilot | 3 | Saved-document inspection/primitive edits, metric/imperial inputs and result-derived recovery |
| OrcaSlicer | 4 | Metric/inch file slicing, checked 3MF/G-code, preserved profiles/geometry, revised settings and recovery; no printer control |
| **Main total** | **21** | **Software-only native execution with backend fixture approvals** |

Agent startup succeeds in the fresh guest, unlike the separate isolated host attempts. The guest's generated token is absent from retained exports; raw upstream console streams remain guest-private, and the owned Agent process/service stop cleanly. This establishes installed Agent execution, not the still-unexecuted real Agent conversation or a diagnosis of host startup.

An independent post-uninstall hash pass matches **736 main exported files** against their guest originals and checks the complete exported file sets. The 64 prepared input files remain unchanged. The separate upgrade's three read-only inputs also remain unchanged. Shipping source still matches this candidate: subsequent repository changes only affect test code and documentation.

## Installed initial chat and ordinary user

The actual installed WebView passes the initial-chat assertions: six program groups, four AI choices, no engineering input forms, no implicit proposal, Send disabled without a project, and initial-chat preservation on reload. The test uses a private profile and temporary app-specific debugging policy; its cleanup checks pass. This is initial installed UI acceptance, not UI approval of the 21 engineering jobs or model inference.

A fresh account outside the Administrators group passes per-user install/update, exact binaries, installed connector transport, two actual KiCad jobs/recovery, startup, uninstall and project/app-data preservation. An independent post-uninstall pass matches **67 ordinary-user exported files** and their complete file sets. This ordinary-user engineering scope is KiCad; broader ordinary-user workflows remain unverified.

The first orchestration wrapper invoked the ordinary-user driver with `-File`, which the guest's Restricted PowerShell policy rejected before account/test creation. That failure is preserved. The correction invokes the unchanged driver explicitly through `Get-Content`/`Invoke-Expression`, as the existing main launcher does. It changes no system execution policy and repeats only the ordinary-user wrapper, not the successful 21 main jobs. The original overall wrapper receipt therefore remains failed; the separate final verification combines the passing main, corrected ordinary-user and upgrade receipts without overwriting that failure.

The main runner is [sandbox-current-package.ps1](../../apps/desktop/tests/sandbox-current-package.ps1); ordinary-user checks use [the driver](../../apps/desktop/tests/sandbox-nonadmin-driver.ps1) and [user test](../../apps/desktop/tests/sandbox-nonadmin-user.ps1), with exact current binaries and fresh owned inputs. Guest runtime copying is a substantial preparation phase, separate from native execution deadlines. Launch was deferred until the observed host memory exceeded the unchanged 3 GiB requirement.

## Remaining goal requirements

This closes current-package guest/backend lifecycle, installed initial chat, the scoped ordinary-user pass and previous-chat-build upgrade. The [real host release Arduino chat](chat-release-native-2026-10-14.md) separately covers native approvals, denial, automatic AI readback and recovery. Real Arduino Agent conversation, other program/provider native UI approvals, running-operation UI cancellation, broader ordinary-user coverage and live/unsaved editing remain unfinished. Claude Code still reports signed out. The full application goal remains active.
