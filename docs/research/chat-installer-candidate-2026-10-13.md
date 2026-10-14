# Chat-first installer candidate — October 13, 2026

This candidate predates the [previous-conversation archive](chat-history-preservation-2026-10-13.md). Its prepared acceptance package is retained, but a fresh build is required to certify that later frontend change.

The privacy-remapped per-user NSIS installer builds from production source commit `ae13a51`. It contains the chat entry, Codex/Claude/Ollama routes, twenty-four shared tools, initial Orca/KiCad source flows and native drawing command plans. This is a **build candidate**, not a passed installed or live-editor acceptance gate.

| Artifact | SHA-256 |
|---|---|
| NSIS installer | `4af5549a01defa78e066692e6dd46d65d488b6a59906cb79c4d711f183ba51a0` |
| Unbundled main executable | `dcab928f7120150f82ed04313c57babc659873f150a4922cd499216a5964427d` |
| Expected installed main executable, NSIS marker | `742f1f02431509ffc562ce08a9eb659c5cb7599c005b71ab4a45900ff32c2df7` |
| Release tools helper | `bd33d6344495bc34a7c9e6d797753136a3b4c74f85f1735ef236c3a7a12fb7c8` |

All **6,545 tracked inputs** remain byte-identical before/after the build. Both shipped executables contain zero matches for the current builder-profile root in the checked Windows/forward-slash UTF-8/UTF-16 spellings. This targeted check does not claim universal binary secret scanning. Build logs and input manifests remain private; no binaries or captures are added to Git.

The rebuilt SDK harness exercises the **actual release helper** over its authenticated MCP transport: 24 tools, exact linked catalog and no engineering jobs. The tested helper matches the hash above. It is a built binary check, not an installed main-window test.

## Acceptance harness correction

The old installed-startup test still required a measurement form, nine-workflow selector and old welcome text. It now shares read-only initial-chat assertions with the production browser gate: six program groups, four AI choices, no engineering forms, no implicit job/proposal, disabled send without a project, empty chat composer, available project actions and unchanged initial state after reload. The guest result verifier checks those new fields, and package preparation copies the shared assertion module. These changes affect acceptance scripts only; shipping application sources remain unchanged after the candidate build.

The first headless initial-state attempt incorrectly waited for visibility of a project button inside collapsed Connections. Waiting for its attached/enabled state preserves a read-only test without opening that panel. The corrected complete production browser suite passes. Node/Python and PowerShell syntax checks pass, and every backend example builds. None of those checks establishes installed native UI acceptance.

The online isolated Windows test package is prepared with this exact installer/app/helper identity and the 24-tool expectation. Preparation passed once available memory recovered above the existing 3 GiB threshold. The updated native-startup checks are included. **No guest was launched and no native app/UI test ran during this preparation.** Native automation remains paused after Escape; explicit resume was requested for the owned test windows and disposable guest.

Still required: actual install/update/startup/uninstall preservation, installed engineering/connector checks, ordinary-user acceptance and native chat/source-picker/approval/cancellation/recovery checks for this candidate; real signed-in conversations and supported live-editor changes also remain unfinished. The full application goal stays active. Prepared guest configurations, raw logs, profiles, credentials and test project paths remain private.
