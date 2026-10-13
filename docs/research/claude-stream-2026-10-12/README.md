# Claude Code streaming boundary

`claude.rs` adds the restricted CLI argument contract, bounded streaming decoder and owned Windows turn runner. The desktop now wires an explicitly selected Claude Code executable into its conversation UI, with streamed replies, follow-up/resume requests, new conversations and stop controls. This is source and browser-fixture coverage; authenticated native chat acceptance remains open.

The argument contract disables built-in tools, uses explicit/strict Hazlo MCP configuration, rejects permission prompts, disables slash commands and applies restricted mode plus session-specific hook disabling. [Official CLI reference](https://code.claude.com/docs/en/cli-reference) documents restricted mode's behavior and retained managed policy. [Hooks reference](https://code.claude.com/docs/en/hooks) explains that session hook disabling does not override managed hooks. The application must not claim that it overrides administrator policy. [Programmatic CLI documentation](https://code.claude.com/docs/en/headless) describes streamed JSON, terminal results and subscription differences from bare mode; this contract preserves the normal client's authentication instead of silently changing to API-only bare mode.

The decoder accepts complete newline-delimited JSON only, preserves split UTF-8, bounds individual/total output and accumulated text, checks session identity, refuses tools outside Hazlo's catalog and requires the exclusive Hazlo connection. Partial tool arguments are not dispatched by this parser. Engineering effects continue through the separate typed MCP host and human approval. Text deltas are streamed while the final result replaces their accumulated text, avoiding duplicate output. An incomplete stream, mismatched session, unexpected tool/server or event type, malformed record or false success is rejected. Error results remain terminal errors rather than successful assistant replies.

The runner uses existing owned Windows process wrappers, closes prompt input, drains stderr without exposing it, limits turn/wait duration and supports cancellation during prompt sending and output reading. It verifies regular input files and pins working-directory lineage. Session resumption is part of the argument contract; real resumption and live cancellation remain unverified.

Validation: 85 regular tests pass, 28 separately scoped ignored entries remain, and all-target desktop/custom-protocol Clippy passes. Three new regular tests cover bytewise UTF-8, partial/final duplication, terminality, catalogue/session/record refusals, missing-login results and restricted argument construction.

The actual installed Claude Code 2.1.285 stream gate passes with a private credential-free home and the release Hazlo MCP helper. It initializes exclusively against Hazlo's six allowed tools and produces a completed error for missing sign-in. No signed-in model response or paid inference is claimed. Latest passing root: `target/claude-stream-probe/d9876b22-5458-4df5-b167-de16a4cff35f`, 0.87 seconds.

Reproduce with the identified installed executable and release helper:

```powershell
rtk proxy pwsh -NoProfile -Command '$env:HAZLOMEXA_TEST_CLAUDE="C:/Users/USER\.local\bin\claude.exe"; cargo test -p hazlomexa --lib actual_restricted_stream_without_credentials -- --ignored --nocapture --test-threads=1'
```

The desktop validates the selected executable's version/hash, rechecks its hash under a regular-file read lock and pins its parent lineage. Each turn gets a project-bound temporary bridge; stopping closes owned processes, and changing projects requires stopping the conversation first. A rejected workflow project change preserves conversation state. Temporary MCP configuration is removed even when bridge closure reports an error. Claude conversation storage is separate from guided workflows and remains visible when switching engineering workflows. Browser fixtures cover unknown measurements, a follow-up, a new session, escaping and the stop control. They simulate replies and do not prove actual inference or process cancellation.

Latest credential-free real-client gate: `target/claude-stream-probe/4974857a-e6fa-4d1a-ba29-5126e2376862`, 9.10 seconds. The full regular suite remains 85 passing/28 ignored; desktop/custom-protocol Clippy and the TypeScript/Vite build pass.

Pending: persistent backend session recovery, a signed-in clarification/tool-driven conversation, additional supported streamed events as evidenced by actual turns, live cancellation/reconciliation and native/installer acceptance. Codex embedded chat and the remaining engineering tool surfaces are unfinished. The full application goal remains unchanged.

Retained [scoped status and source hash](status.json) and [actual emitted views](updates.json) preserve the passing authentication-error gate. Temporary MCP configuration, private client data and credentials are not included.
