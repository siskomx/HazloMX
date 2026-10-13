# CADPilot selected-copy primitives gate

October 11, 2026. Real headless Windows execution passes against CADPilot 0.7.7 source commit `58bfee4d45061e55c158a5fd4786ab98a427f7f6` and the previously provisioned official FreeCAD 1.1.4 Windows x64 portable runtime. This is a primitive/round-trip proof, not an application or GUI/RPC certification.

The source archive was retrieved from the pinned [upstream commit](https://github.com/LBurny/cadpilot/tree/58bfee4d45061e55c158a5fd4786ab98a427f7f6). [Archive identity](archive-identity.json) records URL, bytes and independently computed SHA256. It is not a signed upstream checksum. [Actual status](status.json) records the executable, exact module/macro hashes, invocation, fresh-process verification and artifact hashes. [Results](result.json), [fresh-process check](roundtrip.json), stdout/stderr and the first failed rollback fixture are retained.

## What passed

The probe copies five upstream files unchanged into a fresh private import folder: package initializer, geometry queries, object factory, property mapper and tip policy. It uses their real `measure_geometry`, `edit_object_gui` and `create_object_gui` functions; despite the names, these primitive implementations do not require GUI dispatch for the tested operations. No listener, stock addon initialization, MCP server or inherited user addon is started.

- An explicitly named working-copy document is measured and edited while an unselected document is active. Missing document/object queries fail. Original and unselected saved bytes remain unchanged.
- A partial assignment changes Length and then fails on an invalid Width. Aborting the transaction restores both geometry and dimensions, **after explicitly enabling `Document.UndoMode = 1`**. The initial run without that setting failed: console mode left the partial change at 13 mm. [Retained failed evidence](rollback-disabled/status.json), [trace](rollback-disabled/failure.txt). Do not assume a transaction provides rollback unless recording and restoration are verified.
- Typed numeric edits corresponding to 0.5 / 1 / 0.3 inches produce 12.7 / 25.4 / 7.62 mm; a separated cylinder is created using the upstream object factory. Validity, solid counts, dimensions, volumes and placement are checked.
- A PartDesign feature inside a Body placed at (100, 200, 300) reports global bounds correctly through CADPilot's geometry query.
- The edited FCStd retains both native parametric objects. STEP reimport has two valid solids and matching combined volume. A separate fresh FreeCAD process independently checks native parameters, geometry, STEP and post-reopen parameter editing. It saves no changes; all four saved artifact hashes remain unchanged.

Reproduce the runtime probe with `rtk proxy python tests/cadpilot/geometry_probe.py` after provisioning the pinned source archive under `target/cadpilot-probe/source/cadpilot-<commit>` and the FreeCAD runtime at the documented private path. Fixtures create only fresh owned files; they do not import a user's document.

## Execution boundary and next gate

The [stock RPC server](https://github.com/LBurny/cadpilot/blob/58bfee4d45061e55c158a5fd4786ab98a427f7f6/addon/CADPilot/rpc_server/rpc_server.py) registers the entire `FreeCADRPC` instance, including synchronous/asynchronous arbitrary Python execution, document saves and GUI operations. Loopback/IP filtering does not bind calls to a selected project or approved document. The property mapper also accepts expressions and broadly named properties. This probe does not expose those surfaces or certify them as constrained.

The application must bind the selected original hash, working-copy identity, object/operation and typed arguments before dispatch. Preserve the upstream client's distinction between retryable reads and uncertain writes; never blindly repeat a possible edit. Verify a constrained dispatch boundary and actual GUI/addon pair, including catalog/state, rollback, owned cancellation and lost-reply recovery, before offering interactive CADPilot operations. Headless primitive success cannot substitute for that gate.

Remaining: app source/UI and approval/publication/recovery binding, constrained RPC/GUI endpoint, actual selected user-document capture/dependencies, broader modeling operations, native/installed Windows checks and second environment. No goal completion or CADPilot integration-complete claim is made.
