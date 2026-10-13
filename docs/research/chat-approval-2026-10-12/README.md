# Approval while agent chat streams

The browser regression reproduced a disabled approval button while Claude/Codex chat was busy. The approval control now excludes chat streaming from its busy condition; command dispatch, engineering execution, inference, login, dirty inputs, runtime prerequisites and the one-use approval token still gate it. Project, workflow, provider and parameter controls remain locked during chat. Approval still requires a human click.

Cancellation cleanup previously read the proposal phase and cancelled it under separate workflow locks. A human approval between those steps could be cancelled by chat cleanup. The shared workflow now checks the matching job and AwaitingApproval phase and cancels under one lock. Ordinary explicit operation cancellation keeps its existing behavior.

Validation on October 12, 2026:

- The original browser test failed with `Human approval must remain available while agent chat streams`.
- After the fix, all nine headless Edge workflow fixtures passed, including streaming Claude and Codex proposals, locked inputs, no dispatch before clicking, and approval while chat remains active. IPC/chat are mocked in these fixtures.
- The backend regression passes cancellation-before-approval, approval-before-cleanup, and stale job identity. An approved dispatch remains Running with its cancellation flag unset.
- All 115 regular Rust library tests passed; 34 actual-runtime tests were ignored. TypeScript and production Vite build passed.
- All-target custom-protocol Clippy passed with warnings denied.

The source hashes in `source-inputs.json` identify this gate. The release helper, desktop application and NSIS build passed. `package-identity.json` identifies the unsigned installer: 5,333,643 bytes, SHA-256 `9d2ba301ae48e3f3ac33f84f249c8bebf0f6c897f6c07d320773ec86a954941d`.

Native UI input remained stopped. Signed-in conversations, installed GUI interaction and fresh installed acceptance of this change require separate evidence. Installer `33e45402…` predates this change; its retained backend/lifecycle evidence remains historical evidence for that binary. No runtime guest was started for this gate.
