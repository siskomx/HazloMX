# Native opening failure preserves saved engineering results

The current debug app passes an actual WebView2 test of a failed **Abrir FCStd** action. The fixture copies the [previously generated and verified FreeCAD project](../native-unknown-dimensions-2026-10-12/README.md) into a fresh accented-path folder, preselects it with the existing debug instrumentation, and points the runtime selection at an unavailable owned location. No engineering files or reports are fabricated and no AI service is called.

The native history recovers the completed run. Clicking its FCStd button invokes the real desktop `open_artifact` command. The backend reports missing FreeCAD GUI software, the UI separates opening failure from saved results, and the artifact buttons remain present. Every engineering project file keeps its exact hash; reload recovers identical history and unchanged files. The operational `.hazlomexa/writer.lock` is explicitly excluded from byte comparisons because the running app holds its exclusive lock.

[Result](result.json), [opening failure screenshot](opening-failed.png), [recovered screenshot](recovered.png), [app/harness identity](identity.json), retained project and [file hashes](file-hashes.json) record the evidence. The screenshots were captured from the real WebView; the failure view was visually inspected. This tests error handling and preservation, not successful document opening.

Two initial harness failures are retained: [WebView metadata not ready](first-startup-failure.json) and [attempted reading of the correctly held writer lock](writer-lock-observation-failure.json). The harness now waits for the main WebView and excludes only that named operational file. The corrected gate passes, including the final screenshot run. Owned app/WebView processes are closed afterward.

Reproduce from `apps/desktop`: `rtk node tests/native-opening-failure.mjs <debug-app.exe> <verified-FreeCAD-test-project>`. Use the current `custom-protocol` debug binary; the project input must contain actual compatible completed results. The test creates its own copy and does not alter the source project.

Successful GUI document loading, native file pickers, installed-release acceptance and signed-in conversation remain unfinished. The existing shell-launch acknowledgment does not prove an engineering document opened or establish live unsaved state. This debug preselected-project gate does not complete those requirements. The full application goal stays active.

Later [native successful opening evidence](../native-opening-success-2026-10-12/README.md) separately passes FCStd/STEP dispatch and actual GUI geometry readback through an isolated, read-only test observer. Native pickers, installed-release and product live-GUI tool access retain their unfinished scope.
