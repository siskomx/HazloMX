# Installed schematic fidelity and result recovery

Native schematic editing now preserves the full project’s numeric settings. A second package fixes nested result selection in Explorer and recovers the precise edit. The evidence is deliberately split between those two exact installers; a fresh edit on the final package and its full guest matrix remain pending.

## Native approved edit

Installer `f03a7709…` includes the schema-2 number-preservation fix and the UI changes below. The owned installed window reopened the existing project through its native folder picker, restored its earlier conversation, and accepted the Konnect 0.13.0 and KiCad CLI 10.0.7 executable pickers. Source-folder and schematic pickers discovered 13 components. Selecting the first resistor by its displayed description and saying **cámbiala a 33k** filled the draft without typing a reference. The separate preview showed R1’s actual 10k → proposed 33k and all four captured files; explicit approval completed the native edit.

Independent schematic AST and saved-netlist checks confirm only the approved Value changed. Python Decimal comparison of the entire `.kicad_pro` confirms every other value is preserved, including the seven settings that previously rounded. The PCB and worksheet bytes are unchanged. All four source files, 133 earlier artifact files and eleven report hashes match. The read-only report opens. `approved-edit.json` binds this evidence to its installer and installed app/helper hashes.

The edited-file reveal initially opened Desktop instead of selecting the schematic. This is a failed opening check, not a pass. Nested artifact names contained forward slashes; the shared shell-opening boundary now rebuilds the already validated path from Windows path components before constructing Explorer’s `/select` argument. Validation, pinned files, Unicode and approval rules stay intact.

## Rebuilt opening and recovery

Installer `f6305875…` contains the one-line native path normalization. The guarded owned per-user update verifies installed bytes against the build, accounting for Tauri’s NSIS marker, and preserves all **245 persistent project files**. Native project reopening restores both the earlier schema-1 edit and the new schema-2 result. Its history displays the newest result first. The original schematic, edited schematic and netlist buttons each select the correct saved file in Explorer; the report opens in a read-only modal. All owned Explorer windows and Hazlo are closed after testing. Independent post-test checks confirm all 245 persistent files still match their hashes. `opening-recovery.json` records this final-package scope; it does not claim another approved edit on that package.

## UI changes and checks

- Recovered history sorts by the creation timestamp already used in its date labels, newest first, with stable job-ID ties and undated legacy entries last. Sorting uses a copy and does not modify saved records.
- Idle Results text asks users to gather inputs in chat and review a proposal across all nine workflows. It no longer assumes every workflow needs dimensions.
- File-opening notices clear when the project, workflow or job changes, and when continuing from a CADPilot result.

TypeScript/Vite and the full browser design-flow suite pass. The added browser scenarios cover deliberately unsorted history, equal dates, undated legacy entries, idle copy across all nine workflows and clearing an opening failure on workflow change. The first placement of the history fixture interfered with an earlier discovery assertion; moving it to the isolated final scenario restored the full pass. The desktop binary’s custom-protocol test passes. The preceding precision change’s 118 library tests, Clippy and native adapter checks are recorded in the [number-fidelity report](../kicad-project-number-fidelity-2026-10-13/README.md).

Each package was built from 6,523 captured tracked inputs verified unchanged through its build. Final release app/helper scans match zero builder-profile paths in UTF-8 or UTF-16. Public receipts contain only whitelisted identities, relative artifact names and results; raw paths, projects, captures and logs remain private.

## Remaining work

Konnect PCB, Arduino Agent and OrcaSlicer still need their native UI passes. The latest installer still needs a fresh approved edit and complete second-Windows/ordinary-user acceptance; older package matrices are historical evidence. CADPilot add operations, ordinary GUI-saved CAD fidelity, signed-in conversations and live unsaved editing remain unfinished. No model inference, firmware upload or hardware/printer control ran in this test.
