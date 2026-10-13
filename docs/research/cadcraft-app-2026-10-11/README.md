# CADCraft application workflow

October 11, 2026. The Windows app source now offers drawing inspection, copy conversion to DXF/DWG/PNG and a bounded circle addition. It uses the official pinned CADCraft 0.5.0 CLI, not the unauthenticated GUI control endpoint. [Runtime identity and conversion-loss gate](../cadcraft-runtime-2026-10-11/README.md).

October 12 follow-up: [the native host metric-circle workflow and restart check now pass](../cadcraft-native-2026-10-12/README.md), along with a separately observed actual backend CLI cancellation sample. Earlier unverified native/cancellation statements below describe the original evidence date; the follow-up records the remaining scope precisely.

Native pickers select the exact identified CLI and a local original DXF/DWG. Import captures original bytes in the selected project, checks declared units through the CLI's read-only `units {}` query, and gathers entity counts and extents. Millimetres and inches are supported; missing units require the explicit interpretation chosen in the form. No automatic rescaling is performed. Source inputs are regular non-reparse files up to 32 MiB. The original path is never used as a writing destination.

The proposal binds original name/format/SHA256, original summary, output format, units, optional circle centre/radius and loss consent. The imported bytes and summary are checked again after one-use approval. Owned Windows processes and fresh profiles create only working copies. The executable and its directory lineage are pinned while its exact hash is verified and it runs. Output hashes, original identity, PNG shape, drawing signature and applicable entity-count checks are verified before the existing nonreplacing atomic publication. Schema 5 history reuses the existing recovery rules; earlier schemas remain compatible.

The Spanish workflow gathers intent without model dispatch, presents parameters for review, locks approved proposal controls, displays progress and offers Cancel. Original/result/PNG are explicitly selectable in Explorer; the conversion report opens as literal read-only text in the app. To change a pending drawing proposal, cancel it and prepare another from the original. This first bounded edit is circle addition; broader interactive GUI editing is unfinished.

Actual host checks pass: [retained test status/source hashes](status.json), [stdout](stdout.log), [build output](stderr.log).

- Production adapter: metric DXF, imperial DWG, DWG import/return to DXF with units retained, incorrect DWG-unit rejection, circle addition, PNG export, rejection of unsupported-object conversion without loss consent, accepted reported loss with consent, original preservation and rejection after preview tampering.
- Shared workflow: denial, expired/used approval rejection, completed publication and all four artifact controls, occupied destination preserved with verified staging retained, cancellation before execution, prior output unchanged and four-run project reopen without regeneration.
- Browser fixtures: runtime gate, metric/imperial request fields, circle parameters, locked proposal fields, approval controls, four result actions, escaped read-only report and no model dispatch. Existing FreeCAD/Arduino/Agent UI regression checks also pass. [Fixture screenshot](frontend-fixture.png).

The closed host test project's file hashes are retained [separately](closed-project-hashes.json). They identify this host sample; they are not a second-environment export or native GUI proof.

Current regular suite: 54 tests, 13 separately invoked ignored entries. Desktop/custom-protocol Clippy and frontend build pass. The shared frontend/native JSON fixture additionally covers schema 5; its initial integer/float representation mismatch was corrected in the fixture, not production behavior.

Reproduce the actual adapter/workflow checks with `rtk proxy python tests/cadcraft/verify_app.py` after provisioning the exact runtime. The runner retains an independent UUID output and asserts both required ignored tests pass. Use `rtk proxy node apps/desktop/tests/design-flow.mjs` for browser-only controls.

Still unverified: actual native picker/GUI operation and installed lifecycle; native in-flight CLI cancellation/independent cessation and timeout samples; cancellation during import inspection; complex drawing/property/block/layout/constraint preservation; second Windows environment; live GUI editing/control-port boundary. PNG is a model-space preview, not a certification of every foreign object. Equal counts and extents cannot establish complete fidelity, and accepting loss permits omitted unsupported objects. Import inspection is a bounded background operation but currently has no dedicated cancel button. The latest previously certified release installer predates these CADCraft changes; do not infer installed support from source checks. The full multi-integration goal remains active.
