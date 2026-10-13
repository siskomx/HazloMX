# Ollama benchmark evidence

Earlier runs use the unchanged 20-case Spanish fixture and the actual Rust proposal client. The chat/units run adds ten separately frozen conversation fixtures. They generate proposals only, never CAD effects.

| Run | Acceptable | Warm median | Warm p95 | Result |
|---|---:|---:|---:|---|
| [Qwen3 CPU, initial schema prompt](2026-10-10-qwen3-4b-cpu/report.json) | 12/20 | 6.75 s | 19.69 s | Accuracy failed |
| [Qwen3 CPU, revised schema prompt](2026-10-10-qwen3-4b-cpu-prompt-v2/report.json) | 9/20 | 9.00 s | 17.22 s | Accuracy failed |
| [Kimi explicit cloud profile](2026-10-10-kimi-cloud-accepted/report.json) | 20/20 | 0.76 s | 1.14 s | Fixture accuracy and warm latency passed |
| [Kimi chat and metric/imperial units](2026-10-11-chat-units-cloud-v2/report.json) | 30/30 | 0.93 s | 1.83 s | Original regression cases plus ten conversation/unit cases passed |
| [Qwen3 CPU, chat and units prompt](2026-10-11-qwen3-chat-units-cpu/report.json) | 26/30 (17/20 original) | 5.45 s | 11.72 s | Local accuracy threshold still failed; invalid responses rejected |

The first expanded run stopped at the 8-inch case: the model returned 203.2 mm, and strict host validation rejected it. Its [failure](2026-10-11-chat-units-cloud/generation-error.json) is retained. After clarifying the converted range check in the prompt, a complete new run passed all 30 cases. This is a measured fixture result, not a guarantee for arbitrary input. The local experimental model has not been remeasured with this expanded prompt.

The initial report stored the upper middle observation as its median; `derived-statistics.json` records the conventional median separately. Later reports use the conventional median. Warm samples include nineteen remaining cases and three explicit-case repetitions. Measurements include normal desktop activity. Cloud's first request does not establish remote cold-start time.

Local Qwen3 4B Q4_K_M manifest: `359d7dd4bcdab3d86b87d73ac27966f4dbb9f5efdfcc75d34a8764a09474fae7`. Ollama 0.35.1 reported CPU execution, 4096-token context, and zero GPU memory. Download size is approximately 2.5 GB in a separate proof directory. The initial local requests took 35.80 and 33.55 seconds including loading. JSON-mode testing passed ten fully specified cases before a missing-height response failed host parsing; that run is incomplete.

The user authorized cloud evaluation. The Kimi run selected `kimi-k2.7-code:cloud`, remote model `kimi-k2.7-code`, host `https://ollama.com:443`, and manifest `eda07a6592375dcbde7cf167b6d6b368cdd28e244f9d71559fb59919aca882fa`. This pins routing metadata, not immutable hosted weights. It uses the existing signed-in account; no account keys are copied into the repository. Startup pruning is disabled for the existing user model store.

Observed cloud protocol differences:

- Routing appears in `/api/tags`, while `/api/show` forwards capabilities without routing fields.
- Explicit `:cloud` streams identify the remote model without per-record routing fields.
- Successful NDJSON was labeled `application/json`; this is accepted only in an explicit cloud profile.
- Cloud requests omit schema-constrained `format`, include the schema in their prompt, and still require strict host validation at EOF.

The [recorded streams](../../../tests/fixtures/ollama/README.md) replay at every byte boundary in regular tests. Earlier cloud probe folders preserve rejected contract assumptions; they are not model quality scores.

The local missing-height diagnostic recorded a `proposal` with height `0`; host dimension validation rejected it. This is an accuracy failure, not a reason to relax the minimum or invent a missing dimension. Its generated text is retained in the diagnostic folder.

Actual local inference cessation now passes an independent [live-runner cancellation check](../local-cancellation-2026-10-11-v2/cessation-result.json). Native UI cancellation/publication checks also pass. Full second-environment workflow/negative cases remain incomplete. No offline model accuracy claim has passed. Cloud fixture success does not remove explicit CAD approval or prove correctness for every possible request.
