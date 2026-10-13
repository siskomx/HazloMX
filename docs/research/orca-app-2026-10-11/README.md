# OrcaSlicer application workflow

October 11, 2026. App source and real host backend tests pass against the identified official Windows x64 OrcaSlicer 2.4.2 package. This does not certify the native desktop workflow or a new installer.

The Spanish conversation collects model intent, original STL units, machine/nozzle, material, bed surface and initial bed temperature. Explicit fields and native file pickers bind the request to a selected STL, a profile folder and machine/process/filament JSON files. The preview shows normalized millimetre dimensions, triangle count, profile names, nozzle/layer sizes, first-layer temperatures and captured sources before one-use approval. No AI model is needed.

The backend captures originals and same-category profile inheritance, rejects escaping paths, cycles, incompatible profile identities, unsupported compatibility expressions, post-processing scripts and connection credentials, then normalizes ASCII/binary STL from mm/cm/m/in/ft to millimetres. It runs only the identified CLI under owned-process supervision with private configuration. Original inputs remain pinned during execution. Semicolons in project profile paths are rejected because the CLI interprets them as list separators.

Output checks cover bounded ZIP/XML resources, duplicate/escaping entries, model units, vertex/triangle indices, linear transforms, dimensions, triangle count and a translation-independent triangle geometry signature at 0.001 mm resolution. Effective profile identity, nozzle/layer size, filament diameter, first-layer temperatures and bed surface must match the proposal. The exported G-code must match the embedded G-code, identify the expected producer, use millimetres and report bounded layers/height. Source and output hashes are rechecked for publication and recovery; unexpected files are rejected. These checks do not establish printer calibration or independently simulate toolpaths.

Published results include captured original sources, normalized STL, `Sliced.3mf`, `Toolpath.gcode` and a literal read-only report. Existing output destinations are preserved. Artifact actions explicitly show verified files in Explorer.

## Evidence

- [Retained actual test status and source hashes](status.json), [stdout](stdout.log), [stderr](stderr.log). Reproduce with `rtk proxy python tests/orca/verify_app.py` from the repository root. Three actual tests pass serially.
- Adapter: stock Prusa MK3S 0.4 nozzle / 0.20mm Standard / Prusa Generic PLA / Textured PEI Plate / explicitly confirmed initial bed 45 °C. Millimetre and inch-input versions produce the same approximately 10 mm dimensions and fifty layers; original bytes remain unchanged. Tampered standalone G-code is rejected. The millimetre test stage is intentionally tampered afterward; its files are not clean publication evidence.
- Shared workflow: denial and stale/used approval rejection, all four public artifact roles, nonreplacing publication with a preserved occupied destination, verified staging after publication failure, cancellation before dispatch, prior output preservation and four-run recovery.
- [Live owned-process cancellation](cancel-evidence.json): observe a living direct owned Orca process, cancel, independently observe its cessation, publish nothing, preserve the original and recover Cancelled. This proves supervisor cancellation while the process is alive; it does not prove the slicer had parsed the model before cancellation or native UI latency.
- Regular suite: 66 passed, 20 ignored entries. Desktop/custom-protocol Clippy, frontend build and six-workflow browser fixtures pass. [Browser-fixture screenshot](orca-workflow.png) was visually inspected; this is mocked IPC, not native acceptance.
- Regression archive checks reject changed units, machine identity, G-code mode and changed triangle geometry with unchanged dimensions/count; ASCII/binary equivalence covers all five units and malformed facets.

## Remaining scope

Only one STL object and one extruder are supported. Profile folders must contain `machine`, `process` and `filament` subfolders; each inheritance parent must be in the same category. Arbitrary compatibility expressions, broader machine/material catalogs, multi-material layouts, other model input formats and full 3MF relationship semantics remain unverified. Stock profile resources are not individually pinned as part of the runtime identity; selected profile bytes and inheritance are captured into the approved proposal.

Native pickers/approval/results/restart/Cancel, native artifact loading, second-environment checks, production-deadline expiry and installer lifecycle remain unfinished. Cool/smooth bed selections exist in source but have not passed the actual stock-combination gate. The certified installer predates this workflow. No printer connection or hardware operation is implemented or exercised.
