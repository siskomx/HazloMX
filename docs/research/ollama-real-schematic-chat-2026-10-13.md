# Real Ollama cloud schematic conversation — October 13, 2026

The existing configured cloud route passed a five-turn Spanish conversation through production `ollama::chat::run`, `HostTools` and the native saved-copy schematic workflow. Responses came from the real provider, without scripted assistant responses. This is application acceptance of the existing route, not a model benchmark.

The identified local Ollama daemon was **0.35.1**, serving the existing `kimi-k2.7-code:cloud` profile with the pinned digest `eda07a6592375dcbde7cf167b6d6b368cdd28e244f9d71559fb59919aca882fa`. Its metadata advertised completion and tool support. No model was downloaded and no provider was added. Claude Code independently still reported signed out; this result does not close its authentication or conversation gate.

## Observed conversation

| Turn | User intent | Successful host results observed |
|---|---|---|
| 1 | Inspect the selected schematic, explain its resistors and ask which to change | `inspect_selected_schematic`; source-grounded question; no job |
| 2 | Change the identified resistor to 4.7k, for review | Inspection and `propose_schematic_value`; exact reference and original value checked |
| 3 | Read and explain the approved result | `read_verified_result`; explanation includes 4.7k |
| 4 | Change the same resistor to 10k from the edited copy | Result readback and proposal; actual proposal expects 4.7k, not the original value |
| 5 | Read the final result | `read_verified_result`; final native schematic parser confirms 10k |

The harness supplied the two approvals directly through the backend after checking the pending proposal. Native Konnect/KiCad execution completed both jobs. All four original fixture files and the first edited output remained byte-identical after the follow-up. Reopening the project recovered both completed jobs.

An observational forwarder on an owned loopback port sent request bodies and response bodies/status to and from the fixed local daemon unchanged. HTTP headers/framing differ from the direct desktop connection. It recorded 27 requests, including 12 chat requests, with no HTTP or recorder errors. Successful tool assertions use actual role-tool results returned by the production host in subsequent requests, rather than assistant claims. Response chunks were forwarded immediately; inference was not buffered before returning headers.

Approval nonces were absent from every recorded request. A recursive scan, including JSON nested inside tool-result strings, inspected 5,524 request strings and found no builder home-directory paths. Raw conversations, source copies, request/response records and process logs remain private under ignored `target/`; none are published here.

## Reproduction and limits

Build the `ollama_schematic_chat` Rust example, then explicitly run `python apps/desktop/tests/ollama-schematic-chat.py --run-real-cloud` from the repository. This consumes the existing configured cloud route and requires the already identified Konnect/KiCad runtimes under `target/`. The launcher has a fresh owned fixture root, fixed upstream routing, bounded recording and private failure logs. Missing authentication/model/runtime prerequisites must be resolved separately; there is no automatic download or retry.

The passing run used the private launcher before promotion into the reusable test script. The promoted script adds explicit opt-in, process cleanup, missing-example checking and privacy/error assertions; those assertions were also checked against the completed recording. Only static/build checks were repeated after promotion, avoiding another cloud inference run for recorder-only changes.

This proves one real **backend saved-copy schematic conversation** with fixture approval. It does not prove the desktop approval-to-AI handoff, native pickers, installed chat, ordinary-user Windows acceptance, direct desktop transport, local-model behavior, Claude/Codex conversations, other programs or live/unsaved editing. Native UI automation remains paused. The full goal stays active.
