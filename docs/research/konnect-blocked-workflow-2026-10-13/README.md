# Approved Konnect interruption and recovery

Two actual schematic Workflow cases passed on Windows 11 Pro build 26200 using Konnect 0.13.0 and KiCad 10.0.7. Each case copied four fixture files into a new owned project, proposed the R1 value change from 10k to 12.34k in inch mode, consumed explicit approval and began production execution. An NTFS oplock held access to the staged schematic. The native server was observed alive during the hold and ceased before the barrier was released.

| Case | Retained result | Persisted outcome |
|---|---|---|
| Cancel during held access | Backend Cancel returned in 4 ms; native shutdown completed in 3,062 ms; overall execution 13,431 ms | Cancelled, idle, recovered one cancelled run |
| Leave access held | Overall execution 72,775 ms, including runtime validation, startup and shutdown | Failed with the production 60-second unconfirmed-edit error; idle, recovered one failed run |

Both tests reject approval reuse, verify both artifact APIs refuse unpublished results, find no publication directory or accepted result.json, compare the working schematic to the expected relocated copy, preserve all four original files, and recover history with files_verified false. Timing is backend timing, not native UI response latency. Runtime checks and staging happen before the SDK's unchanged 60-second operation deadline.

Reproduce with `rtk proxy python tests/konnect/verify_blocked_read.py --workflow`. The verifier captures source/runtime hashes before execution, requires exactly two actual passing tests and both receipts, checks those hashes afterward, and retains stdout/stderr. This run passed twice: the initial direct test invocation and the retained strict verifier run. Only the latter pair is archived here. All 115 regular Rust library tests passed (38 runtime tests ignored); all-target custom-protocol Clippy passed with warnings denied.

This closes approved schematic Workflow cancellation/deadline, publication refusal and restart-state recovery at held file access. It does not establish the exact SDK mutation phase, PCB-specific interruption, a pending native KiCad CLI operation, native approval/Cancel interaction, installed-package execution of this gate or signed-in conversational acceptance. Native UI automation remained stopped. Production behavior was unchanged; the delivery installer remains the separately tested unsigned ab43ac2a build. The full application goal remains active.
