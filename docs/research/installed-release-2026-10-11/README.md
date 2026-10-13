# Current installed release in Windows Sandbox

The installer tested here has SHA256 `6484c18e6147b104b6abca47fcae8aef5e43b959c2a11f41dd1cb0919c055d3d`. The fixture guest was build 26100, 2 GiB RAM, with no registered WebView2 initially. Only owned test inputs/output were mapped; the pinned FreeCAD portable directory was shared read-only as `C:\FreeCADRO`. No user AI credentials or model weights were copied.

The first bootstrap attempt failed with code 2 and no installed app. An independent guest download of the same bootstrap URL succeeded, then the unchanged installer passed its bootstrap/install/start/uninstall probe. The initial cause was not established. Preserve both observations; this is not proof that bootstrap is always reliable.

To repeat the lifecycle check, build the installer, run `rtk proxy python apps/desktop/tests/prepare-sandbox-probe.py`, and start/connect the generated owned guest with `wsb`. If its LogonCommand does not run, execute the mapped `probe.ps1` once through `wsb exec --run-as ExistingLogin`; inspect `output/result.json`, including the guest exit code, rather than assuming the host CLI code proves success.

For the native manual workflow, reinstall that package in the guest, set `HAZLOMEXA_FREECAD_CMD=C:\FreeCADRO\FreeCADCmd.exe` for the owned app process, and create `C:\HazloGuestProject` with a sentinel original file. Use the actual native picker to create a project. Expand manual dimensions, switch the default 10/20/30 mm values to inches, prepare, review the canonical dimensions, then approve. Verify the run JSON, artifact validation result and FCStd viewport after explicit opening. Terminate only the owned app after Completed, restart it, use the native open picker, and inspect restored history. Prepare another proposal and deny it; verify that only one artifact directory exists and the original/artifact hashes match the pre-restart observation.

The native UI actions used the Computer Use skill with fresh screenshot-backed clicks. Remote guest keyboard input was unavailable; this run therefore does not prove keyboard navigation or arbitrary typed inch input. Those have separate host evidence. Close the test app before copying the project because its single-writer lock prevents a live recursive copy. Export the entire project and compare every hash before stopping only the owned guest.

See `workflow-result.json` for the exact passing scope and unchecked cases. The clean guest was stopped after exporting nine files; no full G6 acceptance claim is made.
