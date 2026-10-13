# Owned Codex chat runner and actual client protocol fixtures

October 12, 2026. The backend now identifies and hashes installed Codex 0.159.2, starts its app-server inside an owned Windows Job Object, initializes the protocol, checks account state and the complete Hazlo MCP server/catalog, starts a turn and streams thread-bound messages. It accepts an existing thread ID for resume, but resume has not been verified. The desktop is not yet wired to this runner.

The runner uses an explicitly supplied private Codex home and directory, bounded JSON records/RPC event queues/turn output, a five-minute deadline, a 25 ms cancellation check, and checked owned-process termination/wait. It rejects client action requests and refuses an absent account before starting a turn when the provider requires OpenAI authentication. That missing-login branch remains unverified against the actual default provider. Configuration creation and bridge revocation belong to the caller; this gate creates its own connection, removes the temporary capability configuration and closes the bridge after every case.

`codex::configuration` generates the launch configuration shared by the runner gate. The captured fixture request advertises the exact ten Hazlo functions in one namespace, plus MCP resource listing/template/read helpers and `request_user_input`. Shell, exec, file editing, image access, browser/computer use, goal tracking, plugins, hooks, agent spawning and other native action surfaces are absent from this captured catalog. A question/client action request is currently refused rather than forwarded into desktop question UI; plain assistant messages can ask questions.

The fixture uses the documented [custom provider configuration](https://learn.chatgpt.com/docs/config-file/config-reference) and [app-server interface](https://learn.chatgpt.com/docs/app-server). The HTTP listener binds to a random loopback port and rejects authorization headers. It performs no inference: it returns either a scripted SSE message, an HTTP 400, or holds a request until the owned client is cancelled. No credential copying, cloud model turn, global configuration change or engineering execution was used. These are protocol fixtures, not natural language or model quality evidence. The catalog check is scoped to this fixture model and installed client; actual signed-in model combinations remain unverified.

## Actual runner cases

Run `cargo run -p hazlomexa --example codex_chat_protocol -- <installed codex.exe> <mode>`, where mode is `success`, `failure` or `cancel`.

| Case | Actual evidence | Result |
|---|---|---|
| Success | `success/`, source `target/codex-chat-probe/20077128-7e1e-4d30-8d7d-0ef271d0f267` | Partial Spanish UTF-8 text arrives before terminal completion; final text is exactly the fixture's message, without duplication. Exact outer catalog and ten-function namespace pass. |
| Failure | `failure/`, source `target/codex-chat-probe/af518625-f272-4142-b985-b29b2473c864` | Actual HTTP 400 becomes a failed turn with a user-safe error, no successful reply or cancellation. This run preceded the explicit captured-catalog assertion; the complete captured tools are retained. |
| Cancellation | `cancel/`, source `target/codex-chat-probe/45a67c13-7f3d-4ba9-b8d3-927923707217` | Cancellation is triggered only after the actual client has sent its request. Held socket closes in 77 ms; the runner kills and waits for its owned child and returns cancellation. Exact catalog passes. |

The first cancellation fixture incorrectly required graceful TCP EOF. Killing the Windows client produced connection reset (10054), so the diagnostic failed despite the closed connection. The fixture now accepts EOF or that reset and still rejects other errors/timeouts/extra input. `first-cancel-updates.json` retains the first run's updates; its source is `target/codex-chat-probe/8ca289f6-fda0-4507-a9bc-eee7b25f5ab7`. This was a diagnostic expectation fix, not evidence of engineering-operation cancellation.

`actual-failed-events.json` and `actual-completed-events.json` retain earlier raw notifications from installed-client fixture runs (`8d5330c0-c3a4-4784-b145-bd65f3e88cb0` and `fc0f2f5e-cfe9-4bb0-8812-6be90f6c8f11`). A regular regression test replays these through the turn decoder. Those captures validate event compatibility; they do not certify the newer launch configuration.

The separate actual ten-tool RPC gate passes after switching to the shared RPC helper: source `target/codex-probe/36a0e15c-a054-48bb-be4e-ada8931d2dac`, retained `tool-client-status.json`. It reads selected files, prepares a proposal for human review and preserves stale/unverified/cancelled-result refusals. The RPC helper preserves notifications received before acknowledgments and refuses mismatched responses and client actions.

Validation: 99 regular Rust tests pass with 30 separately scoped ignored entries, including seven Codex boundary tests; desktop/custom-protocol Clippy and formatting pass. No frontend behavior changed.

## Remaining application work

Finish normal account connection/login handling in the owned home, actual session resume with refreshed bridge capabilities, question forwarding, project-change revocation and late-proposal cleanup acceptance, signed-in tool-driven conversations, native desktop controls and installer/second-Windows verification. Actual model-specific tool surfaces and the full production deadline remain unverified. A completed Codex reply still cannot certify an engineering result; approved application and native `read_verified_result` remain authoritative. Live editing of open engineering documents and all remaining integration gates stay in the application goal.
