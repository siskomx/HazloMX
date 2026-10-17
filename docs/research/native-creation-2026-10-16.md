# Actual native creation from an empty FreeCAD document

The `owned_cadpilot_creation` example passes the production shared-tool, approval, native execution, receipt and recovery path against an actual owned FreeCAD 1.1.4/CADPilot session. The initial document is empty, unsaved and configured for native Undo. This follows the [explicit creation sizes fix](explicit-native-creation-sizes-2026-10-16.md); it does not introduce another editing path.

The actual sequence establishes:

1. The shared native reader sees the named empty document. A box proposal with no dimensions is refused with no job. Fresh native state remains unchanged.
2. An explicit box proposal requests length `0.5 in`, width `20 mm` and height `10 mm`. Preparing it leaves the document unchanged. A backend fixture supplies one approval; reuse of that approval is refused.
3. Actual native execution completes and independent shared-tool readback measures 12.7 × 20 × 10 mm. The persisted verified receipt agrees with the fresh native state. Reusing the stale proposal is refused.
4. A separate proposal and approval change only the height to `1 in`. Fresh native readback measures 12.7 × 20 × 25.4 mm. Both verified receipt files remain readable and unchanged.
5. Reopening the Hazlo project recovers exactly two completed schema-13 jobs.
6. A separate fixed native check dispatched to FreeCAD's GUI thread confirms all three actual property values, a valid shape and volume `12.7 × 20 × 25.4` cubic millimetres. One native Undo restores height 10 mm; a second removes the created object and restores the empty document. No model file is saved.

The launcher verifies the existing runtime identities, isolates native settings/data/temp files, refuses other FreeCAD processes or an occupied RPC port, and applies the existing unlocked-desktop and memory prerequisites. All owned host/editor processes are stopped and joined, including failed attempts. Native protocol output, profiles, source observations and logs stay under ignored `target/`.

The first helper run expected `document_name` in the public observation, whose actual field is `document`; correcting that test assertion permits the complete production host sequence. The first separate Undo check used CADPilot's background execution thread directly and timed out. Progress markers localized the timeout to Undo of object creation. Dispatching the fixed independent check through the native GUI-thread dispatcher, as production Hazlo already does, resolves it. Earlier failed attempts remain private and are not counted as passed acceptance. Production editing code is unchanged by these helper corrections.

The final example build and native execution pass. Clippy with warnings denied also passes. The [preceding production change](explicit-native-creation-sizes-2026-10-16.md) already passed all 180 regular library tests; that suite is not repeated for this acceptance-only example.

To reproduce, build `owned_cadpilot_creation`, provision a fresh owned directory directly under `target/` named `native-creation-<UUID>`, and launch the pinned native fixture with an empty `OwnedCreationFixture`, native Undo enabled, isolated settings and CADPilot RPC on the verified unused loopback port 9875. Its `native-ready.json` must identify that owned document and its `project` directory must exist. Set `HAZLOMEXA_NATIVE_CREATION_ROOT` to that directory and run the example. Use a supervised fixture; do not connect this example to a user's editor. The independent native geometry/Undo check is separately scoped above.

This proves **real native execution with backend fixture approvals**. It does not prove a model asking for unknown dimensions, actual Hazlo UI approval of creation, visible new geometry, native saving/export of this created part, installed creation or other provider acceptance. Those requirements remain open. Claude testing and the proposed isolated CADCraft source build remain pending separately; the full application goal stays active.
