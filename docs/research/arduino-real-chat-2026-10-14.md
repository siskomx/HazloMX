# Real Arduino compile conversation — October 14, 2026

This gate uses the existing configured Ollama cloud route and the production host loop with the already provisioned Arduino CLI 1.5.1 / AVR core 1.8.8. It exercises code generation, human review, native Uno compilation and verified code/report readback. There is no firmware upload, port selection, device mutation, physical behavior test or native UI automation.

## Scenario and assertions

The first turn asks for a simple built-in LED example without choosing a board or timing. The agent must consult capabilities, ask for missing information and leave the project without a job. The user then confirms Arduino Uno and one second on/one second off, explicitly requesting a proposal rather than execution. The first native compilation follows a separate backend fixture approval.

The next turn requires actual `read_verified_result` before explaining what compilation established. The follow-up asks for half-second on/off intervals based on the compiled code, still on Uno and still compile-only. A second proposal, separate fixture approval/native compilation and final code/report readback must follow.

The acceptance fixture checks a simple sketch with two literal delay calls, expected 1000/500 ms values and built-in LED/output/high/low symbols. It is not a C++ semantic analyzer or a proof of physical timing. For each completed job it independently checks the saved source bytes/hash, board, CLI/core identities, nonzero flash usage, exact verified compiler report and its hash. Existing native validation checks the saved HEX/ELF/source artifacts before the host returns them.

The original project sketch and all four first-job outputs (sketch, HEX, ELF, compiler report) must remain unchanged. Reopening the project must recover exactly two completed jobs. Successful tool assertions observe actual protocol tool results; assistant prose cannot substitute for a pending job or native result.

## Initial failure and routing correction

The first real run gathered the board/timing, prepared and completed the first Uno compilation, and read its verified source/compiler metadata. For the requested faster compile-only follow-up, the agent read the result but prepared an Arduino Agent saved-copy edit instead of a compiler proposal. That different proposal remained unapproved; the harness stopped rather than counting it as a second compilation.

Compiler capabilities, compiler/Agent tool descriptions and verified compiler-result guidance now identify the compiler tool for compile-only revisions. The original source/report hashes and native artifact validation remain unchanged. The shared guidance is available to every configured provider; it does not add an operation parser, provider or approval authority.

## Reproduction and limits

Build `ollama_arduino_chat`, then explicitly run `python apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program arduino`. It uses the existing cloud account and identified CLI/private AVR data under `target/`, without downloading a model, board core or library. Raw records and fixture trees remain private under ignored `target/`.

The recorder forwards bodies/status unchanged through an owned loopback port to the fixed daemon. This is a backend acceptance fixture; it does not certify direct desktop transport, native approval interaction, the installed package, other providers, broader boards/libraries or hardware behavior. Native UI automation remains paused.

## Corrected real run: passed

After rebuilding with the routing guidance, the same five-turn scenario completed two actual Uno compilations. Both proposal turns used `propose_arduino_compile`; the follow-up also successfully read the previous verified result. The native compile reports identify CLI **1.5.1**, AVR core **1.8.8** and Uno, with **924 flash bytes** for each version. The verified source contains the expected simple two-delay **1000 ms** and **500 ms** examples. Exact source/report hashes, first sketch/HEX/ELF/report preservation, original sketch preservation and two-job recovery pass.

The forwarding recorder observed **26 requests, 11 chat**, all with HTTP 200 and no recorder failures. Approval nonces and builder home paths, including paths inside nested JSON tool-result strings, are absent from requests. Raw conversations, source files, binaries and native logs remain private under ignored `target/`.

The regular Rust suite passes **141 tests, 52 ignored**, including capability/readback assertions for the new routing metadata. The later wording qualification preserves explicitly requested Arduino Agent workflows; the final real run uses that qualified guidance. The new example builds, launcher syntax/help checks pass, and desktop binaries/library plus example Clippy pass with warnings denied. An unused import in the initial example build was removed before the real run. No runtime, dependency or model was added or downloaded.

This closes one existing-cloud **Arduino CLI backend conversation** with fixture approval. It does not certify Arduino Agent editing, other providers, arbitrary code semantics, broader boards/libraries, desktop approval handoff, native UI/current installer or physical behavior. The retained installer predates this changed host guidance. The full goal remains active.
