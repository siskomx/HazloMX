# Natural idea to actual native FreeCAD creation

A fresh owned Hazlo/FreeCAD run passes the real existing Ollama Cloud conversation through software choice, unknown-size clarification, native creation, UI approval, readback and a separately approved revision. It also exposes and fixes a native-view framing gap. The two sets of evidence below retain different scopes.

## Actual chat and UI sequence

The bundled debug app built from `13785dd` starts in its normal software-choice mode. An ordinary Spanish idea asks for a rectangular support without knowing the software or dimensions. The real model offers FreeCAD for the solid and OrcaSlicer for later print preparation, and asks which purpose to pursue. Selecting FreeCAD through the existing chat selector preserves the conversation.

The person describes an already open empty document and still supplies no dimensions. The model reads the native document, confirms it is empty and asks about length, width, height and intended shape. No proposal or geometry appears before measurements are supplied. The later natural reply specifies half an inch of length, two centimetres of width and ten millimetres of height, with no internal object name or tool format.

The model prepares a native box proposal containing all three explicit sizes. The actual Hazlo review shows those sizes and waits for its approval button. Independent native inspection still sees the empty document. Clicking the button creates the box in the same native document: **12.7 × 20 × 10 mm**. The automatic real-model result review confirms native geometry and the unsaved state.

In the same conversation, “make the support one inch tall in total, keeping length and width” produces another review and waits for another actual UI approval. Independent native inspection preserves the first shape until that click. After approval, native dimensions are **12.7 × 20 × 25.4 mm**, with no extra objects or saved file. The automatic model review confirms the new height and retained dimensions.

Eight separate native reads agree on one document-lifetime token. Both completed schema-13 jobs and their receipt hashes verify; no FCStd file exists. The owned supervisor stops and joins both apps after the run. Sources, conversations, observations, profiles and logs remain private under ignored `target/`.

This run uses actual Windows input and the existing cloud connection, not browser IPC fixtures or backend approvals. It does not retain an exact model tool-call trace. History restart, saving/exporting this new part, other providers and installed creation are not exercised. The result explanations still expose unnecessary internal metadata; a fully concise natural-language review is not claimed.

## Framing failure and source correction

The first created object appears in FreeCAD, but the camera fitted to the formerly empty document is much too close: only a corner is visible. Native **View → Standard Views → Fit All** makes the complete support visible. Raising the height later also extends beyond that view; the same native Fit All action exposes the complete revised solid. Those manual camera actions are part of the preceding UI evidence, not proof of automatic framing.

The fixed native glue now calls FreeCAD's own `fitAll()` before redraw after a successful geometry transaction. It fits the complete model and therefore adjusts the current camera after remote geometry changes. It does not add a Hazlo viewport, editing control or configuration option. Native saving/exporting return through their existing separate paths; failures retain their rollback behavior.

The focused regression first fails with only redraw observed, then passes with fit-before-redraw. All **14 fixed-glue tests pass**, including assertions that save, STEP export and rolled-back failures do not fit the view. Library/example Clippy passes with warnings denied.

A rebuilt `owned_cadpilot_creation` example then passes a fresh actual native run with the corrected embedded glue: missing-size refusal, creation, inch revision, matching readback/receipts, two-job recovery, valid shape/volume and two native Undo steps back to empty. A separate GUI-thread check projects the final box corners into camera coordinates and verifies that the native orthographic camera contains their vertical extent. The first camera-check attempt needed the bundled `pivy.coin` import to register its native wrapper; after that helper correction the check passes. No model file is saved and all owned processes stop.

The fresh post-fix run uses backend fixture approvals and a native camera check. It does **not** repeat the real chat/UI sequence, establish horizontal/occlusion coverage through that camera assertion, or verify an updated installer. The earlier real chat run remains scoped to its manual Fit All actions. The full application goal remains active; Claude testing and the isolated CADCraft source-build decision stay pending separately.

## Fresh actual UI creation, revision and native save

A later debug build includes `14c774e` and an uncommitted shared result-language instruction. The existing Ollama Cloud conversation creates a support from half-inch, two-centimetre and ten-millimetre dimensions, then raises it to one inch through a separately reviewed native edit. Both actual Hazlo approval buttons are used. Unobstructed native screenshots show the complete created and revised solid automatically, without manual Fit All or camera input.

An explicit request to save produces a third, separate approval. FreeCAD opens its own Save As picker; a new owned destination is chosen there. Eight independent native reads confirm one document lifetime, no objects before creation approval, retained dimensions before follow-up approval, and final dimensions of **12.7 × 20 × 25.4 mm**. The native document reports a saved file and no pending modifications. The FCStd archive's XML agrees on all three dimensions, and its file hash matches the save receipt. All three completed schema-13 receipt hashes verify. Both owned apps stop and join after verification. This check inspects saved XML; it does not independently reopen the file in FreeCAD.

The shared wording alone does not pass concise-language acceptance: the follow-up reply still exposes an internal saved-state key, and the save explanation remains verbose. The automatic completed-operation review request is therefore tightened to ask for two to four ordinary-language sentences about the change, relevant measurements and saved/exported/unsaved state, omitting internal identifiers and only mentioning relevant limits. Existing readback, approval and no-additional-write requirements remain. A fresh real-model check of this final request remains pending; browser request assertions verify its dispatch rather than model obedience. Raw observations and conversations stay in ignored `target/`.

The full browser chat-flow suite passes, including the new review-request assertions across the four provider routes and existing approval, duplicate/stale-event and uncertain-outcome checks. Bounded-context checks and the TypeScript/production frontend build also pass. Focused shared-provider Rust tests pass: Claude 3, Codex 10 and Ollama 18, with two environment-dependent tests ignored across those suites. Claude's authenticated conversation test remains pending at the person's explicit request.

Follow-up: the [native-coordinate regression and fresh actual conversation](native-creation-coordinates-2026-10-16.md) now exercise the final completed-review wording after both creation and revision. Both result replies are concise in that actual provider run; this does not extend to all providers or guarantee future model behavior.
