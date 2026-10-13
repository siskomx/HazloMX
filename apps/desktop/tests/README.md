# Current Windows package acceptance

[Drawing source-first evidence](../../../docs/research/drawing-source-first-2026-10-13/README.md) covers five native discovery cases, selected-source proposal/approval/readback and source/project boundaries, plus existing conversion/circle/loss regressions. Frontend verification now expects 22 Claude/Codex context cases. Serial backend validation passes 116 tests/43 ignored; native UI and installed acceptance of this change remain unfinished.

[PCB native SDK read cancellation/deadline/recovery](../../../docs/research/pcb-sdk-interruption-2026-10-13/README.md) runs from the repository root with `rtk proxy python tests/konnect/verify_blocked_read.py --pcb-sdk`. Its test-only checkpoint holds the actual get_board_info file read after host validation; no release checkpoint is compiled. --pcb remains the separate host-read refusal gate; --workflow remains schematic interruption. The initial parallel timing failure is retained; serial regular validation passes 116 tests/41 ignored without relaxing bounds.

[PCB held-host-read refusal](../../../docs/research/pcb-busy-read-2026-10-13/README.md) reproduces the old synchronous-open cancellation stall and verifies prompt busy-file refusal with source/staging preservation, nonpublication and recovery. Run `rtk proxy python tests/konnect/verify_blocked_read.py --pcb` from the repository root; `--workflow` separately verifies actual schematic cancellation and the full SDK deadline. PCB SDK mutation/deadline and native UI remain outside this gate.

[Relative PCB guide checks](../../../docs/research/pcb-guided-move-2026-10-13/README.md) are included in `rtk proxy python apps/desktop/tests/verify-agent-context.py`: direction/distance gathering, metric/inch/mils, exact precision/bounds, draft-only behavior and pending-question context resets. Native guided interaction and signed-in acceptance remain unverified.

[Guided measured-box checks](../../../docs/research/cadpilot-guided-size-2026-10-13/README.md) run through `rtk proxy python apps/desktop/tests/verify-agent-context.py`: explicit metric/imperial final-size questions, unchanged other dimensions, cancellation/ambiguity/bounds, pending-question resets and native-size versus world-extent preview labels. This is headless frontend fixture evidence, not native engineering or signed-in acceptance.

For connected-chat desktop context, run `rtk python apps/desktop/tests/verify-agent-context.py` from the repository root. It freezes frontend hashes, builds TypeScript/Vite, runs all nine headless browser workflows for both provider callers, and retains twenty selection/draft cases. This is a mocked IPC caller gate; it does not replace signed-in or native/installed conversation acceptance.

From the repository root, build the package test fixtures:

```powershell
rtk cargo build -p hazlomexa --examples
```

If application source changed since the retained installer, run `rtk npm run package` in `apps/desktop` first. Fixture-only changes do not require rebuilding an unchanged installer. Preserve the installer identities for every run.

Provision the identified runtimes and fixtures at the paths required by [prepare-current-package.py](prepare-current-package.py): KiCad/Konnect, CADCraft, FreeCAD/CADPilot, OrcaSlicer, Arduino Agent and Arduino CLI with the private AVR core. These are explicit prerequisites; the preparer does not download or guess them. It also needs the existing Node/Playwright installation and at least 3 GiB available host memory.

Run `rtk python apps/desktop/tests/prepare-current-package.py --online`. Launch the emitted owned Sandbox configuration and let its logon command finish. Runtime mappings are read-only; native tools execute from fresh owned guest copies. The online guest permits the installer to obtain WebView2 when absent. Without `--online`, a fresh guest exercises the offline missing-WebView prerequisite failure.

The authoritative outcome is `output/package-result.json`, together with each engineering result and retained project. Initial UI uses actual installed IPC; it is separate from backend-driven engineering approvals. Require the native UI result and private-profile/policy-cleanup receipt before claiming the combined gate passes. A scoped run may omit engineering fixtures, but must record those omissions and must not infer their acceptance from other results.

Retain exact package/runtime/helper/fixture identities, scripts, source inputs, SDK calls, project artifacts and failures. Upgrade checks need an explicitly retained different previous installer and its identity, even when the application version is unchanged. Stop only the owned test guest after its driver reaches a terminal state. An observation timeout is not a failed job or permission to restart it.

The preparer captures `release-inputs.json` before returning the case directory. Retain that pre-launch manifest and compare its hashes during export. Keep a timed-out prerequisite attempt separate from any explicit resumed run; a successful continuation does not turn the initial attempt into a pass.

These tests do not run inference, upload firmware or control printers. Remaining engineering GUI/picker/cancellation/opening and signed-in conversation checks require their own evidence.
