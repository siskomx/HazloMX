# Native finalization checkpoint

October 17, 2026. No application code or selected runtimes changed.

## CADCraft: native Save As and Save pass

The explicitly selected patched CADCraft 0.7.0 runtime exposes 15 MCP tools,
including `save`. Hazlo's existing generic discovery/proposal/readback route
uses that tool without a new operation wrapper. An owned native-window test:

1. Inspects a disposable metric DXF containing one circle.
2. Adds a second circle through native `execute`; the drawing becomes dirty.
3. Runs `save` with a new DXF path. The path changes and dirty becomes false.
4. Adds a third circle, then runs `save` without a path. Dirty clears again
   and the destination bytes change.
5. Independently parses the saved DXF group codes. Exactly three circles have
   centers/radii (0, 0)/10, (30, 0)/2.5 and (40, 0)/1.25 millimeters.
6. Confirms the original source hash is unchanged. Fits the saved drawing and
   observes all three circles plus the new filename in the native window.

Both owned native/helper processes stop after the check. Approvals in this run
are explicitly backend test-harness approvals, not a new real model conversation
or Hazlo approval-UI test. Earlier real chat/export evidence retains its scope.
The first independent parser assertion counted the SECTION marker as an entity;
correcting that parser and checking the already saved file passes without
repeating either edit or save. No product fix was needed.

## OrcaSlicer: project saving is not in the selected MCP catalog

The previously captured 53-tool catalog exposes `save_gcode` and `save_preset`,
but no project-save tool. The selected server source documents those distinct
outputs. Saving G-code or a preset does not save the open project and must not
be presented as doing so. Hazlo can continue live edits and native readback;
project saving requires the user to save in Orca, or a future selected runtime
that actually exposes that capability. No alternate API, keystroke workaround,
printer control, runtime installation or hardcoded project-save wrapper was added.

G-code finalization/print readiness and a new installed-runtime editing test are
not checked here. Claude sign-in and KiCad 10 live schematic editing stay pending.
