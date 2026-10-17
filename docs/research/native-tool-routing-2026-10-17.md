# Program-owned discovery and routing

October 17, 2026. Source/debug checkpoint; this change is newer than the retained
installer. Hazlo remains a chat. It does not add another geometry, code, schematic
or slicing editor.

## Implemented flow

Select the program and connect its installed MCP in Conexiones. The connection
accepts one standard `mcpServers` entry or a direct configuration with `command`,
`args` and `env`; local Streamable HTTP uses `url` and optional authorization.
Hazlo launches installed stdio servers directly, without a package runner. It
does not install a server or infer tools from a newer upstream release.

The existing explicitly selected CADCraft live runtime, Arduino Agent connection
and KiCad/Konnect runtimes can supply their native connection settings. Other
installed MCP servers can be selected through a configuration file. The four CLI
program groups also accept an independently selected native executable; their
version does not need to match a legacy preset adapter's version whitelist.

The same route covers FreeCAD/CADPilot, CADCraft, Arduino CLI, Arduino Agent,
KiCad/Konnect and OrcaSlicer:

1. Read the native identity and every paginated MCP tool name. Fetch a tool's
   actual `inputSchema` before preparing its arguments. CLI discovery captures
   the executable's actual help, including OrcaSlicer's Windows console output.
   Further CLI help queries use the same reviewed native invocation route.
2. Refresh the catalog before observations, proposals and dispatch. Bind proposals
   to that connection and the identity/schema digest. A changed catalog requires
   new discovery and a new proposal.
3. Read through `read_native_tool` only when the selected MCP explicitly declares
   `readOnlyHint:true`, `destructiveHint:false` and `openWorldHint:false`. Missing
   hints do not qualify. All other calls, including CLI queries, use the existing
   human approval flow. The installed server's annotations are trusted within
   this selected connection; they are not an operating-system sandbox.
4. Send the exact approved native names/arguments once. Edits require a preceding
   native observation and a matching state digest immediately before dispatch.
   Read the program again and preserve its reply and final observation in a
   schema-17 job report. The agent reviews that report in ordinary language.

When a native connection is active, the agent receives six shared coordination
tools, including discovery, read, proposal, result and status access. The old
program-specific editing wrappers are excluded and refused in that scope. This
lets newly discovered tools and loaded Konnect toolsets use the existing route
without adding another Hazlo operation variant. The global compatibility catalog
has 38 tools; inactive legacy workflows retain their previous scope.

## Evidence

| Check | Result and scope |
|---|---|
| Real installed MCP discovery | CADPilot: 27 tools; CADCraft: 11 tools; Konnect: 21 initially loaded tools. Actual native schemas and identities retained privately. Konnect's native toolbox query also passes. These counts are observed catalogs, not hardcoded limits or a promise that every toolbox is loaded. |
| Real CLI discovery/readback | CADCraft, Arduino CLI, KiCad CLI and OrcaSlicer all return actual help through the owned process runner. OrcaSlicer uses a hidden Windows pseudoconsole with the packaged console helper as its parent. CLI access is explicitly labelled as a file/command workflow. |
| Real native editing | Headless CADCraft discovers native command documentation, executes a line through its native `execute` tool, and reads changed entities through `inspect_drawing`. This is a native backend test, not a visible editor, provider or independent geometry certification test. |
| SDK protocol fixture | Paginated catalogs and arbitrary native schemas/calls across all six program groups; secret redaction, schema drift and stale-state refusal; native error after an effect; cancellation without replay; direct CLI arguments in four groups and native console output. |
| Workflow fixture | All six groups use schema-17 jobs, explicit one-use approval, final report access, stale-state failure, cancellation and journal recovery. Annotated native reads create no editing job; a mutating tool cannot use that read route. |
| HTTP SDK fixture | Local Streamable HTTP initialization, discovery, annotated observation, edit, guarded readback and disconnect pass against a separately owned Python MCP SDK server. This does not replace real Arduino Agent acceptance. |
| Chat/browser fixtures | Four AI choices × six program groups exercise native connection, conversation context, proposal, approval, report review and disconnect. Legacy chat flows and bounded conversation context still pass. These are IPC fixtures, not real provider inference. |
| Source checks | 200 default Rust tests pass; 62 native/external tests remain explicitly ignored in the default run. Selected protocol/workflow/HTTP tests and three real discovery/edit tests pass separately. Desktop all-target Clippy with warnings denied and TypeScript/Vite build pass. |

Initial discovery failures remain in ignored private evidence: CADCraft's help
uses a nonzero exit status, CADPilot needed its already-declared Python dependency
and user-site import path, and OrcaSlicer needed console capture. An initial HTTP
fixture assertion incorrectly assumed structured replies; the test now reads its
actual text-content JSON. No earlier failed attempt is represented as a pass.

## Limits and remaining acceptance

Native replies establish what the selected software reported. They do not
independently certify geometry, manufacturing correctness, saving or live GUI
scope. The pre-edit guard and subsequent mutation are separate calls; a program
must supply its own transactional command if atomicity is required. The route
preserves native arguments rather than introducing a second geometry schema.

Cancellation stops waiting and closes an uncertain MCP session; an accepted
native edit may remain. There is no automatic replay or rollback. Known hardware
control commands are refused, but general native scripting is not an OS sandbox.
Connection credentials stay out of agent configuration/context and reports redact
known connection secrets. Raw logs, catalogs, profiles and captures remain ignored.

New native routes have not yet repeated visible editing acceptance for every
program, real provider conversation acceptance or a rebuilt installer/guest test.
The user's pending Claude test stays pending. No live schematic interface or
OrcaSlicer open-document control is claimed. Historical native/UI and installed
checks retain their original binary and integration scope. The full application
goal remains unfinished.
