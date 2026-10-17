# Orca real-chat continuation

The current release image (`2f0d564`) was exercised through the actual Hazlo
window, native project/configuration pickers and existing Ollama cloud profile.
The explicitly selected Orca MCP runtime exposed 53 tools. The initial idea
contained no dimensions; Hazlo suggested Orca and asked where the part was open.
An actual UI-approved native query then reported the open 10 × 10 × 10 mm cube.
Independent read-only API observation agreed, and the saved 3MF stayed unchanged.

The subsequent request for one inch of width, preserving depth and height,
exposed a conversation failure: the agent repeatedly proposed another object
query instead of retrieving the completed result and preparing an edit. Two
queries were approved through Hazlo; the third remained unapplied. No geometry
edit occurred. Private UI text and readback evidence stay outside publication.
Both owned processes were stopped, with the source hash preserved.

The shared native-chat instructions now tell the agent to retrieve the completed
current job before proposing another query solely to recover identifiers and the
readback hash. A previous report remains an observation: the existing native
guard must repeat the query and refuse a changed state before editing. Approval
and post-operation review remain separate; result review cannot initiate writes.

Connection labels also distinguish connected MCP tools from saved-file CLI
workflows. Connecting a tool catalog no longer claims that a document was
inspected. These changes apply to every program using the shared native route.
Prompt-context checks and the production frontend build pass. Fresh real-chat
editing acceptance and a rebuilt installer remain required for this change.

The `d788204` retry confirms the corrected labels in the rebuilt native window,
but the instruction-only change did **not** resolve repeated read proposals.
Its failed observation is retained separately. Inspection of the actual native
result found that it stores the reply and readback hash, but omits the query
needed for the next edit's guard. The shared result tool now returns
`native_tool_followup`: the original query, matching state hash, connection and
catalog identity. The next proposal can use that exact guard; execution still
repeats the query, refuses changed state and requires approval. This adds result
metadata, without predefined program operations or automatic writes. The six
program routing fixture now consumes the returned guard directly and retains
its stale-state refusal check. Real conversation acceptance remains required.

## Corrected release: real conversation passes

Release `76851f1` now passes the actual Hazlo window flow with the existing cloud
profile and the selected 53-tool Orca runtime. The conversation starts with a
sample idea and unknown dimensions. One UI-approved native diagnostic reads the actual
10 mm cube. The next request produces a guarded native scale proposal directly,
without another query approval. Actual UI approval changes its width to
25.4 mm, preserving 10 mm depth/height and its position. A natural-language
follow-up produces another separately approved native proposal, moving it
2.54 mm right while preserving size and the other position axes.

Three native result reports contain no native/tool errors. Independent read-only
API observations match each state within 0.000001 mm. The resized rectangle and
Orca's resize notification are observed in the owned native window. Hazlo reviews
both actual results and distinguishes open-state edits from saving; its final
reply suggests saving the project or exporting the scaled model. The saved 3MF
hash remains unchanged. Both owned apps stop after the test.

The source passes 201 regular Rust tests, Clippy, the six-program native routing
fixture using returned guards, prompt-context checks and the frontend build.
The matching installer builds with unchanged tracked inputs, zero builder-path
matches and the 38-tool release helper check. Its fresh guest lifecycle, other
program conversations and the subsequently requested image/Rea feature remain
separate unfinished checks. Native transform Undo remains unavailable in the
selected Orca build; this test does not claim saving, slicing or print readiness.

KiCad 10 remains selected; live schematic editing and Claude sign-in acceptance
stay pending by explicit user choice. Existing program-specific evidence retains
its original scope in the [status matrix](../native-flow-status.md).
