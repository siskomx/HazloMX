# Automatic connection to existing portable programs

The user selected the existing portable engineering runtimes for this check.
Windows has no installation records or Start-menu entries for those copies.
They were registered in Hazlo's private, program-scoped folder catalog; their
locations are not hardcoded in the application or published here. Discovery
uses these folders before its normal installed-program search. It does not
scan the entire disk. Relative and missing directories are ignored.

Choosing a program generates a new project connection profile from those
locations. The CLI fallback uses the same discovered folders. This preserves
the simple chat and actual native MCP schemas/CLI help; it adds no engineering
operation wrappers or automatic edit approvals. Registered portable folders
must remain available. Moving or removing them requires updating registration.

CADPilot's upstream console entry point was packaged locally against the
existing tested source and dependencies. No new downloads or global Python
installation changes were needed. Orca uses the previously selected portable
API profile with LAN access already off; no security setting was changed.
Its saved preferences append an MD5 comment outside the JSON document. The
parser now accepts that saved format and has a regression check for it.

Real backend discovery and connection checks pass for all seven selections:

| Selection | Connection | Discovered tools |
|---|---|---:|
| FreeCAD / CADPilot | MCP | 27 |
| Selected patched CADCraft | MCP | 12 |
| Arduino CLI | Actual executable help | 1 generic CLI tool |
| Arduino Agent 0.7.1 | Authenticated local MCP | 4 |
| KiCad 10 / Konnect | MCP | 21 |
| Selected Orca MCP runtime | MCP | 53 |
| Bundled Rea | MCP | 207 |

Every check creates a connection file. No token, authorization header or
credential field is persisted in those generated files. Agent initially fails
while its service is closed; starting the existing Agent in its normal profile
then passes discovery and connection. Connecting a catalog does not establish
that a native editor is open or that a live engineering edit succeeds.

Five focused setup tests pass, including program-scoped portable folders,
separate configurations for two projects, invalid/missing paths and saved Orca
preferences. Clippy with warnings denied and the desktop build pass.

The installer from source `320ede2` builds with 6,752 tracked inputs unchanged
and no builder-profile matches in either executable. Its 38-tool release
helper check passes. The updated candidate is installed locally; application,
helper and bundled Rea files match the retained package, startup succeeds and
the registered portable catalog survives the update. Installer SHA-256:
`a04754170d90b05fcf18762e042f49b30b7a72476febef649fbb964796127135`.

Native-window dropdown acceptance is pending a Windows Security prompt for
Arduino's mdns-discovery, which requires user handling. The extra owned debug
app and its development server were stopped to release memory. The installed
candidate and Agent remain running. This candidate is not yet a published
alpha release, and no new clean-guest lifecycle check is claimed. Alpha 4's
separate guest evidence retains its original source scope.

KiCad 10 live schematics remain deferred. This pass does not repeat model
conversations, engineering geometry checks, hardware uploads or physical
printer validation. The installer remains unsigned.
