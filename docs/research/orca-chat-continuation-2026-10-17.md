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

KiCad 10 remains selected; live schematic editing and Claude sign-in acceptance
stay pending by explicit user choice. Existing program-specific evidence retains
its original scope in the [status matrix](../native-flow-status.md).
