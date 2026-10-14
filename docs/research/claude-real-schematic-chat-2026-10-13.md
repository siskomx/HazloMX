# Real configured Claude schematic gate — October 13, 2026

`claude_schematic_chat` is a reusable Windows acceptance harness for the application's real configured Claude Code route. It uses the existing restricted stream decoder, pinned client identity, authenticated shared MCP bridge, selected-source capabilities and native Konnect/KiCad saved-copy runner. It never enables shell, approval or hardware tools.

The intended five-turn gate is source inspection and clarification, an explicitly requested value-change proposal, native execution after backend fixture approval, verified-result explanation, then a second result-derived proposal/execution/readback. The harness requires actual observed tool names, the source-grounded target/old/new values, unchanged originals and earlier output, and recovered completed runs. No fixed model name or synthetic provider responses are supplied. This is a real-agent **backend** gate; its approval fixture does not prove native UI approval or an installed conversation.

## Observed attempt: failed, authentication prerequisite

The installed client identifies as verified **Claude Code 2.1.285**. Its first real turn initializes the exclusive restricted Hazlo MCP connection, then returns a failed result before any observed engineering tool call. The application's generic failure remains intact. Independent `claude auth status --json` reports `loggedIn: false`, `authMethod: none`; the saved OAuth expiry is in the past. No credential values are copied, displayed or published.

The owned test project contains **zero engineering jobs and zero native artifacts**. Its four source files remain byte-identical to the known fixture. The ephemeral MCP configuration is removed after the failed turn and the bridge is closed. Private streaming records, authentication output, source copies and diagnostics remain outside Git.

The harness compiles and `cargo clippy --features desktop --examples -- -D warnings` passes. The real-agent gate is **not passed**: no source inspection, conversational clarification, native change, readback or follow-up is established by this attempt. Personal sign-in through the user's own Claude Code client was requested; the test must not be retried as a successful-account workflow until that prerequisite changes. No model download, benchmark, native UI action or hardware effect ran.

The shipped application source and installer candidate remain unchanged; this adds only the acceptance harness and evidence. Native UI automation is separately still paused after Escape, with a resume request pending. The full cross-program application goal remains active.
