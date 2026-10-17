# Hazlo Mexa desktop

The current app uses a simple chat and each program's own editor. See [current native-flow status](../../docs/native-flow-status.md); older workflow instructions below retain their original scope. For CADCraft's new [native chat integration](../../docs/research/cadcraft-live-chat-2026-10-16.md), open the explicitly selected tested patched runtime with its drawing and local `--control` port, then use **Conexiones → CADCraft → Conectar dibujo abierto en CADCraft parcheado…**. Enter that port and select the tested executable. Ask naturally to create a circle or move an observed circle; Hazlo asks for missing measurements, prepares a reviewed change, uses the native checked tool after approval and rereads the result. Millimetres and inches can be mixed. More commands and native save/export remain unfinished. Installed CADCraft is unchanged; the maintained patched runtime is separate. Old installers cannot open the new schema-16 native-drawing jobs.

If a selected/copied file is reported as occupied, let its pending file operation finish, then retry the inspection or preparation. Hazlo now refuses an NTFS oplock wait promptly rather than letting that open block PCB cancellation. No result is published from that failure, and the original/staging copy is preserved. [Reproduced failure, fixed host-read boundary and exact acceptance scope](../../docs/research/pcb-busy-read-2026-10-13/README.md).

For **Mover componente · PCB**, open the saved PCB and choose a footprint from its measured list. The built-in guide accepts **muévelo 2 mm a la derecha** or a direction followed by an explicit distance with unit. It fills the new position from the inspected coordinates without needing origin coordinates or a memorized reference. Directions use the board plane, right X-positive/down Y-positive; confirm the preview before approval. Moving can leave tracks disconnected. [Guided PCB movement evidence and remaining scope](../../docs/research/pcb-guided-move-2026-10-13/README.md).

For a measured CADPilot box, select **Continuar desde este resultado**, the box and its dimension-edit operation. In the built-in guide, say **hazla más alta** or **make it taller**, then confirm the final height with a unit, for example **0,5 pulgadas**. The chat updates only that draft dimension and preserves the others. Prepare, review and approve separately. If the size is unknown, the guide asks instead of choosing one. Changing source, box, operation, workflow, provider or conversation clears the pending question. [Guided-size and preview evidence](../../docs/research/cadpilot-guided-size-2026-10-13/README.md) covers frontend fixtures; native/installed interaction remains unverified.

CADPilot selected-document operations are available in app source. Select **Documento existente · CADPilot** and FreeCAD 1.1.4. Start with **Medir e inspeccionar primero** when dimensions/names are unknown, then select the FCStd original and identified CADPilot 0.7.7 folder containing `rpc_server`. Approve the inspection and choose **Continuar desde este resultado**, then select a measured box. Its native length/width/height fill the form, so you can change one size while keeping the measured others. Rotated world extents remain separate; older reports without native sizes need a fresh inspection, except for the supported previous-edit parameter fallback. Metric/imperial switches preserve physical measurements. Box edits and new boxes/cylinders operate on a copy; originals and other objects are preserved. [Native dimension and rotated-copy evidence](../../docs/research/cadpilot-native-dimensions-2026-10-13/README.md) and [installed native-dimension/backend scope](../../docs/research/agent-context-installed-2026-10-13/README.md) are separate. Expressions, links, PartDesign histories and uncertified object/resource/GUI states remain unsupported. Open-ended chat, native interaction and installed acceptance of the guided chat changes remain unfinished. Current serial regular suite: 116 passed, 41 separately scoped ignored entries. The initial parallel process timing failure is retained in the [PCB SDK interruption evidence](../../docs/research/pcb-sdk-interruption-2026-10-13/README.md).

The desktop application is being implemented through the expanded [application goal](../../docs/application-goal.md). The Windows Tauri shell includes the verified FreeCAD workflow and Arduino Uno compilation with code review, one-use approval, owned-process cancellation, publication and history. Arduino's native host and installed guest checks pass within their [recorded scope](../../docs/research/arduino-installed-2026-10-11/README.md); remaining engineering integrations are unfinished. Model evaluation is deferred.

For Arduino, provision the official CLI 1.5.1 and AVR core 1.8.8 in a separate Hazlo Mexa toolchain, following the [runtime proof and reproduction steps](../../docs/research/arduino-runtime-2026-10-11/README.md). The prepared data folder needs `hazlomexa-toolchain.json`; ordinary global Arduino data folders are refused. Select **Sketch y compilación · Arduino**, choose the verified Uno target, describe an LED example or paste a sketch, choose **Preparar compilación**, review every line and approve. Editing code invalidates approval. The app saves Sketch.ino, checksum-valid HEX and AVR ELF; it does not upload firmware. **Ver sketch verificado** displays the approved source in a read-only dialog without requiring a text editor. HEX/ELF open their containing folder in Explorer. These controls and native runtime pickers passed installed guest checks.

Run `rtk proxy node apps/desktop/tests/native-arduino.mjs target/debug/hazlomexa.exe C:/absolute/path/arduino-cli.exe C:/absolute/path/owned-data` after building the bundled debug app. This explicitly configured debug fixture uses real native IPC and real compilers. It checks stale approval, native permission denial, keyboard approval, compiler cancellation/independent cessation, failed compilation and reload preservation without models. [Evidence and scope](../../docs/research/arduino-native-2026-10-11/README.md). Current regular backend suite: 54 passed, thirteen standalone ignored entries; actual runtime tests are invoked separately.

CADCraft drawing workflow is now available in app source: select the official Windows x64 0.5.0 `cadcraft-cli.exe`, choose DXF/DWG/PNG, select original units (millimetres or inches), optionally add a circle, then import the original and review before approving. Conversion-loss consent is off by default. Original, result, PNG and literal conversion report are retained. [Real backend checks and unfinished native/installer scope](../../docs/research/cadcraft-app-2026-10-11/README.md). The previously certified installer below predates CADCraft support.

KiCad checks and exports are available in app source: select **Revisión y exportación · KiCad**, official Windows x64 10.0.7 `kicad-cli.exe`, report units (mm/in/mils), then the source folder and principal schematic or PCB. Review captured dependencies and approve ERC/DRC with PDF/netlist or Gerber/drill exports. **Ver hallazgos** separates errors, warnings, exclusions and disabled checks from file-generation success. Originals are preserved and results do not replace prior runs. [Actual backend tests, setup limits and unfinished native/installed scope](../../docs/research/kicad-app-2026-10-11/README.md). The certified installer also predates KiCad support.

The [native host schematic workflow](../../docs/research/kicad-native-2026-10-11/README.md) passes runtime/source pickers, mils selection, actual approval/checks/exports, finding/report dialogs, Explorer selection and restart preservation. Closing the main app exits hidden debug permission-test windows. [Actual backend cancellation and a shortened deadline](../../docs/research/kicad-cancel-2026-10-11/README.md) pass separately; native PCB/Cancel, production-deadline expiry and installed KiCad remain unfinished.

Orca slicing is available in app source: select **Laminado a archivos · OrcaSlicer**, official Windows x64 2.4.2 `orca-slicer.exe`, original STL units, bed surface and initial bed temperature. Choose the STL, profile folder (with `machine`/`process`/`filament` subfolders), and one JSON profile from each category. The portable package includes stock profiles under `resources/profiles/<manufacturer>`. Review normalized dimensions, captured inheritance, profiles and temperatures before approving. Results include original/normalized STL, verified 3MF, matching G-code and a read-only report. [Actual backend evidence and scope](../../docs/research/orca-app-2026-10-11/README.md). Native/installed Orca and broader profiles remain unfinished; the certified installer predates Orca.

For Agent, open the pinned Arduino Agent 0.7.1 with its authenticated local MCP server, then select **Editar copia del sketch · Arduino Agent**. Connect, select a local UTF-8 `.ino`, describe the desired change, edit the proposed code, compare it with the imported original and approve. The app uses the official Rust SDK to edit/open a new temporary copy and publishes Original.ino/Sketch.ino only after verifying the returned document and exact bytes. Tokens never enter frontend state or project history. Reconnection invalidates pending approval. Cancel prevents publication and stops waiting; the separate Agent may still finish changing its temporary copy. The original and accepted artifacts remain separate from those effects. [Real backend proof and remaining native/installed checks](../../docs/research/arduino-agent-app-2026-10-11/README.md). This operation neither compiles nor uploads firmware; use the separately verified CLI workflow for compilation.

Follow the [manual-first demo](../../docs/first-workflow-demo.md) to try the app without AI models. The [acceptance audit](../../docs/acceptance-audit-2026-10-11.md) maps the original requirements to actual native/backend evidence and explicitly pending scope.

Current Agent-capable installer SHA256: `f32aa75be7eb71092e0f365c929e084ffa2ad23927ba4514d599573975fee5d2`. [Build identity](../../docs/research/arduino-agent-app-2026-10-11/build.json) records a passing build; its installed Agent workflow remains unverified. The preceding CLI installer `57c36bf4…` has the recorded [installed scope](../../docs/research/arduino-installed-2026-10-11/README.md) and remains under `target/release-checkpoints/57c36bf4/setup.exe`, alongside earlier packages. Failure keeps verified files in staging and reports generation/verification separately. CLI uses run schema 3 and Agent uses schema 4; older builds can reject newer runs. Existing schema-1/2 FreeCAD records remain readable.

Build the frontend and the bundled debug desktop from the repository root:

```powershell
rtk proxy npm ci --prefix apps/desktop
rtk proxy npm run build --prefix apps/desktop
rtk cargo build --package hazlomexa --features desktop,custom-protocol --bin hazlomexa
```

Launch `target/debug/hazlomexa.exe`. Create a project in an existing local NTFS folder or open one, select FreeCADCmd.exe from FreeCAD 1.1.4 if necessary, and describe what you want to create or recreate in the design chat. The model asks for missing information and uses your follow-up answers. This adapter still creates only rectangular solids; other geometry requires explicit agreement to a rectangular reference block. It does not interpret photos or reference files. You can also expand the manual dimensions section. Editing disables approval until the updated proposal is saved. Approve explicitly to generate, then use the artifact buttons to open in the selected FreeCAD package. Portable packages use bin/FreeCAD.exe; installed layouts may use a sibling executable. An opening failure leaves generated files intact. The configured runtime path is currently session-only; the proven portable runtime path is detected on this development machine.

Manual dimensions support mm, cm, m, inches, and international feet. Switching units converts the values without changing physical size. CAD input and saved geometry remain in millimeters, with the existing 1–200 mm limit per dimension. Manual input uses decimal numbers; chat also accepts fractional inches and mixed units. Review both the selected units and the millimeter equivalent before approval. The conversation is scoped to the project in browser session storage, survives a page reload, and is not added to project snapshots. New conversation clears it. A request includes the whole conversation and is limited to 4096 UTF-8 bytes; starting a new conversation with a summary handles longer exchanges. Selecting cloud sends the whole conversation to Ollama Cloud.

Conversion constants follow NIST: [1 inch is exactly 25.4 mm](https://www.nist.gov/pml/owm/si-units-length), and [1 international foot is exactly 0.3048 m](https://www.nist.gov/pml/us-surveyfoot). UI conversion normalizes floating-point noise below a nanometer; persisted backend dimensions and the fixed CAD adapter remain unchanged.

The revised Windows installer is built at `target/release/bundle/nsis/Hazlo Mexa_0.1.0_x64-setup.exe`. Its [lifecycle check](../../docs/research/chat-units-2026-10-11/installer-result.json) passed with external project preservation. [Native inch-input smoke](../../docs/research/chat-units-2026-10-11/native-result.json) and [30-case cloud benchmark](../../docs/research/benchmarks/2026-10-11-chat-units-cloud-v2/report.json) also pass; full Windows acceptance is still pending.

Run `rtk proxy node apps/desktop/tests/design-flow.mjs` for unit boundaries, imperial conversion, multi-turn context, reload, and approval invalidation in the real frontend with a fixture IPC backend. It uses installed Edge (or `HAZLOMEXA_TEST_BROWSER`), creates no CAD files, and complements the real native smoke test. The expanded cloud benchmark uses the original frozen 20 cases plus ten [design conversation fixtures](../../tests/fixtures/design-conversations.json).

Run `rtk proxy node apps/desktop/tests/ollama-ui-fixture.mjs --self-check` to verify controlled metadata, stalled streaming/disconnect, provider error and Spanish clarification responses without running models. Its normal mode is restricted to an owned Sandbox and binds only numeric loopback; it does not replace a running service. [Installed chat checks](../../docs/research/installed-chat-2026-10-11/status.json) cover the app controls, missing model and seeded provider-error redaction.

For Spanish requests, open Ollama 0.35.1 at its default IPv4 loopback port 11434. The app checks the selected model's tested digest before sending a request. Choose Qwen3 4B local (experimental: ambiguity cases failed), or explicitly select Kimi K2.7 Code cloud (20/20 frozen cases passed). Cloud sends the request to Ollama's service using the existing signed-in account; its routing digest cannot pin mutable remote weights. No model is downloaded automatically. Missing service/model, changed metadata, clarification, or invalid output leaves CAD untouched; manual dimensions remain available. The UI can reconnect to the current inference after reload. Cancellation suppresses late results; inference itself never approves an operation.

The optional native cloud smoke command adds `--cloud C:/absolute/path/ollama.exe` to the smoke invocation below. It temporarily owns port 11434 and refuses to replace an existing Ollama service; it uses existing model manifests/account without downloading models. It checks clarification, cancellation, and reload before the explicit CAD approval flow.

Desktop Rust builds now embed the native KiCad PCB worker. Before the first desktop build, prepare it from the repository root with `rtk proxy python tests/kicad/build_worker.py --python C:/absolute/path/to/Python311/python.exe` (Windows x64 CPython 3.11). Development/package commands reuse a matching artifact or rebuild using `HAZLOMEXA_WORKER_PYTHON`; builds refuse stale source/dependency receipts. The installed application will use the embedded interpreter and SDK. [Native inspection proof and unfinished mutation/installed scope](../../docs/research/kicad-embedded-inspection-2026-10-15.md).

For live frontend development, run `rtk proxy npm run desktop --prefix apps/desktop`. Produce the per-user NSIS package with `rtk proxy npm run package --prefix apps/desktop`. Current-user installation/update/start/uninstall and project preservation pass. Fresh build-26100 guest online WebView2 bootstrap and offline rejection are recorded in [supported combinations](../../docs/supported-combinations.md); exact package revisions and remaining acceptance limits are identified there. The lifecycle test deliberately refuses an existing installation:

```powershell
rtk proxy python apps/desktop/tests/installer-smoke.py "target/release/bundle/nsis/Hazlo Mexa_0.1.0_x64-setup.exe" C:/absolute/path/to/test-project
```

Run the actual WebView2 smoke check after building:

```powershell
rtk proxy node apps/desktop/tests/native-smoke.mjs C:/absolute/path/hazlomexa.exe C:/absolute/path/FreeCADCmd.exe
```

It passed locally with real FreeCAD artifacts, native permission denial for a second webview, stale/edit approval checks, keyboard activation, reload during CAD, and one-job recovery. The cloud variant also passed clarification, cancellation, and reload of a streamed proposal with the correct model selection restored. The debug-only `HAZLOMEXA_SMOKE_PROJECT` environment variable seeds a real validated test folder because CDP cannot operate native folder dialogs; release builds omit it and the unprivileged test window. Screenshots and artifacts remain in the reported temporary evidence directory. This UI test does not certify native folder-dialog interaction, external file associations, or a second machine; the installer has a separate lifecycle test.

Run the Rust contract checks from the repository root:

Latest additional acceptance checks: [offline missing-WebView2 bootstrap](../../docs/research/offline-webview-2026-10-11/result.json) rejects installation without leaving an app or registration, and [local inference cessation](../../docs/research/local-cancellation-2026-10-11-v2/cessation-result.json) passes with the actual Rust client and independently observed live runner. Qwen3's expanded prompt scores 26/30 (17/20 original), so its local quality profile remains experimental. Consult the [current supported combinations](../../docs/supported-combinations.md) before interpreting older historical results below.

To reproduce the bounded cancellation probe with an already provisioned proof model directory:

```powershell
rtk proxy python apps/desktop/tests/local-inference-cancellation.py C:/absolute/path/ollama.exe C:/absolute/path/private-runtime C:/absolute/path/new-evidence
```

It owns a separate loopback server, cancels after a real streamed delta, observes idle runner slots and CPU while the process stays alive, and then cleans up its owned processes. It never replaces the existing Ollama service. The benchmark's `--local model-name [--pull]` selects a research candidate explicitly; this does not add a provider/profile to the application or download models during normal app use.

```powershell
rtk cargo test --workspace
```

Run the actual FreeCAD round-trip checks with an identified official Windows package:

```powershell
$env:HAZLOMEXA_FREECAD_CMD = 'C:\path\to\FreeCADCmd.exe'
rtk proxy python tests/test_freecad.py
```

The native tests use temporary job directories and private FreeCAD configuration/data directories. They generate a new rectangular solid, reopen its editable FCStd document, modify a native dimension, and reimport the STEP derivative. They also check invalid inputs, existing output preservation, and exclusion of inherited user addons/Python paths.

The eight Python native tests passed on the host and a fresh guest with official FreeCAD 1.1.4. Forty regular Rust tests cover input/result contracts, process ownership, project boundaries, publication/recovery, approval invalidation, inference races, and durable history. Publication checks include abrupt subprocess exit before validation, before commit, and immediately after commit, locked output with retained input, simultaneous cancel/commit arbitration, and occupied-destination failure/recovery. Synthetic files isolate publication behavior from actual CAD geometry. Actual snapshot-I/O error 112 preserves previous state/input. Actual CAD publication after disk exhaustion also passes: NTFS commits the complete run and compact snapshot, and reopening preserves both results. See [disk proof and limits](../../docs/research/cad-disk-exhaustion-2026-10-11/status.json); starting generation on an already-full volume and power-loss durability are outside this sample. The additional native Rust launcher test passed separately:

```powershell
rtk cargo test --workspace actual_freecad_runs_inside_owned_job -- --ignored --nocapture
```

Set `HAZLOMEXA_FREECAD_CMD` as above before that command. It runs the real fixed adapter through the Windows supervisor and validates its job identity, exact dimensions, geometry report, output files, and FreeCAD version.

The combined backend workflow also passed with a real FreeCAD process in an accented project path:

```powershell
rtk cargo test --workspace actual_approved_workflow_publishes_and_recovers -- --ignored --nocapture
```

It checks no staging/artifacts before approval, consumes the approval once, publishes both generated artifacts together, preserves completed output after a late cancellation, and recovers the completed run after reopening the project. Filesystem publication tests use synthetic files; the native tests establish actual CAD geometry separately.

To reproduce actual publication after disk exhaustion, run `rtk proxy python apps/desktop/tests/prepare-disk-full-sandbox.py --disk-cad C:/absolute/path/FreeCADCmd.exe` using the official package's root executable. Start/connect its generated owned Sandbox and read `cad-disk-probe.json`, `cad-disk-full-result.json` and logs after the automatic probe finishes. It creates/fills only a new 64 MiB guest NTFS VHD, runs two actual CAD jobs and exports the closed project with hashes. The test is ignored in ordinary host runs and requires the guarded guest volume. A passing CLI launch alone does not establish a passing probe.

Projects require an existing local NTFS folder. The backend rejects junctions/reparse points and path escapes, holds directory pins and a single-writer lock, and replaces run snapshots atomically. During CAD execution the staging directory is pinned against rename without DELETE access, which conflicts with FreeCAD's current-directory handle. After process exit, publication acquires a rename handle, checks the same volume/file identity, revalidates outputs, and renames the whole directory without replacing an existing destination.

FreeCAD 1.1.4 requires ASCII-compatible private configuration and command-line macro paths. The Rust launcher uses a verified Windows short-path alias for accented profile directories, fails clearly if no compatible alias exists, and keeps artifact working paths unchanged. Both accented profile and artifact directories passed the native launcher proof on this volume.

The supervisor starts children suspended, assigns them to an owned Job Object before execution, limits inherited handles, closes stdin, and drains stdout/stderr concurrently while retaining at most 256 KiB per stream. Its tests exercise descendant cancellation, unrelated-process preservation, timeouts, noisy output, and cleanup after owner exit without Rust destructors. These checks do not certify project approval/publication, a desktop installer, or filesystem isolation.

The adapter reads only its fixed `input.json` request, generates `solid.FCStd` and `solid.step`, and writes `result.json`. It never accepts executable code, a destination path, or a shell command from a provider. Approval and publication are enforced in the Rust backend.

New run snapshots use version 2 and retain source/model identity, cloud/local routing identity, adapter version, relative destination, edit history, timestamps, sequence, and terminal failure detail. They store neither the prompt nor raw model output. Existing version 1 run snapshots remain readable without invented source history; the project manifest and geometry operation remain version 1. Unknown versions and malformed metadata are rejected. The native cloud smoke verifies that the completed file exactly matches the backend snapshot, including provenance after a manual edit.

The [Spanish proposal fixtures](../../tests/fixtures/spanish-solid-proposals.json) are frozen before model tuning. Explicit Kimi cloud passed 20/20 original cases and 30/30 expanded chat/unit cases. Local Qwen3's expanded score was 26/30 (17/20 original), below the proposed accuracy threshold; it remains experimental. Further evaluations are deferred at the user's request; see [benchmark evidence](../../docs/research/benchmarks/README.md).

The Rust Ollama client has eight parser/HTTP tests including captured real streams. It uses numeric IPv4 loopback, no proxies/redirects/retries, bounded NDJSON records (64 KiB), total stream (1 MiB), and proposal text (16 KiB). A successful terminal `stop` record and EOF precede strict proposal validation. Metadata checks require Ollama 0.35.1, the backend-selected model digest, nonzero local model size, completion capability, and absent remote-routing fields before sending the user's prompt. Provider text cannot mint an approval or invoke CAD.

Run `rtk cargo test --workspace ollama` for byte-boundary/UTF-8 checks, late errors, truncated responses, duplicate fields, output limits, invalid dimensions, redirect/missing/changed/cloud-model rejection, and cancellation before headers or during a stalled body. Cancellation fixtures observe socket EOF and completion within one second. The client has a two-second connection timeout, 30-second stalled-read limit, and 120-second total proposal deadline. Reqwest 0.13.5 is pinned; root Cargo.lock records resolved dependencies.

These checks establish client behavior. Local accuracy was measured and failed the proposed threshold. Independent local-runner inference cessation and native UI cancellation checks pass; remote inference cessation and GPU acceleration remain unverified. See [supported combinations](../../docs/supported-combinations.md) for the separate scopes.

Run the explicit local-model benchmark from the repository root with absolute paths and a new evidence directory:

```powershell
rtk cargo run --package hazlomexa --example ollama_benchmark -- C:/path/ollama.exe C:/path/private-runtime C:/path/new-evidence --pull
```

`--pull` explicitly downloads the benchmark's Qwen3 4B candidate into the separate runtime's model directory; omit it when already provisioned. The tool starts an owned loopback Ollama server with cloud features and Vulkan disabled, runs the 20 frozen cases through the actual Rust client, adds three warm repeats, captures a real NDJSON response, and saves the digest, request/options, timings, scores, process metadata, and bounded server logs. It shuts down its owned server and descendants on completion/error. It does not replace the user's existing Ollama configuration or service. Record host hardware alongside each report. A successful download is not a benchmark pass.

Run an explicitly selected cloud benchmark using the existing signed-in Ollama account:

```powershell
rtk cargo run --package hazlomexa --example ollama_benchmark -- C:/path/ollama.exe C:/path/private-runtime C:/path/new-evidence --cloud kimi-k2.7-code:cloud
```

Cloud profiles pin official routing metadata before sending prompts. Cloud streams use their observed remote model name and may have an application/json header despite NDJSON framing. Host validation and explicit CAD approval remain mandatory. Use --case fixture-id for diagnosis; a partial fixture run is not a benchmark pass.

Opening evidence: native FCStd and STEP requests preserve completed state and artifact hashes. FCStd renders visible and centered without a manual fit; STEP renders after isometric/Fit All actions. Missing GUI reports that saved files remain available. See [viewport evidence](../../docs/research/artifact-opening-2026-10-11/viewport-result.json) and the [installed workflow](../../docs/research/installed-release-2026-10-11/workflow-result.json).

The native smoke verifies visible Tab focus, keyboard approval/denial, actual owned CAD process termination, history/reload, and cancellation after observing the fixed macro's successful result. Windows CIM observes processes without Python. The latest actual CAD keyboard cancellation sample was 45.39 ms; near-publication IPC cancellation retained Cancelled and published nothing. These are measured samples, not exact atomic-rename timing. Real frontend fixtures also reject delayed Running events and older cancellation replies for both commit/cancel winners. See [current cancellation evidence](../../docs/research/publication-cancel-2026-10-11/status.json).

A clean-environment installer probe is available through tests/prepare-sandbox-probe.py and tests/sandbox-probe.ps1. Preparation requires 3 GiB available host memory and creates only an owned target/sandbox-probe directory, read-only installer input, and writable evidence output. The current installed release's native manual/restart workflow passed in Windows Sandbox; see [the exact checks and remaining cases](../../docs/research/installed-release-2026-10-11/workflow-result.json). Do not infer GUI readiness solely from the probe process remaining alive; native workflow checks must follow a successful environment setup.

To prepare the automated native debug smoke in a fresh guest, first build the current debug desktop binary and release installer, then run `rtk proxy python apps/desktop/tests/prepare-sandbox-probe.py --native-smoke C:/absolute/path/FreeCADCmd.exe`. Start/connect the generated owned guest and let `probe.ps1` complete its installer/WebView2 prerequisite check. Then execute the mapped `C:\HazloProbe\input\native-smoke.ps1` through `wsb exec --run-as ExistingLogin`. Read `native-suite-result.json` and both logs; a successful host CLI exit alone is insufficient. The script uses read-only Node/Playwright/FreeCAD mappings, a controlled missing-model HTTP fixture and real CAD. It tests debug instrumentation of the native commands, separately from the installed release UI. No AI model runs. Host native smoke no longer requires Python/psutil for process observation; it uses Windows CIM and process IDs. The guest attempt currently fails before test execution because no WebView2 debugging listener appears, even though the app renders. See [guest automation limitation](../../docs/research/guest-native-harness-2026-10-11/result.json); use the native manual checks for established guest workflow evidence.

Add --missing-model to the native smoke command for the no-model/manual-fallback check. It temporarily owns an HTTP metadata fixture on 127.0.0.1:11434 and refuses an occupied port. It proves no prompt/download/cloud fallback, a clear missing-model error, and successful manual entry without a stale provider error. This is a controlled metadata fixture, not a local-model accuracy benchmark.

Native create/open project folder pickers have also passed on the development host with an accented path, Escape cancellation, and close/restart/reopen. The project identity and original file were preserved; see [picker evidence](../../docs/research/folder-dialog-probe-2026-10-11/result-v2.json). This does not establish a second-environment installer pass.

New FCStd files include native visibility and a centered isometric camera, so the box appears on opening. For STEP, use FreeCAD 0 for isometric view and V, F for Fit All if needed. Both rendered viewports were verified on the development host; see [evidence](../../docs/research/artifact-opening-2026-10-11/viewport-result.json). Older alpha FCStd files are preserved; select Solid and press Space to show it if it opens hidden.
