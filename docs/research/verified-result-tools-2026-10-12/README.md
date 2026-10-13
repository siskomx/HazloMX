# Verified engineering result readback

October 12, 2026. `read_verified_result` completes the shared tool boundary for reading published results. It accepts only a job UUID in the selected project, requires completed history, resolves the operation's report/sketch through the existing artifact API and revalidates the native outputs before reading. Text is bounded to 64 KiB and returned with its SHA-256; file contents remain untrusted data. Konnect additionally parses the verified edited schematic and returns actual component names/current values. No arbitrary path, approval or effect is available through this tool.

Report mappings exist for FreeCAD, CADPilot, CADCraft, KiCad, Konnect and Orca; Arduino compile and Agent edit return published sketch code. FreeCAD's `solid_report` artifact uses the existing verified `result.json` boundary and can also use the desktop's read-only report action. Embedded Claude instructions now require result readback before describing completion or gathering a follow-up change.

Actual official-SDK/helper/desktop-workflow tests pass:

- FreeCAD 1.1.4: reviewed inch proposal produces a solid measured as 12.7 × 25.4 × 6.35 mm. Result readback returns the native geometry report.
- Arduino CLI 1.5.1 / AVR 1.8.8: reviewed Uno compilation publishes the actual sketch. Replacing the published code causes result readback to fail; restoring the exact output restores validity.
- Konnect 0.13.0 / KiCad 10.0.7: reviewed schematic edit is independently verified before publication; readback parses the edited sheet and confirms R1 = 12.34k.
- Reopening the solid/compile project without selecting an executable still allows validated result reads from recovered history. Switching to another project refuses those job IDs. Pending/unpublished and arbitrary-path requests are rejected.

The regular suite passes 86 tests with 28 explicitly ignored runtime/guest gates. All-target desktop/custom-protocol Clippy and TypeScript/Vite build pass. The installed Claude 2.1.285 credential-free stream gate accepts the expanded nine-tool catalog and returns the expected missing-login terminal error; no authenticated model inference is claimed.

Follow-up gate: the actual approved native CADCraft, KiCad, Orca and CADPilot workflows now call the shared result reader and confirm its content matches their verified published report. CADPilot covers inspection/edit-box/add-box/add-cylinder runs. Retained readback responses and source hashes are in the follow-up evidence files. Runtime durations: CADCraft 8.43 seconds, KiCad 21.46 seconds, Orca 13.25 seconds and CADPilot 10.82 seconds. These are backend tests, not native desktop UI acceptance.

`propose_selected_sketch_edit` now optionally accepts `base_job_id`. It resolves only a completed Arduino compile/Agent edit in the selected project, revalidates the published sketch, requires the inspected name/hash and uses that actual code as the proposed copy's original. The selected picker snapshot remains unchanged. The real helper/SDK solid-and-compile test passes proposal from published output, incorrect-hash refusal, source/code review, MCP provenance, cancellation and three-run recovery (two completed, one cancelled). Reopened readback still works and refuses cancelled/foreign jobs. The resulting Agent proposal is not executed by this gate.

The first follow-up test failed because it expected the current job to remain the completed compile after creating and cancelling a new proposal. The corrected assertion verifies the cancelled current proposal and the original completed job in history separately; the passing follow-up duration is 4.97 seconds. No production completion behavior was weakened to pass this check.

Pending: new-tool readback acceptance for Arduino Agent, authenticated execution of a follow-up based on published output, signed-in conversational follow-ups, verified-output targets for other editing integrations, live native editing, native desktop and updated installer acceptance. Existing runtime tests do not prove those wider requirements. The full application goal remains open.
