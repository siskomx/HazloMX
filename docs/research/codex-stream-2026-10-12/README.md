# Codex framing and conversation-state boundary

October 12, 2026. This advances the embedded Codex connection; it does not enable or certify a signed-in conversation.

`src/codex.rs` adds a newline-framed, bounded app-server reader and a thread/turn-bound notification decoder. The existing installed-client gate now uses that reader. Turn state follows the installed 0.159.2 experimental JSON schema and the [official app-server documentation](https://learn.chatgpt.com/docs/app-server).

## Implemented checks

- Read at most 1 MiB per record, reject incomplete records, invalid JSON and non-object events before dispatch.
- Keep RPC acknowledgments separate from streamed messages and terminal turn status. An acknowledgment never completes a conversation.
- Assemble message deltas and replace them with authoritative completed text, avoiding duplicate final replies. Bound message count, identifiers and total text.
- Bind recognized notifications to the expected thread and turn. Reject later events after completion and premature stream closure.
- Keep failed and interrupted turns separate from completed replies. Do not expose raw client error payloads as user messages.
- Recognize only the current Hazlo MCP catalog for tool notifications; reject client action requests, foreign tools, command/file actions and unsupported item types, including in terminal item lists.

The decoder observes notifications; it cannot prevent builtin tools from executing inside Codex. It is not a substitute for an independently verified restricted launch/configuration. The desktop has not been wired to this turn decoder. Before enabling embedded Codex, complete the owned session runner, client tool restriction checks, cancellation, project revocation, session handling and signed-in/native acceptance. The documented `features.shell_tool` switch alone does not establish an exclusive Hazlo tool surface.

## Actual installed-client evidence

`cargo run -p hazlomexa --example codex_protocol -- <installed codex.exe>` passes using the shared bounded reader. Evidence came from `target/codex-probe/a51d5074-5aa3-449b-83ca-f350aecd2362`.

- Exactly ten Hazlo tools discovered through the desktop-owned bridge and release helper.
- Actual selected schematic and sketch reads, source-bound sketch proposal awaiting desktop approval, stale-state refusals, measured CAD capabilities and cancelled-result refusal.
- No model turn, credential access, global configuration change or engineering execution. This gate validates tool RPC, not natural language reasoning or the turn decoder's live streaming behavior.
- Independent retained PID absence and temporary configuration removal checks in `runtime.json`; the private capability configuration is not retained.

Three regular regression tests exercise bounded framing, authoritative final text and protocol negatives. The full regular suite passes 95 tests with 30 separately scoped ignored entries; desktop/custom-protocol Clippy passes. The turn fixtures are schema-based synthetic events, not a captured signed-in reply.

Hazlo still reports an engineering operation complete only after software output verification and `read_verified_result`; completion of an agent turn alone cannot certify a design change. Live editing of open native documents and the remaining full application gates are unfinished.
