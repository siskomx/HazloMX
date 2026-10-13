# Native Konnect blocked-read cancellation and deadline

Two actual Konnect 0.13.0 dispatcher cases passed on Windows 11 Pro build 26200, with the identified KiCad 10.0.7 CLI. The existing NTFS file barrier held the captured schematic's component-file access. The native server was observed alive during that access and ceased before the barrier was released. Both the original fixture schematic and captured copy remained byte-for-byte unchanged.

| Case | Retained sample | Outcome |
|---|---:|---|
| Cancel while the component read is held | 3,033 ms from cancellation request to returned shutdown; 4,418 ms including startup | Explicit cancellation error; owned server ceased before release |
| Leave the component read held | 63,190 ms including startup and shutdown | Unchanged production 60-second tool deadline; explicit unconfirmed-edit error; owned server ceased before release |

Cancellation timing is backend cessation, not UI feedback latency. The SDK's pinned child transport gives its child a three-second graceful-exit window before killing it. These blocked-read samples are distinct from the faster prior startup-only cancellation samples.

`rtk python tests/konnect/verify_blocked_read.py` requires two actual passing tests, both modes, real receipts, observed owned process identities, preservation, and unchanged source/runtime hashes across execution. It runs tests serially, retains stdout/stderr and cannot accept a zero-test invocation. The archived `status.json` records the command and hashes; `cancellation/` and `deadline/` contain the tested fixtures, profiles and receipts. `tested-source/` contains the exact source inputs. The first closed export verified 26 files; `file-hashes.json` also covers this documentation and verification metadata.

The native pair passed, as did all 115 regular Rust library tests (36 runtime tests ignored), all-target custom-protocol Clippy with warnings denied, and the standalone file-barrier self-check. Added code is test-only and exposes the existing barrier to those tests; production timeout/shutdown behavior was unchanged. The delivery installer remains the separately tested unsigned `ab43ac2a…` build.

This proves initialized blocked-read cancellation and the full tool deadline before mutation in the real dispatcher. It does not prove mutation-phase cancellation, PCB-specific blocked access, a pending native CLI child, full workflow failure/publication/recovery on this timeout, or native GUI/approval/Cancel latency. No native UI input, signed-in AI turn, model inference or device/printer control ran. Those gates remain unfinished; the full application goal stays active.

After the native run, the verifier reporting flags were hardened to stay false for failed or partial runs. Isolated failure/partial/complete-pass checks passed without repeating native effects. The exact executed positive-run verifier remains under tested-source; verifier-with-reporting-guard.py records the updated reporting-only revision.
