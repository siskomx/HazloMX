# Installed Codex change/readback boundary

October 12, 2026. Installed Codex 0.159.2 connects through the release stdio helper to the desktop-owned authenticated bridge. This extends the existing credential-free app-server gate to the current ten-tool catalog and the change/readback boundaries. It does not start a model turn.

The actual client discovers the exact current catalog, reads measured CAD capabilities and preserves `live_editing: false`. It also reads the selected sketch and prepares a schema-10 MCP proposal requiring human approval. The diagnostic cancels that proposal and changes the selected source; Codex preserves the resulting stale-source refusal.

Two additional actual-client calls verify the execution loop's boundaries: `propose_cad_document_change` refuses a job that is not a verified completed CAD result, and `read_verified_result` refuses the cancelled sketch proposal. Both return structured tool errors without a success payload. The desktop still has exactly one cancelled history entry; rejected requests create no replacement job.

The retained `status.json`, catalog/capability responses, sketch proposal/refusal and CAD/result refusals come from `target/codex-probe/f7a7b081-bdfd-4fd9-8e22-51fc2401bd76`. Source/helper/client hashes are in `identity.json`. The gate stops its owned app-server process, revokes the bridge and removes its temporary MCP configuration. Tokens and credentials are excluded from this evidence.

Reproduce from the repository root with the installed executable path:

```powershell
rtk cargo build -p hazlomexa --release --bin hazlomexa-tools
rtk cargo run -p hazlomexa --example codex_protocol -- <absolute-codex.exe>
rtk cargo clippy -p hazlomexa --all-targets --features desktop,custom-protocol -- -D warnings
```

The actual client gate and all-target desktop/custom-protocol Clippy pass. Positive native CAD editing/readback is covered separately by the [actual helper/SDK CAD gate](../measured-cad-tools-2026-10-12/README.md); this Codex gate does not execute a CAD edit. Signed-in natural-language reasoning, embedded Codex conversation, native connection UI, installed application acceptance and live editing of open documents remain unfinished. The full application goal remains active.
