# First Windows workflow acceptance audit

This is the historical FreeCAD-first goal audit. The user cancelled that goal and registered the broader [application goal](application-goal.md), which includes all engineering integrations. Local-model quality remains experimental and further model evaluation is explicitly deferred; it does not block the expanded engineering implementation. New [Arduino host/native evidence](research/arduino-native-2026-10-11/README.md) and a FreeCAD regression on the multi-workflow build are retained separately below that expanded goal.

October 11, 2026. App workflow and covered Windows checks pass. Goal remains active because the proposed local-model accuracy gate failed and further model evaluation is deferred.

The current application implements the requested manual/Spanish design workflow, chat gathering and metric/imperial entry. This audit distinguishes direct production/native checks from synthetic fixtures, earlier package revisions, and unsupported combinations. No new model evaluation ran during this audit. Latest release installer SHA256 is `cc395aa2aad549181301a10241dc467ddd50b52b1c069dc3efb2c98add491f15`.

Evidence keys:

- **C**: [current native cancellation and fresh offline 40-test backend suite](research/publication-cancel-2026-10-11/status.json), including its raw backend log.
- **F**: [installed fixed-macro/publication failures and native reopen](research/publication-recovery-2026-10-11/status.json).
- **N**: [installed CAD cancellation, denial, missing runtime/GUI](research/installed-negative-2026-10-11/status.json).
- **I**: [installed chat, missing model, stalled cancellation and seeded-error scan](research/installed-chat-2026-10-11/status.json).
- **W**: [installed manual/inch workflow, visible FCStd and abrupt restart](research/installed-release-2026-10-11/workflow-result.json).
- **G**: [actual guest FreeCAD geometry/failure and owned-launch tests](research/guest-backend-cad-2026-10-11/result.json), with [macro identity](research/guest-backend-cad-2026-10-11/identity.json). The current macro hash matches this evidence.
- **M**: [frozen local/cloud benchmark evidence](research/benchmarks/README.md).
- **P**: [current installed publication overlap and exact-package fresh WebView2 bootstrap](research/installed-publication-2026-10-11/status.json), with held/release markers, native screenshots and all 23 closed-export file hashes. The failed first fixture attempt is retained separately.
- **D**: [actual CAD publication on an exhausted guest NTFS disk](research/cad-disk-exhaustion-2026-10-11/status.json), including actual result/log, closed-export hashes, current 40-test suite and corrected launcher.

## G1 — fixed FreeCAD operation

| Requirement | Result and authoritative evidence |
|---|---|
| App-owned fixed script, JSON input, no provider code or destination | Implemented in `freecad::generate`, `SolidInput`, and bundled `create-solid.FCMacro`; actual native invocation in G/F/C |
| Embedded Python, native Part::Box, no GUI import; stdin closed; process terminates | G's eight native tests and two production backend tests; `process::run_owned` owns stdin/process handles |
| 10/20/30, 1/1/1, 200/200/200 and 12.5/7.25/3.5 mm geometry | G native `test_native_and_step_round_trip` iterates all four |
| Editable FCStd; changed Length recomputes; STEP reimports | G's verification macro checks native parameters, recomputes and measures STEP |
| Bounds 0.01 mm and volume max(0.001, expected × 0.000001) | Actual geometry tests in G; runtime `measure` and `validate_result` retain the same bounds |
| Exception, invalid/duplicate/unknown fields, missing input/version, failed export, absent/failed result reject success | G's seven failure/isolation tests; F directly observes fixed macro success=false and persisted Failed |
| Fresh job identity and staging outputs; Unicode/spaces/apostrophes | G production adapter tests retain accented artifact paths; C native project path is accented; backend identity tests reject mismatch |
| Private config, user-addon/Python-path exclusion | G native isolation test plus owned launcher. ASCII profiles pass; accented private profiles without an ASCII short alias fail closed and are unsupported |
| Retained script, fingerprints and invocation/geometry fixtures | Current macro SHA256 `5abc79546b17926d0ecc8590bd013e373bdf73e8f00cf9dbd4f7930d98e47df1`; source macro, `tests/test_freecad.py`, verification macro and G identity/logs retained |

## G2 — ownership and cancellation

| Requirement | Result and authoritative evidence |
|---|---|
| Cancel before dispatch creates no child | C raw suite: `cancellation_before_dispatch_creates_no_process_or_file` |
| Cancel during staging produces no publication | C real macro-success cancellation returns Cancelled with no artifact directory; N actual CAD cancellation also publishes no new set |
| Owned descendants terminate within five seconds; unrelated process remains | C native keyboard sample 45.39 ms and both observed PIDs absent; process test verifies unrelated-process preservation; N independent guest CAD observer confirms cessation |
| Drain both streams under limits; noisy child cannot deadlock/unboundedly fill memory | C raw suite: `quotes_arguments_and_drains_both_pipes_under_limits`; supervisor retains at most 256 KiB per stream |
| Crash/shutdown close ownership handles; no surviving descendants | C raw suite: `abrupt_owner_exit_closes_job_and_terminates_descendants`; test bypasses Rust destructors |
| Denial, repeated/stale approval, old output or project switch cannot dispatch | C native ACL/approval tests and regular workflow tests; N/W installed denial |
| Defined cancel/commit outcome; persisted recovery agrees | C's 24 concurrent backend iterations and raw guest log; real native near-publication cancellation; frontend fixtures exercise both winners and stale messages. P establishes native Cancel input while actual production publication is held, followed by a complete committed set. It does not instrument the indivisible rename syscall or exact backend Cancel dispatch instant |
| Retained state/supervisor/noisy-child fixtures; no filesystem-sandbox claim | `jobs.rs`, `process.rs`, `tests/fixtures/process_probe.rs` and C logs retained; documentation explicitly distinguishes Job Objects from filesystem isolation |

## G3 — project publication and recovery

| Requirement | Result and authoritative evidence |
|---|---|
| Reserved stage and complete run directory; whole-set nonreplacing publication | `Project::reserve_stage/publish`, C raw suite; W actual CAD; F occupied destination stays intact |
| Preserve existing originals and output | W/N/F independently compared closed-export hashes; F's eleven baseline files unchanged and all twenty-one files verified after uninstall |
| Abrupt exit before validation, before commit and after commit; restart never regenerates | C raw `abrupt_exit_at_publication_boundaries_recovers_without_regeneration` uses native ExitProcess(73); W actual post-publication restart |
| Failed/partial snapshot write preserves readable prior state | C regular partial-write test and actual bounded NTFS error-112 probe |
| Denial/traversal/junction/ADS/device/UNC leave outside files untouched | C raw project-boundary tests and workflow denial tests |
| Reject second writer and unknown formats without rewriting | C raw `single_writer_unknown_versions_and_originals_are_preserved` and metadata/contract tests |
| Locked output and failed rename actionable; preserve input/stage | C locked-output test; F actual error 183 retains generated FCStd/STEP and gives a Spanish publication failure |
| Disk exhaustion actionable and input/state preserved | C/D bounded 64 MiB NTFS probe reaches actual error 112 on snapshot I/O, preserves bytes and recovers Interrupted. D also fills the volume after actual CAD generation and immediately before production publication: whole-set commit and compact snapshot save legitimately succeed, reopen preserves both Completed runs, all thirteen exported file hashes match. This sample does not start generation on an already-full disk |
| Occupied failed destination does not block project reopen; completed corruption still rejects | F native reopen and C six-phase regression; existing occupant is never overwritten |
| Versioned manifest/run schema and independent fixtures retained | Manifest/input version 1, audited run version 2; `contract-fixture.ts` compared to Rust; legacy run data remains readable. Power-loss durability, synced/network volumes and concurrent writers are outside supported scope |

## G4 — native boundary and package

| Requirement | Result and authoritative evidence |
|---|---|
| Privileged bundled main window only; explicit commands/capability/CSP | Current `build.rs`, `capabilities/main.json`, `tauri.conf.json`; C native unprivileged window denied get_snapshot |
| Direct calls cannot bypass root/approval checks; no frontend generic shell/filesystem/HTTP permission | C native stale approval plus regular boundary tests; capability grants only bounded application commands |
| Per-user NSIS install/start/update/uninstall preserves projects | F current package installed, updated, started and uninstalled on build 26100; twenty-one project hashes preserved; earlier host lifecycle records cover HKCU registration |
| Missing WebView2 strategy; honest offline failure | Earlier unchanged-config online bootstrap passes in W/N/I; [offline package exits 2 without app/registration](research/offline-webview-2026-10-11/result.json). P additionally passes fresh online bootstrap/install/start/uninstall on the exact current installer hash |
| Ordered channel/state handling; reload does not create/approve a second job | C actual native reload during CAD and current snapshot/history checks; real frontend rejects stale Running/cancel replies for both terminal winners |
| Keyboard navigation, visible focus, approval/denial and Cancel | C actual native Tab/Enter checks and 45.39 ms keyboard CAD cancellation; N installed mouse cancellation independently verifies process cessation |
| Locked build/setup artifacts retained | Root Cargo.lock, npm lockfile, exact Tauri/Reqwest versions, app config/permissions, current release executable and NSIS bundle; setup in desktop README |

## G5 — model/protocol contract

| Requirement | Result and authoritative evidence |
|---|---|
| Every byte boundary, including accented UTF-8, preserves recorded streams | C raw parser tests replay synthetic and recorded actual streams |
| Late/mid-stream error, malformed/duplicate/oversized/truncated records never produce an executable proposal | C raw parser/host-validation tests; I seeded provider error stays generic and creates no job |
| Responsive cancellation and independently observed local cessation | Native inference UI sample in [cloud smoke](research/desktop-cloud-smoke-2026-10-11/result.json); I native stalled-fixture controls/disconnect; [actual live local runner cessation](research/local-cancellation-2026-10-11-v2/cessation-result.json). No remote cessation claim |
| Local requests do not follow proxies/redirects or cloud routing | Current client numeric loopback/no-proxy/no-redirect/no-retry setup and C transport/metadata rejection tests |
| Missing model explicit; no auto-download or cloud fallback | I installed metadata fixture rejects before show/generate; source client requires explicitly selected profile |
| Frozen Spanish cases and strict host rejection | M original 20 plus ten separately frozen chat/unit cases; invalid dimensions rejected before proposal/approval |
| Measured model identity, hardware/backend/options/cold/warm latency | M records Ollama 0.35.1, pinned Qwen3 digest, CPU/zero GPU memory, 4096 context, cold loading and median/p95 samples |
| At least 18/20 acceptable local outcomes | **Not passed:** expanded Qwen3 17/20 original (26/30 total). Local selection remains explicitly experimental; manual entry is independent. User deferred further model evaluation. Explicit cloud passed 20/20 and 30/30; this does not certify local quality |
| Warm p95 proposal within 30 seconds | Expanded local measured p95 11.72 s; cloud 1.83 s. Initial local loading exceeded 30 s; no GPU or universal latency claim |
| Recorded streams/results/profile artifacts retained | `tests/fixtures/ollama`, frozen JSON cases and M reports/digests/logs; private model weights/account keys are not repository deliverables |

## G6 — complete workflow and user additions

| Requirement | Result and authoritative evidence |
|---|---|
| Spanish request → validated proposal → editable approval → real CAD → verified FCStd/STEP → explicit opening | [Native actual cloud workflow](research/desktop-cloud-smoke-2026-10-11/result.json), chat/unit native evidence and actual [visible FCStd/STEP](research/artifact-opening-2026-10-11/viewport-result.json). Current C manual native path passes; F current installed failures/recovery pass |
| Second Windows environment | Build-26100 installed W/N/I/F checks and current C offline backend. W manual/inch path completes real CAD, visible FCStd opening and restart. P adds current installed publication input overlap; Spanish protocol/clarification and actual CAD remain separately evidenced |
| Denial; inference/CAD/commit cancellation | Installed denial and inference/CAD checks N/I/W; exact backend commit arbitration C; native near-publication and frontend message-ordering C. P now covers installed Cancel input while the production publication read is blocked; committed output remains Completed after release |
| Missing runtime, script error, occupied destination, restart, selected model missing | N/F/W/I directly exercise installed native UI; occupied-destination reopen regression now passes |
| Seeded credential excluded from logs/persisted data | C backend suite checks log sentinel exclusion; I scans 295 readable project/app-specific files after closure. This is not a deleted-data, registry or process-memory forensic claim |
| Separate generation, validation, publication and opening results | F current UI and persisted verified-stage marker; opening failure preserves old committed files N/W; current frontend fixture tests project-scoped opening status |
| Metric and English/imperial measurements | mm/cm/m/in/international ft; unit/proportion fixture and native inch workflow; canonical mm and 1–200 mm limits unchanged |
| Chat gathers intent/unknown dimensions for create/recreate | Current chat-first UI, project/session scoped turns, manual edits as context, controlled installed Spanish clarification I; real cloud multi-turn/unit benchmark M. Only rectangular solids supported; photos/reference files are not interpreted |
| Retain one demo, support matrix, failure fixtures and build | [Manual-first user demo](first-workflow-demo.md), supported-combinations.md, native/guest fixtures, dated evidence and current NSIS bundle retained |
| Preserve website/projects; pin versions; defer wider integrations | Tracked source diff touches only root README/.gitignore; new desktop lives under apps/desktop. Existing projects preserved in W/N/F. No publish, push, firmware/printer operation, provider expansion, cloud sync or ARM64 work performed |

## Remaining acceptance scope

The application and the covered native workflows pass. Installed Cancel input overlapping actual publication is now proved in P, with complete output and all 23 exported file hashes verified. Exact command dispatch and the indivisible rename syscall are not instrumented. Full goal completion remains unproved because the proposed local-model accuracy gate failed. Actual backend CAD publication on an exhausted disk is now proved in D, with its scope identified above. Local accuracy failed the proposed gate and model work remains deferred by the user. An accented private profile without a compatible short alias is unsupported and fails clearly.

Do not convert these limits into passing certification, silently replace local accuracy with cloud accuracy, or infer exact UI timing from backend/fixture results. Existing component results and package revisions remain separately identified in the support matrix.
