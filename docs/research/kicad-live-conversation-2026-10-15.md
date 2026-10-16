# KiCad native conversation gate — October 15, 2026

The opt-in `ollama_pcb_live_chat` example exercises the existing cloud route against an actual owned KiCad PCB editor through the production native reader, proposal tool and approval/job pipeline. The five-turn gate is **prepared, not passed**. No production editing capability or provider is added by this acceptance change.

## Required scenario

The example opens a fresh copy of the public PCB fixture in the pinned owned editor, using its already configured test profile. The person asks to identify a resistor without knowing its reference or coordinates. The model must inspect and clarify, then resolve “the topmost 10k resistor” and prepare a two-millimetre move to the right. After fixture approval, it must read the verified receipt and current native state. A second request moves the same resistor one tenth of an inch down from its current position, with separate approval and final readback.

Assertions check the exact instance/document/whole-board state binding, unchanged state before approval, correct relative metric/imperial positions, preserved saved bytes and first receipt, and two completed schema-14 jobs after recovery. Saved-copy operations cannot satisfy the gate. The shared recorder checks program-scoped tools and excludes approval nonces and builder paths, including nested JSON strings, from provider requests.

The editor runs in an owned process tree with desktop-lock, 1 GB physical/commit memory and bounded-lifetime monitors. Cancellation is signalled before writing monitor diagnostics, so a logging failure cannot prevent cleanup. The native worker uses the existing bounded whole-board verifier through the example executable's internal validation entry point. Security/privacy dialogs and profile preferences are not automated or changed. Raw traffic, native state, profiles and failures remain under ignored `target/`.

## Attempts and current evidence

The first real conversation inspected the board, recovered from an invalid component identifier, then lost its native connection before a successful proposal. The owned editor had stopped; that initial monitor did not record the stop cause, so memory is not asserted as its cause.

The next attempt prepared the correct native proposal, including its UUID, original position and requested two-millimetre movement. The unchanged-state assertion passed before fixture approval. Execution then failed because this new example omitted the existing internal verifier entry point: the worker invokes the current executable with `--validate-pcb-move` before starting its native commit. The example now dispatches both internal verifier modes through the same production helper used by the desktop tools executable. The worker's validation and editing behavior are unchanged.

The corrected executable's next owned run stopped at the memory floor before the editor became ready and before any provider request. Its diagnostic records `memory_floor`, and its process cleanup records successful cancellation. Gradle/Kotlin daemons had returned after their authorized shutdown. Further GUI runs await sufficient memory; this is not a passing conversation or editing result.

The example builds, passes Clippy with warnings denied and passes formatting checks. Six malformed-input subprocess checks exercise both verifier modes and confirm their fixed negative response and failure status. The Python launcher parses and its help path includes `pcb-live`. Production source/frontend behavior is unchanged; their existing suites were not repeated for this acceptance-only addition.

## Reproduction and limits

Build the desktop-feature `ollama_pcb_live_chat` example. Set `HAZLOMEXA_REAL_PCB_ROOT` to the existing configured owned KiCad fixture root, then run `apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program pcb-live`. The example validates the owned root and pinned editor binary, creates a fresh board and project for each attempt, and stops its owned editor on completion or failure. Never retry an uncertain native write against the same document.

Approvals in this gate are backend fixtures. A future passing result would establish only this real-provider/native-PCB scenario; it would not establish Hazlo's native approval UI, ordinary-user installation, other providers, saving, routing, schematic editing or current installer acceptance. The full application goal remains active.
