# Installed agent-client protocol gate

The installed Codex 0.159.2 app-server connects to Hazlo's release stdio helper and authenticated local pipe. An ephemeral client thread discovers the exact three-tool catalog, reads a user-selected fixture component and receives a structured success. A stale selection produces `isError: true` with no success payload. No `turn/start` is sent, no model inference occurs and no sign-in credentials are supplied.

Claude Code 2.1.285 separately uses its supported `mcp add-json` and `mcp list` commands with an owned temporary configuration/home. Its health check reports Hazlo connected. This proves client initialization/health, not a Claude tool call or model-driven edit. The diagnostic does not modify the user's existing agent configuration, read credentials or use a paid API.

The reproducible gate is `apps/desktop/src-tauri/examples/codex_protocol.rs`. Pass the actual installed `codex.exe` and optional `claude.exe` paths; build the release helper first. It creates a private `target/codex-probe/<UUID>` root, uses owned Windows process supervision, bounds records/deadlines, refuses unexpected server action requests and revokes the capability on exit. Temporary configuration is not retained in public evidence.

```powershell
rtk cargo build -p hazlomexa --release --bin hazlomexa-tools
rtk proxy cargo run -p hazlomexa --example codex_protocol -- <absolute-codex.exe> <absolute-claude.exe>
```

The passing root is `target/codex-probe/d234e24d-5830-4709-a8ee-03d8e1e65acc`. The generated installed-CLI schemas confirm that MCP status/tool-call routes can be exercised without starting a model turn. This confirms the displayed MCP configuration formats against actual clients rather than an SDK-only substitute. Clippy passes all targets including the new executable gate.

Retained evidence: [scoped result](status.json), [Codex initialization](initialize.json), [connected catalog](mcp-status.json), [structured component read](tool-read.json), [host refusal](tool-refusal.json), [Claude health output](claude-mcp-list.txt) and [source/runtime hashes](identity.json). OAuth status `unsupported` for the stdio server describes that protocol's OAuth mechanism; the helper still authenticates to Hazlo's pipe using the temporary capability before MCP starts.

Remaining: signed-in/model-driven turns and edits through both clients, embedded conversation, native connection/discovery UI, other workflow tools, live cancellation/reconciliation, current installer and second-Windows acceptance. A CLI health check and explicit app-server tool RPC do not prove natural language reasoning.
