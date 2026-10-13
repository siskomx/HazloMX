# Installed recovery and KiCad UI checks

The current `1c4ea36a…` installer extends the [native drawing acceptance](../native-guided-drawing-2026-10-13/README.md) with artifact access, project restart recovery and actual KiCad schematic/PCB operations driven through the installed Windows UI.

The drawing report opens read-only inside Hazlo. `Mostrar resultado` opens File Explorer with the verified `Drawing.dxf` selected. Closing and restarting Hazlo, then reopening the owned project through its native picker restores completed/cancelled history and the usable report button. All nine persistent project files match their hashes from before restart; the transient writer lock is excluded.

The KiCad UI accepts the existing official CLI 10.0.7, explicitly selects inch report coordinates, and captures an owned source folder and its selected main schematic/PCB through native pickers. Each preview lists the four captured files, version, units and exact planned checks/exports before explicit approval.

The schematic operation publishes native ERC, PDF and netlist outputs. Its findings dialog displays two errors, one warning, zero excluded findings and four disabled checks. The PCB operation publishes native DRC, Gerber and drill outputs with zero errors, six warnings and seven disabled checks. The UI explains that completed execution does not establish an error-free design.

A separate PCB run attempted cancellation after its owned KiCad process and running job were observed. Its final state was **completed**, its process exited and its artifacts were published. This attempt does **not** prove running-process UI cancellation; that gate remains open. All 35 earlier artifact files remain unchanged. The attempt's additional completed PCB output is separately hash-verified.

[The sanitized receipt](checks.json) retains versions, identities, findings and scope. Independent filesystem checks verify every report-listed output hash (11 files for the schematic and 18 for each PCB run), valid PDF signature, nonempty netlist, Gerber/drill presence, four preserved imported originals per run, all five unchanged source fixture files and unchanged prior drawing/cancellation records. UI interaction used the computer-use skill; raw captures, profiles and engineering files remain private.

Signed-in agent conversation, live unsaved editing, successful running-process cancellation through the UI, the other seven engineering workflows through native UI, all remaining artifact-opening formats and KiCad restart recovery remain unfinished. No model inference or hardware action ran. The full goal remains active.
