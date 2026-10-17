# Explicit sizes before native primitive creation

The live FreeCAD proposal path previously accepted `create_object` for a box without dimensional properties. Native object creation could consequently use FreeCAD's built-in sizes even though no size appeared in the approved payload. A workflow regression reproduced the incomplete proposal reaching human approval.

The shared live input now has an effect-specific validation method, called both before proposing a native job and before dispatching it to FreeCAD. New solid primitives require explicit essential sizes in `obj_properties`:

| Native primitive | Required properties |
| --- | --- |
| Box | Length, Width, Height |
| Cylinder | Radius, Height |
| Cone | Radius1, Radius2, Height |
| Sphere | Radius |
| Torus | Radius1, Radius2 |
| Wedge | Xmin, Ymin, Zmin, X2min, Z2min, Xmax, Ymax, Zmax, X2max, Z2max |

Missing, null, Boolean, blank, array or object values do not count as an explicit size. Numeric native values and nonblank native quantity strings retain their existing meaning; this check does not add another quantity/expression parser. Native validation and rollback still handle invalid geometry. Zero remains allowed where it has a native meaning, such as a cone's pointed end or a wedge's face origin. Containers, empty sketches and spreadsheets do not acquire fictitious dimension requirements. Partial edits of existing geometry continue to preserve omitted properties.

The shared tool description tells Claude, Codex and Ollama to gather missing measurements in plain language and translate diameters internally, rather than relying on native size defaults. It lists the internal dimensional properties for the agent. No measurement form, provider, tool or job schema is added.

General historical-input and result validation remain unchanged. The stricter policy runs at proposal/execution boundaries so older verified receipts remain readable. Adding a receipt digest cannot bypass the effect-specific check. This does not establish that a model obtained meaningful user confirmation merely because it supplied every property: the person must still review the actual sizes before approval.

Validation:

- The before-fix workflow regression fails because the incomplete box reaches approval. After the fix it refuses without a current job or recovered history entry.
- The six-primitive matrix accepts explicit metric/imperial quantities and numeric values, then removes or substitutes each required property to verify refusal. It also checks historical validation, digest non-bypass, dimensionless containers and partial edits.
- All **180 regular desktop library tests pass**, with **54 environment-dependent tests ignored**, run serially.
- Library Clippy passes with warnings denied. Source-derived approval-text checks pass after completing the cylinder-creation fixture with an explicit height.

An initial test-cleanup failure attempted to reopen a project while its workflow still owned the writer lock. Dropping that owner before reopening fixes the fixture without changing project locking behavior. Earlier failure output remains private.

No native creation UI/model sequence or new installer is claimed by these checks. Existing native edit/save and package evidence retain their original scope. CADCraft's isolated source-build decision and Claude model testing remain pending separately; the full application goal stays active.
