# Native tool routing in actual Windows programs

October 17, 2026. These checks use the shared discovery/proposal/readback route
from the actual MCP SDK and schema-17 jobs. The acceptance harness supplies test
approvals; observed program windows are separate evidence. This is not real
provider inference or a claim of human approvals in Hazlo's UI.

| Program | Actual native result |
|---|---|
| FreeCAD / CADPilot | The 27-tool catalog drives the owned open support from 20 × 15 × 10 mm to 25.4 × 15 × 12 mm. Native readback reports a valid 4,572 mm³ solid, the changed model is visible, and native UI Undo restores the original dimensions and 3,000 mm³ volume. No model is saved. |
| CADCraft | The explicitly selected maintained runtime exposes 15 native tools. Native command documentation, line creation, a 2.54 mm follow-up move and two native Undo commands pass. The new line is visible, the original circle remains unchanged, and the source DXF bytes are preserved. No preset Hazlo geometry wrapper is used. |
| KiCad PCB / Konnect | Loading the discovered footprint and navigation toolsets expands the catalog from 21 to 46 tools. IPC reports KiCad 10.0.7 and the exact owned board. A 2.54 mm move preserves the other eight footprints and the saved board. The basic native move does not provide Undo in this runtime; the discovered transactional placement tool does. A separate 1 mm transactional move visibly changes the board, and native UI Undo restores all footprint positions. |
| Arduino Agent | The actual authenticated local HTTP MCP exposes four discovery/router tools. Its native sketch category supplies the nested tool schema. A saved sketch visibly changes from a 1,000 ms delay to 500 ms, then 250 ms, preserving a test note. Native readback and saved bytes agree. This native tool writes immediately; unsaved-buffer preservation and Undo are not claimed. No hardware is used. |
| Arduino CLI | The shared native CLI route compiles that saved 250 ms sketch for Uno. Independent Intel HEX validation and the ELF header pass; 590 firmware data bytes are present. Source bytes remain unchanged. A CLI compilation is not live editor control. |
| OrcaSlicer CLI | Native CLI slicing and output information readback pass for a 10 mm cube: 50 layers, 10 mm height, metric G-code and matching G-code checksum. Input bytes remain unchanged. After the user finishes the setup wizard, the generated cube is observed in the native output window. The saved 3MF remains unchanged, and the owned window is closed. No live open-document control or printer connection is claimed. |

The selected KiCad endpoint explicitly reports schematic document enumeration
as unsupported. A saved-file workflow is not used as evidence of live schematic
editing. The historical patched CADCraft runtime remains separate from installed
CADCraft.

## Problems found and corrections

FreeCAD's full object reply includes volatile memory addresses. The original
guard correctly refuses that edit. A stable geometry query through the native
Python tool permits the reviewed edit without weakening the guard. One camera
snippet used an unavailable alias; its native error is retained, and a subsequent
explicit call using the actual native module fits the view. Hazlo's automatic
review now explicitly checks all native response text for errors, not only formal
MCP error flags or the final state.

KiCad's single-footprint move and transactional placement have different Undo
behavior. The tests retain the failed single-move Undo attempt. The transactional
tool restores the test fixture, then passes its own separate movement/Undo check.
Footprint ordering changes across transactions, so independent comparison uses
reference identity rather than list order. No program-specific operation wrapper
or upstream runtime patch is added to solve this.

OrcaSlicer mixes ordinary standard streams with `CONOUT$`. The owned console
helper now captures both. GUI-subsystem executables start inside the hidden
console from their first CLI call, using Windows' documented
[PE subsystem field](https://learn.microsoft.com/en-us/windows/win32/debug/pe-format).
The CLI catalog retains actual captured help for the selected binary hash;
repeated console rendering cannot invalidate an unchanged executable's catalog.
Every dispatch still checks the executable's hash. MCP tool catalogs continue to
refresh from the server. Control sequences alone cannot count as native help.

The actual Arduino router uses `tool_name`; that field now receives the same
hardware-command refusal as the other routed command identifiers. Regression
checks cover this, empty terminal help, cached help identity, and mixed console
and standard-stream capture. General native scripting remains outside an OS
sandbox; these checks do not claim universal confinement.

The generic `native_session_probe` example is an owned acceptance harness under
the ignored workspace test directory. It discovers schemas, uses the production
SDK tool boundary and existing approval/job machinery, and stores raw replies
privately. It is not bundled with the app and adds no product editing UI.

The default 201 Rust tests, desktop Clippy with warnings denied, frontend build,
bounded conversation check, and chat IPC cases pass. Native SDK fixtures and real
runtime discovery/edit checks pass separately. Initial failed attempts remain in
private evidence; no raw configuration, credentials, logs or captures are published.

Installer rebuilding and fresh Windows guest acceptance follow this source
checkpoint. The earlier installer does not contain these corrections. Full
provider and installed engineering/UI acceptance retain their explicit limits.
