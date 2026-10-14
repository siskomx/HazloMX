# Real OrcaSlicer file conversation — October 14, 2026

This acceptance gate uses the existing configured cloud route, production host tools and the identified OrcaSlicer 2.4.2 CLI. A fresh copy of the public inch-based STL is selected with the existing Prusa MK3S 0.4 nozzle, 0.20 mm Standard process and Prusa Generic PLA profiles. Native UI automation remains paused. There is no printer connection or control.

## Scenario and verification

The five-turn scenario requires native source/profile inspection, clarification of missing STL units and bed settings, an actual pending first proposal, native generation after backend fixture approval, verified-result explanation, a temperature-only follow-up proposal, separate fixture approval and final readback. The user explicitly confirms source coordinates in inches, a smooth bed and **45 °C** initial bed temperature, then requests **50 °C** while preserving everything else.

Proposal assertions compare the full approved printer/process/material settings with the selected native profile values, permitting only the requested temperature override. Converted model bounds must exactly match the binary STL's stored 32-bit millimeter coordinates. Follow-up mesh bounds/signature and all other settings must remain identical. Actual 3MF/G-code readback must match the approved settings, retain the converted geometry and have nonzero layers. Native artifact validation runs before host readback.

Original STL bytes, all 714 files in the selected Prusa profile tree and the first 3MF/G-code/report outputs must remain unchanged. Project recovery must contain exactly two completed jobs. The shared recorder independently observes successful real host tool results and checks approval nonces and builder home paths are absent from provider requests, including paths nested inside JSON strings.

## Fixture corrections

The first attempt stopped before any provider request because the harness expected a textured-bed profile that the selected material inheritance does not advertise. Its actual inherited initial temperatures cover cool and smooth beds. The fixture now uses the supported smooth-bed case with an explicit user-requested temperature; the application does not invent the missing profile settings.

The next attempt reached the first pending proposal but stopped before approval because the harness compared normalized binary STL bounds with unrounded doubles. The adapter correctly stores converted coordinates as 32-bit floats. The corrected assertion compares the exact stored float representation instead of introducing a broader tolerance. No application parser, unit conversion, native validation or approval rule was changed for either fixture correction.

A later conversation correctly asked whether the fixture's vague “smooth bed” meant Cool Plate or High Temp Plate. The test had expected a proposal too soon. Its explicit answer now selects the described High Temp Plate, whose observed profile temperature is 60 °C, and requests 45 °C for the first layer. That clarification is valid conversation behavior, not an application failure. The production approval summary now also displays the actual selected native bed name; previously it showed temperatures without that choice, leaving it only in technical details.

The first completed native slice then exposed another overly strict harness assertion: persisted JSON metadata can have the already documented one-writer/parser floating-point representation difference. Readback now compares the exact JSON round-trip representation; proposed binary STL bounds still require exact stored-float equality, and the cryptographic mesh signature must match. This reuses the application's established storage contract rather than relaxing native geometry validation.

## Reproduction and limits

Build `ollama_slice_chat`, then explicitly run `python apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program slice`. The identified existing Orca runtime and its selected Prusa profiles are prerequisites. No model, runtime, profile or provider is automatically downloaded. All source copies, raw conversation/request records and native logs stay private under ignored `target/`.

This is a backend saved-file workflow with fixture approvals and a fixed-daemon body-forwarding recorder. It does not prove direct desktop transport, native approval/pickers/views, current installed or ordinary-user acceptance, other providers, broader profiles, live GUI editing or physical print quality. Printer control remains excluded.

## Final real run: passed

All five turns and two actual native slicing jobs passed. Both generated **50 layers**. The first uses the explicitly confirmed inch STL and High Temp Plate at **45 °C**; the result-derived follow-up changes the initial bed temperature to **50 °C**. Actual readback retains the approved printer, process, material, nozzle and geometry. The model uses initial source inspection/proposal tools, then `read_verified_result` and the hash-bound `propose_slice_settings` follow-up tool.

Original STL bytes, hashes for all **714 selected Prusa profile-tree files**, all three first outputs and two-job recovery pass. The recorder observed **29 requests, including 14 chat requests**, all with HTTP 200 and no recorder failures. Approval nonces and builder home paths, including paths embedded in nested JSON tool-result strings, are absent from provider requests. Raw receipts remain private.

The production TypeScript/Vite build and focused approval-fact checks pass. The full headless browser chat gate passes all four AI choices, six program groups and **36 explicit approval/result-review cases**, including the selected native bed label and existing failure/cancellation/history checks. The acceptance example builds and passes Clippy with warnings denied; launcher syntax/help checks pass. Rust production behavior did not change, so the 141-test backend suite was not rerun solely for this presentation/acceptance change. No model, provider, runtime or dependency was added.

This closes one existing-cloud **Orca saved-file backend conversation** gate and the missing selected-bed approval fact. Native/installed approval interaction, ordinary-user/current-installer acceptance, other providers and broader profiles remain unfinished. The retained installer predates the new UI fact and must be rebuilt for current-source acceptance. The full goal stays active.
