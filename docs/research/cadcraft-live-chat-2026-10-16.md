# CADCraft chat edits in its own native window

October 16, 2026. The person explicitly selected the tested patched runtime.
Hazlo now connects that runtime to the existing chat, one-use approval,
verified report and project-recovery flow. Installed CADCraft is unchanged.

The native runtime is pinned upstream v0.7.0 plus the three retained patches
in [the binding trial](cadcraft-native-binding-2026-10-16.md) and
[read-only units correction](cadcraft-readonly-units-2026-10-16.md).
The rebuilt executable SHA-256 is
`9b3d00dfcfb466f39c3a66af1bd05c4e16d68527c3b52d2dc2e09db663438092`.
Hazlo refuses another binary. This remains a separately maintained runtime;
there is no automatic build, download, installation or upstream adoption.

## What is enabled

In Connections choose CADCraft, enter its explicitly configured local control
port and select the tested patched executable with the native file picker.
The runtime must already be open with its drawing and `--control` port.
Hazlo accepts only IPv4 loopback and checks the actual server-side established
TCP connection's process and executable. It retains the process and executable
handles; a failed, cancelled or malformed exchange closes the connection and
requires reconnection, with no automatic write retry.

The chat reads complete bounded Model-space geometry, including unsaved work,
and native millimetre/inch settings. It resolves circle identities internally
and asks for missing measurements in ordinary language. Two typed operations
are enabled: create a circle from an explicit center and full diameter, or move
an existing circle by a signed horizontal/vertical distance. Requested mm/inch
distances are converted once to the drawing's native units. Drawings with
unsupported units, incomplete geometry, active block editing, omitted records
or more than 128 entities are refused. There are additional byte/coordinate
bounds; this is not an arbitrary native-command interface.

Approval dispatch uses the native GUI-thread checked binding, including drawing
identity and full immutable drawing state. It does not infer write safety from
the reduced geometry projection. Native Undo must be enabled without an open
group. Hazlo rereads geometry after the native command and verifies the intended
circle addition/movement and preservation of other observed entities. A separate
schema-16 journal stores a digest-bound report; recovering it grants no approval
or live connection. The existing CADCraft CLI copy workflow stays separate.

## What passed

- [Production backend acceptance](cadcraft-live-chat-2026-10-16/backend.json):
  a 1 mm move, a further 0.1 inch move to 3.54 mm, circle creation, three native
  Undo steps, approval replay refusal, pre-dispatch cancellation, report recovery
  and refusal after an unprojected layer-lineweight change. Clean units queries
  preserve native state. These approvals were backend fixtures.
- [Five-turn real cloud conversation](cadcraft-live-chat-2026-10-16/cloud.json):
  missing-parameter clarification, metric edit, verified-result/current-state
  review, inch follow-up and final review. It uses the existing configured route,
  with 29 recorded HTTP exchanges and backend fixture approvals. Private traces
  remain ignored; builder-profile paths are excluded from model requests.
- [Actual owned Windows UI](cadcraft-live-chat-2026-10-16/native-ui.json):
  native runtime picker, ordinary-language missing-distance question, two actual
  Hazlo approval-button clicks and automatic result reviews. Independent native
  reads show 0, 1, 1 and 3.54 mm before/after the approvals. Circle size is
  unchanged, both reports open as read-only text, and the final CADCraft window
  visibly contains the shifted circle and unsaved marker. Its right edge is
  partly outside the viewport; precise geometry is proved by native readback.
- [Regular checks](cadcraft-live-chat-2026-10-16/checks.json): 189 Rust tests
  pass, 54 separately scoped tests remain ignored, Clippy passes with warnings
  denied, desktop/helper and TypeScript/Vite builds pass, and four-provider
  frontend fixtures cover native selection, scope, approval, review and explicit
  switching back to a saved copy. Fixtures are not real-provider acceptance.

All owned native processes are stopped. Source DXF bytes and host CADCraft
settings remain unchanged. The owned UI fixture needed its own Windows known
folders created before startup/picker use; this does not change host settings.
Backend acceptance caught the missing schema-16 audit range, now covered by
a journal regression.

## Remaining scope

Other drawing commands, native save/export, multiple documents and current
installer/guest acceptance of this integration remain unfinished. The app does
not claim a drawing was saved. Old installers do not understand schema-16 jobs;
keep this test project with a compatible build. The real Claude/Codex gates
retain their existing sign-in and pending-test limits. Live schematic and
slicer work remains separately unfinished in [current status](../native-flow-status.md).

The retained UI executable identifies the actual source/debug acceptance run;
it is not an installed-package claim. Raw profiles, paths, native documents,
conversations and captures remain under ignored test directories.
