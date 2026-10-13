# Local agent bridge evidence

The desktop owns the existing Workflow and HostTools. A local, remote-client-rejecting Windows pipe authenticates before starting the official rmcp 3.5.1 server protocol. The standalone release `hazlomexa-tools.exe` relays stdio to that pipe; it does not select projects, own a writer, approve edits or run external engineering commands. Desktop controls enable/revoke a temporary configuration. Each session remains bound to its initial project. The installer build now compiles and includes the helper resource; installer execution is unverified.

Passing checks:

```powershell
rtk cargo build -p hazlomexa --release --bin hazlomexa-tools
rtk cargo test -p hazlomexa --lib
rtk proxy cargo test -p hazlomexa --lib actual_stdio_helper_and_idle_revocation -- --ignored --nocapture --test-threads=1
rtk proxy cargo test -p hazlomexa --lib actual_mcp_proposal_keeps_desktop_approval_and_verification -- --ignored --nocapture --test-threads=1
rtk cargo clippy -p hazlomexa --all-targets --features desktop,custom-protocol -- -D warnings
rtk proxy node apps/desktop/tests/design-flow.mjs
```

81 regular tests pass; 26 ignored entries have separate prerequisites/scope. The regular bridge test refuses a wrong token and a remote endpoint, performs SDK initialization/list/status through the pipe, refuses later-project data and observes revocation. The release helper test lists/reads tools through the separate process, closes on idle revocation and independently checks the helper PID ceased within five seconds. The actual native edit test now uses the separate release helper, pipe and SDK before the one-use desktop approval; Konnect/KiCad verification, publication, original preservation and recovery all pass. The retained run root is `C:/Users/USER\AppData\Local\Temp\hazlomexa-host-tools-18c54155-c83b-4379-88b4-9417f5dd648a`.

Browser fixtures verify enable/revoke configuration controls, literal JSON/TOML rendering, clearing the temporary token display and polling an externally prepared proposal into the human review panel. Clippy and frontend compilation pass. No model inference, paid API call, credential inspection or global client configuration change was used.

Unverified: actual Claude Code/Codex configuration and authenticated turns, embedded provider conversation, remaining engineering tools, live mutation cancellation/reconciliation, native source/connection UI and updated installer/second-environment acceptance. This is a working relay/protocol/native-edit gate, not a certification of either AI client.

Retained [scoped status, release helper hash and source hashes](status.json), [recovered run](run.json), [published result](Artifacts/result.json), [original sheet](Artifacts/Original/test.kicad_sch), [edited sheet](Artifacts/Edited/test.kicad_sch) and [native readback](Artifacts/Netlist.xml). Temporary connection tokens, private profiles and agent credentials are not included.
