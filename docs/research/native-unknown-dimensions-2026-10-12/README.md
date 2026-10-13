# Native unknown dimensions, approval and cancellation

The current debug desktop, built with `custom-protocol` and embedded production frontend assets, passes the real Windows WebView2 harness with FreeCAD 1.1.4. This uses an owned accented-path test project and the debug-only project preselection/unprivileged window. It is a native source-app test, not installed-release or file-picker acceptance. No AI service, model inference or hardware operation runs.

[Result](result.json), [source/binary identities](source-identity.json), [unknown-dimension screenshot](unknown-dimensions.png), [completed screenshot](completed.png) and the retained project record the proof:

- All three starting measurements are empty. Selecting inches leaves them empty. Entering a one-inch length and switching to centimeters yields 2.54 while width and height stay unknown, preparation stays disabled, the preview stays hidden and the actual backend has no job.
- Explicitly entered dimensions prepare the usual source-bound proposal. Native keyboard navigation has visible focus. Editing invalidates approval; a stale token is refused. Keyboard approval generates and independently verifies FCStd/STEP, and reload restores the same authoritative job and audit/history.
- Keyboard denial publishes nothing. The harness observes an actual owned FreeCAD PID before cancelling another job, checks the native cancelled state and independently verifies that PID has stopped. The observed UI cancellation latency is about 35 ms in this run.
- Cancellation after an actual successful macro result resolves to one authoritative cancelled outcome with no additional publication. The prior completed files and history remain available.

The [first attempt](first-failure.json) passed the early input/approval/publication checks but missed the cancellation PID because starting a fresh PowerShell enumeration allowed FreeCAD to finish first. It is retained as a failed run. The fixed harness warms an owned, bounded metadata-only observer before approval and records the PID while execution is live; it does not add a delay or mock the CAD job. That retry passes the complete gate. Exact publication commit timing is not claimed.

Reproduce from `apps/desktop` after the current frontend build and `cargo build -p hazlomexa --features custom-protocol --bin hazlomexa`: `rtk node tests/native-smoke.mjs <debug-app.exe> <FreeCADCmd.exe>`. The harness owns its app, project, WebView profile and observer and stops its owned processes. Cloud/missing-model modes are separate and were not run here.

Native unknown-size conversation with signed-in agents, project/runtime picker acceptance, actual artifact opening, installed-release acceptance of this input change, other native integration workflows and live unsaved-document editing remain unfinished. The latest tested `d8c54f69…` installer predates the empty FreeCAD draft change. The full application goal remains active.
