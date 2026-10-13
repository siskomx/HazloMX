# Schematic change/readback/follow-up through the actual shared tools

October 12, 2026. Konnect 0.13.0, KiCad CLI 10.0.7, release stdio helper and official rmcp 3.5.1. The existing component proposal tool now accepts paired optional `base_job_id`/`result_sha256` fields. Older requests still edit the explicitly selected original. Follow-up requests explicitly target a completed verified Konnect output in the same project; they do not switch the desktop's original selection.

The production workflow revalidates the completed report and all native artifacts, checks the caller's report hash, holds the report and edited dependencies read-only while capturing a fresh copy, derives the selected component's actual current value and prepares the existing schema-10 MCP approval preview. No arbitrary path, script, approval token or native document command is accepted. Every follow-up uses the same one-use human approval, typed Konnect execution, independent KiCad netlist readback, validated nonreplacing publication and recovery as the first edit.

The actual helper/SDK fixture passes two approved changes: R1 **10k → 12.34k**, followed by **12.34k → 22k**. The second captured original comes from the first saved output. All four source files (schematic, project, board and worksheet) are captured for the follow-up. Actual KiCad netlists confirm the values; prior output bytes remain unchanged. Inspecting the original picker selection still returns 10k. All four original hashes independently match the source fixtures.

Stale report hashes, a changed edited schematic and a changed worksheet dependency are refused before a new proposal is created. Both completed jobs recover after reopening and the final report passes native-artifact validation. A regular regression test rejects missing paired context, nonexistent results and arbitrary source paths without creating jobs/history.

Validation: **88 regular tests pass**, with 29 separately ignored runtime gates. The extended actual helper/SDK/native fixture passes in **19.42 seconds**. All-target desktop/custom-protocol Clippy passes. The installed Codex protocol gate is rerun separately against this catalog; it does not start a model turn or execute these schematic edits.

Retained `konnect-readback.json` and `konnect-followup.json` bind the first/final results to the fixture. Both native reports and KiCad netlists, original hashes and source/helper/runtime hashes are retained beside this file. Probe root: `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-168b97f6-4772-4a8a-873c-010fedd62c03`. Temporary bridge tokens and credentials are excluded.

Reproduce with the release helper present beside its build output:

```powershell
rtk cargo test -p hazlomexa --lib
rtk proxy cargo test -p hazlomexa --lib host_tools::tests::actual_mcp_proposal_keeps_desktop_approval_and_verification -- --ignored --exact --nocapture
rtk cargo clippy -p hazlomexa --all-targets --features desktop,custom-protocol -- -D warnings
```

Scope remains selected-sheet value editing on verified project copies. Signed-in natural-language reasoning, native follow-up UI, live editing of an already open user document, broader schematic edits, PCB native IPC, cancellation/deadline coverage and updated installed acceptance remain unfinished. The full application goal stays active.
