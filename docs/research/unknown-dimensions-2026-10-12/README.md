# Unknown dimensions remain unknown

The FreeCAD manual form now starts with empty length, width and height fields. It no longer supplies the former 10 × 20 × 30 example as an implicit build input. A partial draft shows which dimensions still need confirmation; the proportional preview stays hidden until all three dimensions are positive. Preparing a proposal requires all three measurements within the existing verified 1–200 mm range.

Changing units converts only entered measurements and leaves unknown fields empty. A 1-inch length becomes 2.54 cm without turning an unknown width or height into zero. The submit handler also rejects missing/out-of-range dimensions before dispatch, including when invoked directly instead of through browser form validation. Backend validation and explicit engineering approval remain unchanged.

Changing the selected project ID or folder clears the unsaved manual draft. This also covers copied projects with an identical ID. A new authoritative job snapshot can still fill its actual proposed dimensions through the existing measured/proposal path; conversation text alone does not restore draft measurements.

Validation:

- `rtk npm run build` passes TypeScript and production compilation.
- `rtk node tests/design-flow.mjs` passes all nine workflow browser fixtures, scoped conversation recovery and client controls.
- New checks cover empty initial fields, empty-field unit switching, partial-draft physical preservation, hidden preview, missing-input refusal before native dispatch, inch inputs above/below the existing range, confirmed proposal behavior, project switches and same-ID folder isolation.
- The generated [partial-draft screenshot](unknown-dimensions.png) was visually inspected. It includes the explicit submission-refusal test state; ordinary incomplete inputs keep preparation disabled.

These are frontend/browser-fixture checks, not signed-in natural-language or native desktop acceptance. The latest tested `d8c54f69…` installer predates this FreeCAD draft change. An updated package and native unknown-size conversation remain required. Existing native engineering evidence keeps its original scope; the full application goal remains active.

Later [native debug-app evidence](../native-unknown-dimensions-2026-10-12/README.md) separately passes empty/partial inputs, keyboard approval, actual FreeCAD output/recovery and observed cancellation. It does not establish signed-in conversation, native pickers or installed-release acceptance.
