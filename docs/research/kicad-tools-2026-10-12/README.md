# KiCad reviewed checks/exports from verified saved results

The fifteen-tool source catalog adds `propose_kicad_check`. It accepts a selected project, completed KiCad/Konnect schematic-or-PCB job, verified report hash and explicit coordinate units (`mm`, `in`, `mils`). Agents must read the result first, explain genuine electrical findings in plain language and clarify the intended check/report units. Native KiCad and Konnect PCB result reads also return a structured `checks` summary. Electrical errors are findings in a completed check, not a failed execution or proof they were fixed.

Hazlo resolves the actual saved source and its companion/dependency files from the validated publication. It binds every source file's bytes/hash, captures another copy, rejects project/runtime/source changes during capture, and requires the existing one-use human approval before native ERC/DRC and exports. The saved file determines schematic versus PCB behavior. Report units do not rescale physical geometry; PCB checking can refill zones and save the checked copy. No arbitrary paths, native commands, open-document changes or hardware effects are exposed.

Three actual native gates pass with KiCad 10.0.7 and Konnect 0.13.0:

- A completed inch-coordinate KiCad schematic result becomes a reviewed mil-coordinate check/export. Its two genuine ERC errors remain visible; report units change, original and previous published files remain intact, and five runs recover. [Calls](schematic/calls.json).
- A Konnect-edited sheet becomes a reviewed KiCad ERC/export. The native checked sheet retains R1's actual edited value `12.34k`; original and previous output bytes remain unchanged, and five runs recover. [Calls](edited-schematic/calls.json).
- The real release-helper/SDK executes two Konnect PCB moves, reads the final saved board, then proposes and completes a mil-coordinate KiCad check/export of that exact board. The captured board hash matches the software-saved output; three completed MCP jobs recover with schema 12 for moves and schema 10 for checks. Genuine DRC findings remain visible. [Calls](pcb/calls.json).

Completed artifacts and snapshots for all three paths, [status](status.json) and [file hashes](file-hashes.json) retain the proof. Stale report hashes, injected paths, missing/foreign results and cancellation are refused without an executable approval. The same manual KiCad workflow keeps schema 6 and its existing copy/check/export behavior.

110 regular Rust tests pass, including fifteen-tool local SDK discovery/revocation and no-effect refusals. Formatting, desktop/all-target Clippy, production frontend, desktop/helper builds and all nine browser workflow fixtures pass. The actual release-helper fixture discovers all fifteen tools without engineering jobs. Installed Codex recognizes that catalog and resumes its restricted conversation against a credential-free loopback fixture ([status](codex-status.json), [tool surface](codex-tool-surface.json)); no cloud inference is run.

Signed-in natural-language checking, native UI/installed tool acceptance, broader resource fidelity and live open-document editing remain unfinished. The earlier installer proof is bound to its thirteen-tool build; a fifteen-tool package still needs acceptance. The full application goal stays active.
