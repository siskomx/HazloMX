# Embedded Codex desktop wiring

October 12, 2026. `desktop_codex.rs`, the desktop snapshot, TypeScript contracts and the conversation controls now connect the [verified backend](../codex-resume-login-2026-10-12/README.md) to the app.

- Provider selection offers guided/Ollama, installed Claude Code and installed Codex. Codex has a version/hash-validated runtime picker and separate account connection controls.
- Account connection streams status and an official link. The link opens only through a user-clicked command tied to the current, active login ID. Cancel clears the link and waits for the owned connection to finish. Login and chat refuse concurrent use of the same connection configuration.
- Codex turns use the existing desktop-owned `HostTools` and a fresh project-bound bridge. Native proposals retain the existing approval/publication/readback boundary. The temporary bridge configuration is removed after a turn, while Codex-managed account/session files remain in the application's own connection home.
- Successful turns retain their native thread ID for same-project follow-up during the app run. New conversations, runtime selection and project changes clear the corresponding in-memory session mapping. Failure/cancellation discard the resumable mapping.
- Claude and Codex share chat display, cancellation, source-bounded input and the existing sequenced event handling. Codex's displayed conversation is scoped by project/provider, preserving messages when the engineering workflow changes. Earlier account events and older snapshots cannot replace the later sequenced status; login cancellation also retains the latest event.
- The initial five commands are in the Tauri handler, command manifest and bundled-main-view permissions. The subsequent [question forwarding](../codex-questions-2026-10-12/README.md) adds a sixth command for ordinary human answers. There is no new agent approval, arbitrary path, shell or hardware tool.

## Verification

TypeScript/Vite production build, desktop/custom-protocol Clippy, formatting, desktop/helper binary build and 100 regular Rust tests pass (30 separately scoped ignored entries). No model inference or signed-in chat was used.

`node apps/desktop/tests/design-flow.mjs` passes all eight existing workflow fixtures and the added Codex controls. Tests cover runtime/account prerequisites, no automatic sign-in-link opening, explicit opening command, cancellation, an early login event arriving before its older command response, an old connection callback after reconnection, chat display/escaping, workflow-switch context preservation, first/follow-up session flags and chat cancellation. These use mocked IPC and do not certify natural language behavior or live authentication.

The first added workflow-switch assertion exposed a real frontend bug: the storage scope still treated Codex like the guided workflow, clearing its conversation when switching to CADPilot. The scope now follows the provider, and the regression passes. Existing Claude checks pass unchanged.

## Native check stopped

The rebuilt `target/debug/hazlomexa.exe` launched and its native accessibility tree was retrieved. Windows Graphics Capture returned a black frame. Activating that returned window failed with `failed to activate captured window`; after listing windows and rehydrating the same live target, the permitted recovery attempt returned the same error. No provider selection, runtime picker or authentication input was performed. The owned Hazlo PID 3228 was verified by its executable path, stopped outside UI automation and independently confirmed absent. Unrelated Codex/Vivaldi windows were untouched.

The [Computer Use skill](C:/Users/USER/.codex/plugins/cache/openai-bundled/computer-use/26.1007.21434/skills/computer-use/SKILL.md) requires its [guidance](C:/Users/USER/.codex/plugins/cache/openai-bundled/computer-use/26.1007.21434/docs/guidance.md), which says to “retry once; report the exact error if recovery fails.” The native check remains incomplete rather than repeating inputs against the failed window binding. The same guidance says “Do not automate user authentication dialogs”; successful browser sign-in is a user action.

Native controls/pickers, successful sign-in, signed-in model-specific tools and proposal/approval/edit/readback conversations, signed-in question behavior, restart recovery, updated installer and second-Windows acceptance remain required. Question forwarding passes the subsequent installed-client loopback and browser fixtures linked above. This progress does not close the complete application goal or its live editing/remaining integration requirements.
