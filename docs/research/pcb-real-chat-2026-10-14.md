# Real KiCad PCB conversation — October 14, 2026

Five real conversation turns through the existing configured Ollama cloud route pass the production host loop with Konnect 0.13.0 and KiCad 10.0.7. The programs edit and check saved copies; Hazlo supplies conversation, source binding, review and verified readback. No editor was added to Hazlo.

The user first asks to move a connector without knowing its reference or destination. The agent must inspect the actual selected board, explain its connectors, ask for the missing choice/position and leave the project without a proposal. The next turn selects the observed J1 connector and explicitly confirms an absolute center at X 8 inches, Y 6 inches. The proposed destination must equal 203.2 mm, 152.4 mm and bind the inspected original position. Separate backend fixture approval then runs the native edit and KiCad checks.

After a verified-result explanation, the user asks to move the same connector 2 mm farther right without changing height or orientation. The agent must read the completed result, inspect that saved copy using its job/hash pair, and prepare a new proposal based on the actual updated position. A second separate fixture approval produces the final saved position 205.2 mm, 152.4 mm.

Both saved boards are independently parsed to verify the connector UUID, exact positions and unchanged rotation. Verified result responses include native KiCad check summaries. Original board/project/schematic/worksheet bytes and the entire first edited board remain unchanged. Reopening the project recovers exactly two completed jobs. These assertions inspect actual files and successful protocol tool results, rather than relying on assistant claims.

The recording forwarder observed 32 requests, including 17 chat requests, all HTTP 200 without recorder errors. Request bodies exclude approval nonces and builder home paths, including nested JSON strings. Raw conversations and native artifacts remain private under ignored `target/`.

Reproduce by building `ollama_pcb_chat`, then running `python apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program pcb`. It uses existing runtimes and cloud configuration without downloads. Both new PCB and Arduino Agent conversation examples pass build and Clippy with warnings denied.

This certifies one existing-cloud saved-copy PCB conversation with backend fixture approval. Moving a footprint can disconnect tracks; native check findings are not a guarantee of a manufacturable board. Native approvals, live/unsaved editing, installed UI, other providers and hardware remain unverified by this gate.

The Arduino Agent conversation fixture is also prepared: source inspection and clarification, 500 ms and 250 ms edits, native readback, preserved originals/prior copies and recovery. It is built but has not run: no Agent listener was available, and native app launch remains paused after Escape. The installed KiCad workflow's obsolete 17-tool assertion now compares against the current host catalog, preventing a stale test expectation when the actual installed connector is exercised.

## Current package

The rebuilt installer SHA-256 is `6a2f7ef9cbe6778f916d28cda2d2ac22cde2fe2c09acffbf5f5bc3ad8a61fdd5` (5,534,473 bytes). The build verifies all 6,573 tracked inputs remained unchanged and finds zero builder-profile path matches in the shipped app/helper binaries. Its actual release helper passes the SDK protocol test against all 26 current tools, rejects unknown selections and exposes no approval tool. A first host fixture attempt passed a relative project path and was refused before connecting; the corrected absolute-path run passes.

This is a built package and host connector check, not a current installed-app lifecycle or native UI pass. Native launch remains paused after Escape, and no new guest has been launched for this package.

The refreshed installed-workflow example also passes on the host against that exact release helper: two real schematic edits and two real PCB moves, inspected follow-ups, preserved sources/prior copies and recovery. This directly exercises the corrected catalog assertion without treating host execution as a second Windows environment. A fresh guest package is prepared with matching installer, installed-app and helper identities, all 26 tools and the initial chat checks; preparation passes without launching it. Arduino Agent and native picker/approval/cancellation/current installed UI acceptance remain unfinished.
