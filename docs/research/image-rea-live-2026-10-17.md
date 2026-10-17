# Image-to-CAD and Rea conversation acceptance

Hazlo remains a chat. The agent discovers the connected program's tools,
prepares native calls for approval, and reads back the result. No geometry editor
or new fixed operation wrapper was added for these checks.

## Image to native FreeCAD

The release from `ac77461` used the existing vision-capable Ollama cloud profile
and a disposable, synthetic washer reference. Through the actual Windows image
picker, Hazlo displayed the reference and sent it with an ordinary-language idea.
The model identified a washer-like disc with a central hole, suggested FreeCAD,
and explicitly asked about purpose/scale rather than assigning dimensions to the
image. The Windows helper could not click the WebView-owned image dialog; the
user completed that one Open action. The remaining chat approvals were exercised
through owned native windows.

After confirming a demonstration part with 1 inch outside diameter, a 1/4 inch
hole and 2 mm thickness, the chat discovered CADPilot's 27 tools. It proposed an
initial document query, then a native batch creating two cylinders and a Boolean
cut. Both requests required UI approval. Hazlo read the final native measurement
and explained the result without claiming independent geometry certification.

A separate native query confirmed:

- One valid solid, 25.4 mm outside diameter and 2 mm thickness.
- Cylindrical face radii of 12.7 mm and 3.175 mm, confirming the 6.35 mm hole.
- Volume matching the annular-cylinder formula within the native query precision.
- The original 20 × 15 × 10 mm support remained unchanged, with volume 3000 mm³.
- The washer was visibly present in FreeCAD. This test did not save/export it or
  establish manufacturing tolerances, material properties or photograph fidelity.

The initial preparation reached the old eight-round tool limit without editing.
An explicit continuation produced the approved creation. Rea's initial discovery
also reached that bound. A private trace showed valid schema queries consuming
the budget, plus a recoverable wildcard-name mistake. Source `753fafc` clarifies
omitting `tool` to list names and allows 16 bounded rounds. The regression checks
that longer discovery reaches a still-unexecuted approval, while endless reads
stop. No automatic approval, write retry or unbounded tool loop was introduced.

## Rea through the rebuilt chat

The actual `753fafc` release connected the separately selected Rea 6.5.0 runtime
using its own compact schema settings. Hazlo displayed all **207 native tools**.
The same release corrects Rea inheriting OrcaSlicer's legacy source buttons;
actual native-window observation and four-provider browser fixtures confirm Rea
has its own MCP controls only.

An ordinary-language request asked for a static summary of an owned JavaScript
sample without executing it or changing its sources. The agent discovered the
native schema, prepared `analyze_javascript_application` with summary delivery,
bound a guard to Rea's evidence query and used `get_evidence_bundle` for readback.
After explicit UI approval, the native job completed. Hazlo read the report and
explained three relevant files, two JavaScript files parsed without errors,
25 syntax-tree nodes, four findings and partial semantic coverage with five
unresolved points. Independent hashes confirmed all three input files unchanged.

This covers the selected static analysis flow, not every Rea tool, runtime
instrumentation, external analyzer dependency or software-reconstruction claim.
Rea is a software/firmware analysis integration; physical photo reconstruction
continues to use an image-capable model and native CAD tools.

## Windows package

The `753fafc` installer SHA-256 is
`51934fe574ec2428116f0f78e2b0f6177de8a85140c2567b66233dec834df8ce`.
Its build preserved 6738 tracked inputs; application/helper scans found no
builder-profile paths, and the release helper passed its 38-tool protocol check.
Fresh Windows guest checks pass for current-user installation, update from the
previous build, matching installed application/helper hashes, both 38-tool helper
checks, application startup and uninstall. Original project files and application
data survive. Three original project files were independently exported and hashed.

The actual installed window also passes simple-chat startup, the visible image
control, four AI choices, seven program groups and Rea's corrected MCP-only
controls. Mouse-driven native picker selection opens a disposable project, enables
image attachment and survives another picker being cancelled. All three files of
that separately selected project also survive uninstall and independent export.
The guest is stopped. Remote text injection did not reach the guest picker; an
owned fixture in Documents allowed normal mouse selection instead.

These guest checks cover installation, UI and preservation. They do not repeat
signed-in inference, every engineering workflow or the ordinary-user token suite
inside the guest. The real provider/native conversations above are separately
scoped host tests using the release images. No credentials were copied into the
guest and no Claude sign-in was attempted.

The current source passes 205 regular Rust tests, Clippy with warnings denied,
production frontend build, bounded-context and image/privacy fixtures, and 28
native chat IPC approval/review cases across four AI choices and seven program
groups. The 62 external/native tests remain explicitly ignored by the regular
suite; the actual conversations above are separate acceptance evidence.

Claude sign-in and real Claude image acceptance remain pending. KiCad stays on
version 10, with live schematic editing deferred by the user's choice. Raw image,
conversations, configurations, identities and guest evidence remain private in
ignored test output; they are not publication artifacts.
