# Open FreeCAD chat acceptance — October 14, 2026

The bundled production desktop app completes the first visible native edit/readback/follow-up gate using the existing Ollama Cloud route, FreeCAD 1.1.4 and the pinned CADPilot addon. Hazlo remains a chat. All geometry changes occur in one owned, unsaved native FreeCAD document through its GUI-thread tools, without selecting a saved source or reloading a file.

The conversation starts with an idea and unknown measurements. Hazlo reads the open part, explains its measured dimensions and asks for the missing direction and length. The person supplies 1.2 inches as the final length, preserving width, height and position. Hazlo prepares a real pending proposal with readable before/after facts. Independent native readback confirms no write occurred before the UI approval.

| Native observation | Length | Width | Height |
| --- | ---: | ---: | ---: |
| Before approval | 25.4 mm | 20 mm | 10 mm |
| After approved 1.2-inch request | 30.48 mm | 20 mm | 10 mm |
| After approved “5 mm more” follow-up | 35.48 mm | 20 mm | 10 mm |

Both native window changes were observed directly in the viewport and property editor. Each approved edit has a completed native transaction with undo available. The second proposal starts from the first result. Its before fingerprint equals the first transaction's after fingerprint; its after fingerprint matches independent current native readback. The exact in-process document token is unchanged. Hazlo automatically reads the verified report and the current open document after both approvals.

The test found and fixed three integration defects: material property ZIP timestamps falsely invalidated unchanged state; capability guidance mixed saved-copy action names with native actions; and desktop context still advertised saved-copy-only scope. Material fingerprints now hash bounded persisted archive contents, while genuine content changes still invalidate a proposal. A single open document is inspected automatically; listing several documents requires choosing one, rather than falsely reporting an unsupported connection.

Validation includes six fixed-script regression tests, a delayed real-engine test crossing the ZIP timestamp boundary, two real native headless edits/undo/atomic rollback/close-reopen refusal, the 150-test regular Rust suite (52 ignored), the updated native discovery protocol test, library Clippy, production frontend/release builds and all 40 production browser approval/review handoffs across four AI routes. Browser tests are IPC fixtures; the visible signed-in conversation above uses Ollama Cloud only. Captured implementation inputs remained unchanged during final acceptance. The tested main executable SHA-256 is 879aadcabb2c484297170ab2bb6fc627348e8ea8dd7c083817477f7a74218945.

Raw captures, native observations, reports, profiles and logs remain private in ignored build output. No credentials, personal file paths or raw desktop captures are published.

This closes only the first visible FreeCAD interaction gate. Live save/export, broader native geometry, real Codex/Claude conversations, other programs' equivalent live interaction and a current installer remain unfinished. Native quantities can still show floating-point noise in optional review facts, and AI prose sometimes includes unnecessary native names; presentation refinement remains open. The full application goal stays active.
