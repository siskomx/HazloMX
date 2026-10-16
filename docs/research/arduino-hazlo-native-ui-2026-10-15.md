# Arduino and Hazlo native UI gate — October 15, 2026

The owned Arduino fixture now supports `--with-hazlo` together with `--launch --hold-for-conversation`. This prepares the remaining native Hazlo approval/review test against the actual open sketch. The two-app launch and shared cleanup pass; the engineering approval/review UI gate has **not passed**.

The existing Rust supervisor waits for the native editor probe and direct default-profile host checks to pass before starting the bundled debug Hazlo app. Hazlo uses that fixture's editor-registration profile, a fresh project and separate app/WebView data. No connection record is copied and no user IDE profile is changed. Both applications run through the existing owned-process implementation and share cancellation, desktop-lock and physical/commit memory monitoring. When the editor finishes, the supervisor cancels and joins the Hazlo task before returning. Private logs retain the main executable hash and stop outcome, with `native_ui_pass` explicitly false until separately observed.

This replaces the ad-hoc Hazlo process ownership approach whose missing cleanup receipt was identified during the [software-choice native checkpoint](choose-program-2026-10-15.md). No production chat, companion or job behavior changes in this acceptance-only addition.

The modified examples build and the supervisor passes Clippy with warnings denied. Python syntax/help checks pass, and all three incomplete flag combinations refuse before preparing or launching a fixture. The later positive two-app launch and shared stop checks are recorded below; native UI approval/review still require actual acceptance. Existing process-tree tests are supporting evidence, not a substitute for that positive run.

At the resource check Windows has about 1.6 GB free, with Java using about 3.4 GB. The two-app run was initially held at that headroom. The earlier request to close the unused project restarting Gradle remains pending; no additional daemon termination or unrelated application closure occurred in this checkpoint.

To reproduce, build the production frontend and bundled `hazlomexa` debug binary with `custom-protocol`, plus `owned_arduino_editor_probe` and `owned_agent_editor_host`. Run `tests/arduino/prepare_native_editor.py --launch --hold-for-conversation --with-hazlo` with Windows unlocked and enough memory for both programs. Wait for the actual windows and passing default-profile prerequisite. In Hazlo select the existing provider and Arduino Agent, inspect the open sketch, request a natural-language pause change, approve through the UI, verify the native buffer and automatic result review, then test a follow-up and separate native save. Stop through the existing owned fixture stop flag and verify both process outcomes. Do not automate security/privacy prompts or retry uncertain writes.

The separately passed [real Arduino conversation](arduino-live-conversation-2026-10-15.md) uses backend fixture approvals. It does not close this remaining UI gate, ordinary-user installation, current installer acceptance or the full multi-program objective.


## Actual launch and cleanup checkpoint

A later check reported about 3.5 GB free, and the fresh combined fixture was launched. The real native editor probe and direct default-profile host checks both pass. The bundled Hazlo window then opens with its fresh project, demonstrating that the waiting prerequisite and editor-registration profile handoff execute. The main executable SHA-256 is e103957b7b59745f67434cd943ed180a352b1bb41a4d9480eb2ac31dc3955848.

A Windows Security permission request for mdns-discovery covers the workspace. It is left untouched and no native UI input follows it. The Computer Use guidance forbids acting on security/privacy permission requests; the existing human handoff remains pending. No Hazlo conversation, approval, edit, follow-up or save is counted as tested. Its project contains no jobs.

Before cleanup an independent process-tree query confirms both Hazlo and Arduino Agent live among 22 owned descendants. The existing explicit stop flag cancels both owned process trees. Both stop receipts report cancellation, the editor supervisor finishes successfully, and a separate per-process identity/creation-time check confirms all 22 observed descendants have stopped. This closes the positive fixture launch and shared-cancellation checkpoint, not the missing native engineering UI gate. Raw process records, profiles, logs and observations stay private under ignored build output.
