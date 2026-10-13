# CADPilot STEP readback checks each solid

The selected-copy macro already checked every approved native dimension/position before saving and again after reopening the FCStd. The STEP export previously checked validity, solid count and total volume. Its additional per-solid check now compares actual exported volume, world bounds and center of mass against each edited native object's global shape, independent of export order. Every expected solid must be matched once; duplicate and omitted solids cannot reuse an earlier match.

The check retains the existing volume tolerance (max 0.01 mm³ or 0.002%) and uses 0.000001 mm tolerance for bounds and center of mass. It uses raw native shapes rather than the upstream six-significant-digit report. Invalid/nonfinite measurements refuse publication. The desktop gives a specific Spanish STEP mismatch message and preserves the original and private working copy.

Validation passes:

- Four Python policy tests, including reordered/identical solids, accepted tiny numeric noise, equal-volume misplaced or dimension-swapped solids, duplicate/missing solids, nonfinite coordinates and changed centers of mass.
- Actual identified FreeCAD 1.1.4 / CADPilot 0.7.7 selected-copy adapter: box editing and inch cylinder addition, real STEP reads, original preservation and tamper refusal. `cylinder/` retains the untampered native cylinder output and report.
- Actual shared-backend publication/recovery gate: inspection, inch box edit, centimeter box addition, millimeter cylinder addition, one-use approval, cancellation, occupied publication destination, preservation of earlier outputs and seven-run recovery. `project/` retains that real project, including four completed, two cancelled and one failed publication case.
- 111 regular Rust tests, the specific new rejection-message assertion, formatting and all-target desktop Clippy.

This checks the listed measured properties, not complete geometric equivalence of arbitrary shapes. Broader document/shape fidelity remains unfinished. The `0cdc30ad…` installer and its earlier CADPilot connector acceptance predate this macro change; updated-package and native/signed-in conversational acceptance remain unfinished. No AI inference or hardware call runs. The full application goal stays active.
