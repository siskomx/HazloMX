# Plain-language drawing measurements

The CADCraft form now asks for the circle's full diameter and explains horizontal/vertical center positions relative to the drawing origin. Initial size and position fields are empty: selecting circle editing cannot silently accept the former radius-10/default-origin example. The guided response asks to inspect the drawing when measurements are unknown.

Circle measurements have their own millimeter/inch selector. Changing it preserves entered physical size and position. Source units remain a separate interpretation of the original drawing, subject to the existing native unit checks. The form converts circle coordinates and half the diameter into the original drawing's units for the existing typed radius request; backend editing, approval and native result validation are unchanged.

Validation on October 12, 2026:

- `rtk npm run build` passes TypeScript and production Vite compilation.
- `rtk node tests/design-flow.mjs` passes all nine workflow browser fixtures, conversation recovery and client controls.
- Drawing assertions cover empty unconfirmed measurements, missing-input and zero-diameter refusals, mm/in/mm physical preservation, mixed-unit request conversion (50.8 mm horizontal position and 25.4 mm diameter become center `[2,0]` and radius `0.5` in an inch drawing), diameter review text and approval-time disabled inputs.
- The generated full-page drawing screenshot was visually inspected; the circle fieldset now stacks its controls without overlap.

This is frontend/browser-fixture evidence. It does not prove native desktop interaction, a signed-in natural-language turn, or an updated installer. Existing native drawing execution/readback evidence retains its own scope. Live unsaved-document editing and broader drawing fidelity remain required.
