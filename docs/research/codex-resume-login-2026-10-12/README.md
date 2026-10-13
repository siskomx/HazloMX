# Codex resume with renewed tools and owned account connection

October 12, 2026. This extends the [owned runner](../codex-runner-2026-10-12/README.md). The backend now shares one owned-client implementation between chat and account connection. It retains pinned paths/files, runtime hash validation, bounded I/O, cancellation and checked child shutdown. The desktop is not yet wired to these Codex operations.

## Actual resumed client/tool gate

`cargo run -p hazlomexa --example codex_chat_protocol -- <installed codex.exe> resume` passes with Codex 0.159.2. Retained files prefixed `resume-` came from `target/codex-chat-probe/2a38766e-40a1-44d9-9262-3afe92d59b97` after the final account validation and client refactor.

1. The first owned client stores a completed fixture conversation and stops.
2. Its Hazlo bridge is revoked; a direct reconnect using that old capability is refused.
3. A new bridge receives a different pipe/token and its configuration replaces the temporary configuration in the same private Codex home.
4. A second actual app-server resumes the same thread. The next request contains the original user message, earlier assistant message and new follow-up message.
5. The loopback fixture supplies a typed `get_workflow_status` function call in the Hazlo namespace. Codex dispatches it through the renewed bridge and sends the actual host result back to the fixture. The diagnostic parses the returned JSON and checks the selected project ID, no job and empty history. Engineering state remains unchanged.
6. The resumed turn completes; the owned client stops, the new bridge closes and the temporary capability configuration is removed.

The captured model-facing surface remains the ten-function Hazlo namespace plus the four documented infrastructure helpers. This is an actual client, persistence and MCP dispatch test driven by scripted loopback responses. No inference or natural language reasoning is claimed, and no engineering edit was executed.

## Actual account boundary

`cargo run -p hazlomexa --example codex_login_protocol -- <installed codex.exe>` passes. `login-status.json` came from `target/codex-login-probe/92c33161-2e9f-40ad-b382-66201695b350`.

The fresh private home has no connected account. Through the [supported app-server login API](https://learn.chatgpt.com/docs/app-server), Codex supplies its official HTTPS sign-in URL and owns the local callback server. The gate cancels as soon as the link is received; it does not open that link. The cancellation finishes in 94 ms, the callback port no longer accepts connections, and the cancelled attempt creates no `auth.json`. The OAuth URL/state/query is deliberately not retained. The retained `browser_opened: false` field describes the gate's lack of an open-browser action; browser inventory was not instrumented.

A subsequent chat attempt is refused for missing login before thread/tool/model dispatch. No signed-in account, model turn, global settings change or credential copying was used. `authenticate` returns an official URL and connection status; Codex owns credential storage. The shared policy explicitly uses file storage in the supplied private home, avoiding reliance on another connection's keyring. HTTPS host/user-info/port/fragment checks reject foreign login links. Account status must contain its required authentication flag and either no account or a supported account type; malformed/foreign account metadata is refused.

## Checks and remaining scope

100 regular Rust tests pass with 30 separately scoped ignored entries; eight Codex boundary tests, desktop/custom-protocol Clippy and formatting pass. The new regular assertions cover account metadata and login-link validation. The actual resume and missing-login gates pass after the client refactor.

Complete the native provider/runtime/account controls, official-link opening through an explicit user action, event sequencing, project revocation and late-proposal acceptance. Successful browser sign-in, signed-in model-specific tool surfaces, natural language proposal/approval/edit/readback conversations, account/session recovery in the desktop, installer and second-Windows acceptance remain unverified. All remaining engineering integrations and live editing requirements stay in the full application goal.
