# Optional native device diagnostics

Hazlo now has separate TShark, USBPcap and pySerial program scopes in the simple
chat. Planning exposes their purposes and upstream repositories. Selecting a
scope detects installed tools, generates an app-local connection configuration
and routes reviewed calls through the existing native transport and journal.
It does not create another editor in Hazlo.

## Detection and optional setup

- TShark and USBPcap discovery uses PATH, known program directories, uninstall
  locations, Wireshark's extcap directory and the existing portable-folder catalog.
  The selected executable is pinned and its actual help supplies the CLI catalog.
- pySerial discovery checks Python installations for the real library, including
  Hazlo's optional private environment. Its worker runs with isolated Python;
  the agent cannot submit Python source, module names or arbitrary URLs.
- If missing, a UI action downloads the pinned official package and verifies its
  SHA-256. The reviewed versions are Wireshark/TShark 4.6.9, USBPcap 1.5.4.0 and
  pySerial 3.5. Wireshark's hash matches its published signatures file; the USBPcap
  pin was established from the official GitHub release asset; the wheel hash
  matches PyPI metadata. A changed or corrupt package is refused.
- TShark/USBPcap setup opens the verified official installer for the user. Download
  completion does not claim successful installation or driver readiness. The user
  completes choices, elevation and any restart, then requests detection again.
  pySerial uses a private virtual environment with an existing Python interpreter;
  its verified wheel is installed without dependencies or index access. A machine
  without a compatible Python still needs that interpreter supplied separately.
- Downloads are optional, cancellable and cached; no packages are fetched merely
  because a program is selected. Repository release checks report updates but do
  not replace runtimes or install drivers automatically.

## Execution scope

TShark and USBPcap retain native CLI routing rather than a fixed list of protocol
operations. The catalog includes the actual runtime help and diagnostic limits.
TShark live capture requires an explicit interface and a native duration limit of
at most 30 seconds. Script-loading/export shortcuts and USB registry initialization
are refused. USB capture requires a root hub and explicit device addresses; it
cannot silently capture every device on the hub. USBPcap's implicit elevation is
blocked when the current session lacks capture privileges, because an elevated
worker could escape the owned process job and cancellation. Saved captures can
still be analyzed without that privileged acquisition path.

pySerial exposes library identity, enumerated local ports and bounded binary reads
(up to 30 seconds / 32 KiB). It sends no bytes and does not accept socket/RFC2217
URLs or an unlisted port. Opening a serial port can still affect a device's control
lines, so byte capture always requires review and states this limit.

Diagnostics use one reviewed call in `readback`, with no change calls or guard
that would repeat a capture. Cancellation, executable identity and retained report
validation reuse the existing workflow. New capture output files must be inside
the project and are atomically reserved without replacement; successful native
completion records their size and SHA-256 up to the report limit. The hash is an
observation at completion, not physical-device validation. Interrupted/timed-out
captures can remain partial; stopping does not undo their side effects.

## Validation

The real Windows probe uses an owned project, extracted official native binaries
and a private pySerial environment. It performs actual optional downloads with
hash/cache validation, automatic detection, configuration generation, native
catalog discovery, human-approval state transitions, execution and verified-result
reading for all three program scopes. It decodes a generated Ethernet/IPv4/UDP
packet with TShark, queries USBPcap's extcap version and discovers ports with the
real pySerial package. It also verifies TShark file output hashes, preservation of
an existing output, single-use approvals, cancellation before approval and refusal
to repeat a diagnostic call as a guarded write.

The Python worker fixture checks bounded binary reads, closure, no transmission,
invalid/unlisted port refusal, control-line settings and no code/URL injection.
Chat IPC fixtures cover all four AI choices and ten program groups, including
optional setup success/cancellation and no implied approval or connection merely
from downloading an installer. Provider scope checks and the regular Rust suite
remain in place.

These are source/build and native-protocol checks. No capture driver was installed,
no user traffic was collected, no physical USB/serial device was opened and no
installer's interactive installation flow was accepted during testing. A rebuilt
desktop/installer is required before the installed Hazlo includes these changes.
