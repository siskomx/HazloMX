# Real FreeCAD conversation acceptance — October 14, 2026

This gate uses the existing configured Ollama cloud route and the production host tool loop with FreeCAD 1.1.4/CADPilot 0.7.7. Native work runs headlessly on fresh saved copies of the public fixture. It does not resume native UI automation, open an editor or establish live/unsaved editing.

## Failure that exposed an application gap

The first real run inspected the native document and asked a source-grounded question about the block's measured height, without creating a job. The user then requested a final height of two inches while preserving length and width. The model repeatedly sent malformed proposal structures, including dimensions at the wrong nesting level and guesses for the object field. All six malformed proposals were refused; no native edit ran. The assistant's final prose said it would prepare the change, but the harness required an actual pending proposal and failed instead of accepting that claim.

The generic argument error supplied no correction structure. The shared host now returns the tool's exact advertised JSON schema when argument deserialization fails. It does not echo invalid field names, supplied values, source content or paths; does not add defaults, accept aliases, loosen validation or grant approval. Every configured provider uses this same host boundary. The tool name and source/project/hash requirements remain unchanged.

New tests cover every advertised tool's malformed-call refusal with exact schema-only feedback and no dispatch, plus real loopback HTTP where a model receives the error, sends corrected arguments and reaches human review without execution. Their first run passed behavior assertions but exposed Windows fixture cleanup while the project lock was still held; the tests now drop their workflow owners before deleting their own fixture directories.

## Reproduction and scope

Build both `ollama_cad_chat` and `ollama_schematic_chat` examples. Explicitly run `python apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program freecad`. The existing schematic recorder is reused; conversation/history, observation of actual successful tool results and fixture approval are shared between the two examples. The earlier schematic inference was not repeated for this extraction.

The FreeCAD scenario requires five real turns: native inspection/clarification, a proposed final height of 2 inches (50.8 mm), verified-result explanation, a result-derived increase of 5 mm (55.8 mm), and final readback. Assertions require unchanged 10/20 mm length/width, actual native report measurements, equality of follow-up before-state and prior after-state, unchanged original and first FCStd/STEP/report outputs, and recovery of exactly two completed jobs. Approvals are supplied by the backend fixture only. Recorder bodies are forwarded unchanged through an owned loopback port to the fixed local daemon; this is not direct desktop transport acceptance.

## Corrected real run: passed

After rebuilding against the host correction, the same scenario and assertions passed all five turns and two actual native jobs. The real provider initially malformed both the first-change and follow-up calls. In each turn it received the schema error, corrected the call and reached the expected pending proposal. This directly exercises recovery from the failure that motivated the change; it is not a scripted model response or a relaxed assertion.

Native reports confirm final heights of **50.8 mm** and **55.8 mm**, with **10 mm length and 20 mm width** preserved. The follow-up starts from the prior report's actual after-state. Original bytes, all three first-result outputs and two-job recovery pass. Every recorded HTTP request succeeded: **29 total, 14 chat**, with no recorder errors. Approval nonces and builder home paths, including paths nested inside JSON tool-result strings, are absent from provider requests. Raw records remain private under ignored `target/`.

The full regular Rust suite passes **141 tests, 52 ignored**. Both real-conversation examples build; desktop-feature Clippy for the production binaries/library and both examples passes with warnings denied. Syntax/help checks pass for the renamed opt-in launcher. Existing schematic inference was not repeated merely for shared test-code extraction. No model download, provider addition or benchmark ran.

This closes the existing cloud route's **FreeCAD saved-copy backend conversation** gate, with fixture approval. Native source picker/approval/result viewing, the desktop approval-to-agent handoff, ordinary-user/current-installer acceptance, live/unsaved editing, broader native tool coverage, other providers and the remaining program conversations are unfinished. The retained installer must be rebuilt against the changed production host. Native UI automation remains paused and the full goal stays active.
