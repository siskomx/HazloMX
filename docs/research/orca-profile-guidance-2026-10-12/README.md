# Orca profile compatibility metadata is validated

The shared profile-settings check now rejects malformed `compatible_printers` fields, non-string/empty/control-character printer names, and non-string compatibility conditions in both process and material profiles. A malformed field no longer acts as a missing restriction. Existing valid matching printer lists and empty unrestricted lists remain accepted; nonempty compatibility expressions remain unsupported rather than evaluated by Hazlo.

Spanish errors identify the selected printer and whether the process or material needs correcting, asking for a compatible profile for that printer/nozzle. The same check runs during initial capture and verification of captured profiles/results; it does not reinterpret printer compatibility or modify the original profiles. The desktop already renders these errors as text.

Validation passes: 112 regular Rust tests, including both profile kinds and malformed/null/string/list/condition cases, positive matching/unrestricted lists, and the existing capture/source/inheritance/script refusal cases. The new fixture was corrected to include the explicitly chosen bed temperature normally inserted by production capture. Formatting and all-target desktop Clippy pass.

The actual identified OrcaSlicer 2.4.2 native gate passes with the existing Prusa MK3S 0.4 nozzle / 0.20 mm Standard / Generic PLA profiles: metric and inch STL inputs produce the measured 10 mm cube with 50 layers, verified 3MF/G-code/report outputs, preserved original model bytes and tamper refusal. `native-inch/` retains the untampered inch run; the metric run's test intentionally overwrites its G-code for tamper rejection and is not presented as an intact output.

This is backend/native adapter evidence. Profile discovery through natural conversation, native picker/GUI approval, signed-in agent interaction, broader printer/material combinations and an installer containing these changes remain unfinished. The current `0cdc30ad…` installer predates this change. No hardware control or AI inference runs; the full goal stays active.
