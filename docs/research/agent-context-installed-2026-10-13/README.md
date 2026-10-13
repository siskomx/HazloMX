# Current chat-context package: installed backend and lifecycle

The unsigned 0.1.0 installer 9314157498049b831bddb38d1dfee5e65543886b0986abba7afcdad83298970a (5,346,420 bytes) passed this scoped gate in fresh Windows Sandbox build 26100. It includes the selected desktop chat context and native CADPilot box-dimension source changes. The pre-launch release-inputs manifest was captured before guest execution and all source hashes still matched at export.

The guest passed per-user installation of the previously tested ab43ac2a package, exact baseline bytes and fifteen-tool checks, upgrade to 93141574, exact current installed binaries and fifteen-tool checks, 21 actual native jobs through the installed connector and shared backend fixtures, five-second hidden app-process startup, and uninstall preserving project sources, prior results and application user data. This first run did not require the earlier package's explicit WebView2 bootstrap recovery. The runtime copy took substantial time; live process/CPU/I/O observations confirmed activity, and the same guest was retained throughout. The guest session was established through the Sandbox CLI without native input automation.

| Included workflow | Actual completed jobs and scope |
|---|---|
| Arduino Agent 0.7.1 | 2 file edits, original preservation, refusal/reconnect/recovery; raw logs private and exported token redaction checked |
| Arduino CLI 1.5.1 / AVR 1.8.8 | 2 Uno compilations plus actual invalid-code failure and denial/recovery |
| KiCad 10.0.7 | 2 checks/exports, metric/inch-mil reporting and genuine ERC findings |
| Konnect 0.13.0 / KiCad 10.0.7 | 4 schematic/PCB edits; missing CRT refusal then verified signed app-local runtime supplied |
| CADCraft 0.5.0 | 4 DXF/DWG metric/inch edits and saved-result follow-ups |
| CADPilot 0.7.7 / FreeCAD 1.1.4 | 3 inspection/box-edit/cylinder-addition jobs with measured saved-result readback |
| OrcaSlicer 2.4.2 | 4 metric/inch slicing jobs and settings follow-ups |

The included workflow receipts, projects, preserved files and driver are retained under guest/. Package/runtime/fixture identities are in package-identity.json. Native UI drivers and new-solid FreeCAD execution were deliberately omitted and are recorded in fixture-scope.json; FreeCAD generation retains its separate evidence. Approvals and execution use shared-backend fixtures, while the installed stdio connector uses the tested package bytes. This does not prove native GUI/picker/approval/Cancel interaction, production desktop command input, signed-in interpretation, ordinary nonadmin-user behavior, or open/unsaved-document editing. Sandbox's WDAG user is not evidence for the ordinary-user gate. The host caller-context browser gate remains separate from this backend/lifecycle run.

Native UI automation remained stopped, and no model inference, firmware upload or printer/device control ran. The full application goal stays active. Guest shutdown is recorded separately after retaining and verifying these results.

The completed driver was independently absent from the final process snapshot before shutdown. The owned guest was stopped; Sandbox list returned no environments. All three published CADPilot reports include the new native box-dimension field. Shutdown and this scoped readback are recorded in separate receipts.
