# Natural choices for ambiguous targets

Hazlo's shared agent instruction now asks people to choose ambiguous parts
using recognizable differences supported by inspection. It resolves native
references internally, asks for a description or an indication in the program
when inspection cannot distinguish candidates, and waits before preparing
the change. The same instruction is used by Claude Code, Codex and both
existing Ollama chat routes. No parser, selector form or new tool is added.

## Real native conversation

An owned KiCad 10.0.7 board contains three 10k resistors. In actual Hazlo
chat through the existing configured Ollama cloud route, request:

> Quiero mover la resistencia de 10k dos milímetros a la derecha.

Before the change, the agent correctly asks which resistor and leaves the
board untouched, but offers references and raw coordinates as the choices.
After changing only the shared instruction paragraph and rebuilding the
desktop, the same request offers top-left, middle and bottom-right choices.
References appear as supplementary labels; the person does not need them.
There is no proposal or job while the clarification is unanswered.

Reply:

> La de más arriba. Déjala sin guardar.

The agent resolves the topmost matching resistor internally and prepares a
2 mm move right. Independent native inspection still matches the initial
state before the actual Hazlo approval button is used. After approval,
KiCad visibly shows the changed component. Its verified commit receipt and
independent live read agree on `(197, 141)` to `(199, 141)` mm. The remaining
properties of that component and all other component projections are intact.
Original saved PCB bytes remain identical. Automatic result review describes
the confirmed unsaved native result and the lack of automatic track rerouting.

The [sanitized receipt](natural-target-clarification-2026-10-16/receipt.json)
binds the tested executable and source. Four independent native reads cover
initial state, the unanswered question, pending approval and the completed
move. There is one completed schema-14 job. Both owned applications are
stopped afterward; raw conversations, board paths and profiles remain private
under ignored `target/`.

## Verification and limits

Scoped Rust checks pass: three Claude tests, ten Codex tests and eighteen
Ollama tests, with two existing ignored native/model cases. Library Clippy
with warnings denied and the custom-protocol desktop build pass. Two early
test-filter invocations matched zero cases and are not counted as validation.

This is one observed real-model conversation, not a deterministic guarantee
of every agent response. Real Claude and Codex model acceptance remains
unverified; the person explicitly keeps Claude's sign-in test pending.
The current installer predates this instruction change. No manufacturing,
hardware, in-flight cancellation or broader integration claim is added.
