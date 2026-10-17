# OrcaSlicer live native MCP support

October 17, 2026. The person selected an isolated MCP-enabled OrcaSlicer runtime,
kept installed KiCad 10, and left live schematics and Claude sign-in pending.
Hazlo remains a simple chat. This integration reuses the existing generic native
MCP connection, discovery, proposal, approval, readback and result review route.
No Orca operation wrapper, editor or additional chat form is added.

## Selected native software

The [maintained OrcaSlicer fork](https://github.com/MaxEllis/OrcaSlicer/releases/tag/v2.4.2-mcp.13)
provides a token-authenticated local control API. Its Windows portable archive
matches the release's SHA-256. The separately installed
[orcaslicer-mcp 0.1.17](https://github.com/MaxEllis/orcaslicer-mcp/releases/tag/v0.1.17)
exposes 53 native tools, including schema discovery for settings, objects, plates
and slicing. Its API and server supply those operations; Hazlo routes their
actual schemas. Stock OrcaSlicer 2.4.2 remains the separately supported CLI path.

Both native projects are added to Hazlo's existing scoped source catalog and
release checks, alongside stock OrcaSlicer. Repository metadata still does not
install software, grant execution capability or prove runtime compatibility.

## Native Windows acceptance

The actual MCP SDK route discovers the server as `orcaslicer` version `0.1.17`
and all 53 tool schemas. Test-harness approvals execute these reviewed calls in
an owned, visible portable runtime with a disposable model:

- Read the actual open 10 mm cube, including stable object identity and bounds.
- Scale its width to 25.4 mm, preserving 10 mm depth and height.
- Move the same object 2.54 mm to the right, preserving its dimensions.
- Observe the changed geometry in Orca's window and verify native readback.
- Change layer height from 0.2 to 0.15 mm, observe that field in Orca, and
  preserve the queried wall count and infill.
- Restore both the geometry and layer height with separately reviewed native
  calls. The final geometry exactly matches its initial native reply, and the
  saved 3MF bytes stay unchanged. No user preset files are created.

The first follow-up assertion compared floating-point widths exactly. It is
retained as failed test evidence; the write is not replayed. Independent checks
use a 0.000001 mm numerical tolerance. Production stale-state guards continue to
compare exact native replies and are unchanged.

**Native Undo limit:** the tested fork's transform handler changes the live model
without creating an Undo snapshot. A native UI Undo attempt leaves the transformed
object unchanged. Restoration above is an explicit inverse edit, not native
Undo. Hazlo's runtime discovery guidance records that limit. Do not promise
automatic rollback or atomic multi-call edits.

The person enabled the Remote API and disabled LAN access in the owned window.
The test checks localhost-only configuration before connecting. Credentials are
read locally into the private MCP configuration and never included in agent
context or published evidence. No printer, device or hardware tool is used.
Both owned processes stop after the checks.

## Connect from Hazlo

Open the explicitly selected MCP-enabled OrcaSlicer build and enable its local
Remote API yourself. Leave LAN access and script/G-code/connection edits disabled.
Install the upstream MCP server once in an isolated environment; configure its
installed executable with `ORCA_API_URL` set to the local API and `ORCA_API_TOKEN`
set locally to that instance's token. Do not put credentials in the chat.

In Conexiones choose OrcaSlicer and **Conectar MCP nativo…**, then select that
single-server configuration. The optional catalog lists the discovered native
tools. The chat can then inspect, propose changes, read results and discuss the
next step. Hazlo does not run package installers such as `uvx` on connection;
select the already installed MCP executable or its isolated Python interpreter.
This is the existing connection flow, already shipped in the `4173250` package.
The two additional upstream release sources require the updated source build.

These are real native software and observed-window checks with test-harness
approvals. They do not repeat a signed-in AI conversation, Hazlo UI approvals or
installed-package acceptance for this runtime. Existing shared-route fixtures
retain their separate scope.

## KiCad schematic choice

[KiCad's native API documentation](https://dev-docs.kicad.org/en/apis-and-binding/ipc-api/for-addon-developers/)
places schematic plugins in the KiCad 11 development line. Current KiCad master
also implements schematic item mutation handlers. The existing
[KiPilot MCP](https://www.kipilot.org/docs.html) exposes that native surface, but
requires a compatible KiCad 11 nightly. The person chose to keep KiCad 10 and
leave this live gate pending. No nightly is installed, no existing schematic is
reloaded or rewritten, and saved-file editing is not counted as live support.
