# Understandable approval reviews — October 13, 2026

The production chat previously relied on a short title and expandable code/JSON for reviewing a change. The new read-only Spanish review derives facts from the actual pending operation snapshot. An assistant's description cannot replace the operation list. No editing fields, operation parser, approval authority or native execution behavior was added.

The review covers each current workflow:

- FreeCAD generation: final block dimensions in millimeters. Saved-document changes: named operation/object, proposed dimensions in their confirmed units, current block dimensions when native inspection supplies them, full cylinder diameter and placement.
- CADCraft: saved drawing, named units/output format, the circle's full diameter/center, or each native action in execution order. All 33 reviewed command IDs have Spanish labels. The AI's intent is explicitly labelled as its description; exact entity selections and arbitrary native parameters still require the expandable details. An accepted lossy conversion is called out.
- KiCad/Konnect: source document and actual old/new component value or absolute old/new PCB positions in millimeters. Check/export proposals distinguish report units from design scale.
- Arduino CLI: Arduino Uno by name, compilation and generated-file scope. Arduino Agent: saved sketch copy and original/proposed code comparison in details. Neither review implies firmware upload.
- OrcaSlicer: source coordinate units, actual interpreted millimeter size, selected machine/filament/process, nozzle/layer dimensions and temperatures. The review states that it generates files without sending them to a printer.

All workflows identify preservation of original files and show the audited relative result folder when available. Facts and source-derived names render through DOM `textContent`, including potentially hostile model intent. Technical details remain expandable for exact parameters and code. Completed history and stored conversations are unchanged.

## Evidence and limits

`rtk proxy node apps/desktop/tests/proposal-review.mjs` passes operation-specific facts, five CAD input unit names, three KiCad report unit names, unchanged user-unit values, radius-to-diameter presentation, current/native versus requested block measurements, absolute PCB positions, interpreted STL bounds, all 33 drawing labels, unknown-command/prototype handling and relative destination checks. This is presentation coverage; host validation remains authoritative.

`rtk proxy node apps/desktop/tests/chat-flow.mjs` passes the production page with mocked IPC for all four AI choices, six program groups and 36 explicit approval/result-review handoffs. It checks escaped native intent, visible source-derived facts, no editing fields or automatic approval, and existing cancellation/failure/history boundaries. TypeScript and the production Vite build pass. No backend behavior changed, so native engineering jobs were not rerun for this presentation change.

Native WebView interaction, current installed review acceptance and live editing remain unverified. The retained installer predates this shipping-source change and must be rebuilt for current acceptance. This improves the review step; it does not close the full cross-program orchestration goal.
