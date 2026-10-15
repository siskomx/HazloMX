# Circular measurements in the chat review

The native FreeCAD review no longer requires the person to understand `Radius1` and `Radius2`. It uses the inspected object type to describe the cone's base and opposite end, a cylinder's base circle, a complete sphere, and the separate ring/tube measurements of a torus. A diameter is explained once as the full width through a circle's centre.

For recognized primitives, numeric millimetre radii and simple `mm`, `cm`, `m`, `in` or `ft` quantities are shown as full reference-circle diameters. Each quantity keeps its own unit. A torus's first radius remains the distance from the ring centre to the tube centre; it is not presented as the outside width of the ring. The second radius can be shown as the diameter of the circle forming its tube. These describe the primitive's defining geometry, not its rotated or cut bounding dimensions.

The implementation follows the [FreeCAD 1.1.4 primitive definitions](https://github.com/FreeCAD/FreeCAD/blob/1.1.4/src/Mod/Part/App/PrimitiveFeature.cpp) and the official [cone](https://github.com/FreeCAD/FreeCAD-documentation/blob/main/wiki/Part_Cone.md) and [torus](https://github.com/FreeCAD/FreeCAD-documentation/blob/main/wiki/Part_Torus.md) descriptions. Unknown expressions, unsupported units or incomplete scalar interpretation retain their literal value with a centre-to-edge description; Hazlo does not evaluate or guess their diameter. Existing object labels remain preferred to internal identifiers.

This is a read-only presentation change. The reviewed and approved native payload still contains its original radius values and unit strings. No geometry, approval identity, connector or software editor changes. The shared Claude/Codex/Ollama guidance now asks for physical sizes and translates property names internally rather than asking the person to calculate radii. It still requires inspection and clarification of missing information.

## Verification and limits

The production TypeScript/Vite build, proposal-fact checks, full browser suite and desktop library Clippy pass. All 171 regular desktop Rust tests pass, with 55 integration tests ignored. New cases cover cone/sphere/cylinder/torus distinctions, all five units, scientific quantities, unknown-expression fallback, inherited-property refusal and unchanged native payloads. The browser adds a cone review/approval check for each of the four AI choices and verifies that half-inch native radii remain unchanged after the full-diameter display.

The browser uses IPC fixtures; it does not establish real-model terminology, new native geometry execution or current installer acceptance. Windows still reports a locked desktop, so native UI work stays stopped with the existing unlock request pending. The full application goal remains active, including the unfinished integrations and native conversation gates.
