# Selected Arduino sketch tools

October 12, 2026. Two tools now connect the existing desktop-selected Arduino source and approved Agent copy-edit workflow to the shared MCP host:

- `inspect_selected_sketch` reads only the selected project snapshot, returning the actual source, file name and SHA-256. It makes no arbitrary file read and exposes no authentication data.
- `propose_selected_sketch_edit` binds replacement code to the inspected project/name/hash. Unknown fields, wrong project/name/hash, changed selections, oversized code and overlapping proposals are rejected. It creates a pending schema-10 MCP proposal with the original/new code; no approval token is returned.

Both the manual desktop and tool route reuse the same workflow preparation. Actual effects remain behind Hazlo's one-use human approval. The existing Arduino Agent executor sends `set_content` only after checking `get_content` on the private copy, then verifies `get_content` and a disk read against the approved code before accepting the output. The engineering software owns the result; an acknowledgment alone is insufficient. This new tool gate does not repeat the authenticated Agent execution gate or prove live native editing.

Validation: 86 regular library tests pass, with 28 explicitly ignored runtime/guest gates. The new regression covers absent source, source binding, stale/foreign selection, arbitrary-path rejection, code limits, pending proposal exclusion, original/new review, approval denial after cancellation, MCP provenance and cancelled-run recovery. Desktop/custom-protocol all-target Clippy, TypeScript/Vite build and eight-workflow browser fixtures pass.

Installed Codex 0.159.2 actually discovers the exact eight-tool catalog, reads the selected sketch, prepares the desktop approval proposal and preserves the host refusal after changing the source. Owned Codex shutdown is observed. Installed Claude Code 2.1.285 reports a healthy connection in private configuration and passes the expanded catalog's credential-free restricted-stream initialization/error gate. Neither gate starts an authenticated model turn or performs an engineering effect.

Codex probe: `target/codex-probe/7d82dc3f-a0e7-469f-91e3-5b7a9f30bbd8`. Claude stream probe: `target/claude-stream-probe/8dc14f33-0b36-4d72-9f2e-27ae90748131` (10.89 seconds). Selected sanitized protocol results and source hashes are retained beside this file. Temporary client configurations, capability tokens and credentials are excluded.

Pending: native Agent execution through the new tools, signed-in conversational edit/readback, live native editing/session recovery and updated installer acceptance. The full application goal remains open.
