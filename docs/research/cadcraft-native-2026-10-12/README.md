# Native CADCraft drawing edit and recovery

October 12, 2026. Current desktop/custom-protocol Windows host binary and identified CADCraft CLI 0.5.0. No model inference, installation, authentication, hardware control or live CADCraft GUI mutation ran.

Later follow-up: [schema-2 requested-circle geometry validation](../cadcraft-geometry-2026-10-12/README.md) adds actual native centre/radius readback and new-request downgrade refusal. The desktop observations below retain their earlier schema-1 scope; they do not certify the rebuilt schema-2 desktop flow.

The actual Hazlo desktop creates a fresh owned project through the folder picker, selects the pinned CADCraft executable through its runtime picker, and imports the known metric DXF through the source picker. Proposal preparation is disabled before runtime selection. The original contains one circle, centre (0, 0) and radius 10 mm. The user reviews a source-bound proposal adding a circle at (30, 0), radius 2 mm, with DXF output and no loss consent. Pending controls lock until explicit approval.

Approval runs the native CLI on copies, verifies and publishes the captured original, drawing, PNG and report. The native read-only report shows two circles, extents (-10, -10) to (32, 10), and no warnings. Independent inspection of the ASCII DXF fixture confirms both circles' actual centres/radii; the generated PNG visibly contains both circles. This validates this fixture's geometry, not complex drawing fidelity.

Closing Hazlo, restarting it and reopening the project restores the completed drawing history and all four artifact actions. The actual saved report opens without selecting CADCraft again or regenerating the result. All four accepted artifact hashes match their pre-restart values. The selected original keeps SHA256 `b8d261ace3b289ebf38075a12b764dd4188eadaeaf2597b1447c9fb066aa1982`. The owned test window is closed; unrelated user windows remain untouched. Explorer reveal/result viewing in the CADCraft GUI and native imperial conversion were not exercised.

The new `jobs::tests::actual_cadcraft_owned_process_cancellation` test separately observes a real owned `cadcraft-cli.exe` during an approved dispatch, cancels it, independently verifies PID cessation, refuses result access and publication, preserves the selected original, and reopens one cancelled run. The actual sample passes in 2.23 seconds; cancellation/cessation takes 427 ms. `cancel-evidence.json` records the process and scope. This is backend evidence; exact mutation phase, native Cancel-button latency and full production-deadline behavior remain unverified. Run this process-sensitive gate alone:

```powershell
rtk proxy cargo test -p hazlomexa --lib actual_cadcraft_owned_process_cancellation -- --ignored --nocapture --test-threads=1
```

Validation after the test addition: 89 regular Rust tests pass, with 30 runtime gates separately ignored; desktop/custom-protocol all-target Clippy passes. Production code is unchanged in this turn. The runtime fixture and native result are retained here with source/build hashes in `status.json`, accessibility observations, the exact run/result, DXF files and PNG. Project: `target/native-cadcraft-20261012-01/project`; completed job: `e907ee66-c637-4eb0-9951-688fe74e76fc`.

Native imperial workflows, more formats/documents, live GUI editing, import-inspection cancellation, production timeout behavior, the updated installer and second-environment acceptance remain unfinished. The full multi-integration application goal remains active.
