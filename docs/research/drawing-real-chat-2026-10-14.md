# Real CADCraft conversation acceptance — October 14, 2026

This gate exercises the existing configured Ollama cloud route through production host tools and CADCraft 0.5.0's identified native headless CLI. It uses a fresh copy of the public metric drawing, the reviewed native command catalog and backend fixture approvals. Native UI automation remains paused; this is saved-copy execution, not live-window editing.

## Initial failure and product correction

The first real run successfully inspected native entities and asked for missing movement information without creating a job. After the user confirmed millimeter drawing units and explicitly requested a one-inch rightward movement proposal, the AI inspected again, explained the correct conversion and desired circle position, then asked another confirmation before sending it for review. It did not call the proposal tool. The harness required an actual pending native proposal and failed; no edit ran.

The installed Codex/Claude instructions now explicitly distinguish preparation from execution: when the person requests a proposal and the necessary inputs are confirmed, use the proposal tool to create the pending review rather than asking another confirmation merely to prepare it. Saying it is not yet approved does not revoke the request to prepare it. An assistant description is not an actual pending proposal, and only the person may approve execution. Ollama now reuses those same instructions instead of a shorter separate policy. Credential, saved-copy/live-document and verified-result boundaries are retained.

The loopback protocol regression checks that the outgoing Ollama system instruction is exactly the same instruction used by the installed clients. Existing tests remain responsible for call validation, human-only approval, cancellation and source/project binding.

## Reproduction and assertions

Build `ollama_drawing_chat`, then explicitly run `python apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program drawing`. This uses the existing cloud account; no model is downloaded or benchmarked. The shared test helper records visible conversation context and observes actual successful host results in the fixed-daemon forwarding recorder. Logs and source/result copies remain private under ignored `target/`.

The five-turn scenario requires native inspection/clarification, one native movement proposal, readback/explanation, a result-derived second movement proposal and final readback. The first movement is **1 inch = 25.4 mm** horizontally; the follow-up is **5 mm** farther right. The user never supplies an internal entity handle. The model must resolve the single circle from native inspection and prepare one `move` command with the correct handle/delta in each proposal; optional native selection calls are permitted, but no unrelated transform is accepted. Each execution requires separate backend fixture approval.

Assertions check source hashes, confirmed drawing units, loss refusal, native saved geometry and all original entity properties. Only the chosen circle's horizontal center may change; its radius, vertical position and every unrelated entity must remain unchanged. The second proposal's actual captured source hash must match the first saved DXF. Original bytes and the first drawing/preview/report outputs must remain unchanged, and the project must recover exactly two completed jobs.

## Corrected real run: passed

After rebuilding with the shared instructions, all five turns and two actual native jobs passed. The agent created the requested pending proposals without another confirmation merely to prepare them. Each execution still waited for the fixture's separate backend approval. In both proposal turns the agent also corrected a malformed call after receiving the shared schema feedback introduced by the FreeCAD gate; no validation was loosened.

Native readback verifies **25.4 mm** and then **5 mm** rightward movements, unchanged circle radius/vertical position and exact preservation of all unrelated entities/properties. The second captured source hash matches the first saved drawing, so its movement is applied to the edited copy. Original/first DXF/preview/report bytes and recovery of exactly two completed jobs pass.

The recorder observed **30 requests, including 15 chat requests**, all with HTTP 200 and no recorder failures. Successful host tools are observed in actual protocol results, independently of assistant prose. Approval nonces and builder home paths, including paths nested in JSON tool-result strings, are absent from provider requests. All raw recordings and fixture trees remain private under ignored `target/`.

Regular Rust validation passes **141 tests, 52 ignored**. The new example builds; production desktop binaries/library and example Clippy pass with warnings denied. Launcher syntax/help checks pass. The first example build accidentally referenced a crate-private hashing helper; using the already installed SHA-256 dependency fixed the acceptance harness without changing the library API. No provider, model, benchmark or dependency was added.

This establishes one real existing-cloud **CADCraft saved-copy backend conversation**. The broader 33-command parameter surface, other providers, direct desktop approval-to-agent handoff, native pickers/views, ordinary-user/current-installer acceptance and live document editing remain unfinished. The retained installer predates the changed instructions and must be rebuilt for current-source acceptance. The full goal stays active and native UI automation remains paused.
