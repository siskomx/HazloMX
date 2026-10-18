# Automatic program connection and bundled Rea

Hazlo stays a simple chat. Choosing software, or opening its project after
choosing it, attempts native-tool connection without asking for a JSON file.
Discovery checks absolute PATH entries, standard installation folders and
Windows uninstall records. Relative/current-directory lookup is excluded.
Successful manual portable CLI/MCP selections are remembered as local paths;
their configuration secrets are not copied into that location record.
MCP configuration references are bound to the exact selected project folder;
portable executable locations can be reused across projects.

Hazlo generates its own local connection files without replacing existing
files. Actual installed MCP schemas and CLI help still govern capabilities.
Connection does not approve or execute engineering edits. Existing selected
native connections take precedence. Manual connection remains available.

Rea 6.5.0 and Node 22.23.2 are bundled with the installer. The build pins npm
dependencies in a lockfile and verifies the Node Windows archive SHA-256. The
app launches its bundled Node directly with an isolated Rea profile; it does not
register personal AI clients or run Rea's interactive setup. Upstream licenses
are included. Rea remains software/firmware analysis, not physical photo-to-CAD.
Specialized analysis engines and optional native components can still require
their own installation; a tool catalog is not proof that every engine is ready.

FreeCAD uses an installed CADPilot MCP; CADCraft uses its installed native MCP;
Arduino Agent uses its running authenticated local service; KiCad uses Konnect
and the detected KiCad CLI. Supported native CLI routes cover CADCraft, Arduino,
KiCad and stock Orca. An installed Orca MCP uses the existing localhost Remote
API settings without changing them. Disabled API, LAN-enabled settings, missing
tokens and invalid ports are refused. KiCad 10 live schematics stay deferred.

Focused tests cover relative-path exclusion, no-overwrite configuration,
portable selection persistence without credential copies, stale locations,
Orca localhost requirements, and real bundled Rea catalog discovery. The shared
chat fixture covers automatic attempts across all seven program groups without
automatic edit approval, alongside its existing provider/routing tests.

The initial bundled Rea check exceeded the old 20-second cold-start timeout;
Rea now has a bounded 45-second startup allowance. A later successful discovery
hit a test-cleanup race while the closing process released its profile; bounded
cleanup fixes the fixture. The final real bundled catalog check passes.

Installer and native-window acceptance for this change are still pending.

The first fresh guest (`997d529`) passes installation/update/helper/lifecycle
checks but fails automatic Rea startup. Tauri returns a verbatim Windows resource
path; Node's main-module resolver reports `EISDIR` for its drive prefix. A private
diagnostic in that same guest completes the Rea handshake with the equivalent
normal path. The launcher now normalizes that drive prefix without changing the
target. A focused path test and the real bundled MCP test use the canonical
Windows resource path. The failed candidate is not published; its guest is
stopped and six project files are independently preserved.
