# Hazlo investigation skill

The user requested a reusable skill that researches public documentation, explores
creative engineering routes, makes practical plans and works with Rea. They
explicitly requested independent Claude Fable and Sol reviews at maximum effort.

The candidate is
[hazlo-investigate](../../apps/desktop/src-tauri/resources/skills/hazlo-investigate/SKILL.md).
It applies to unfamiliar repair, recreation, interoperability and adaptation work.
Ordinary edits remain direct; no extra interface or investigation wizard is added.

## Decisions the skill changes

- An unsupported project eliminates that route, not the entire user objective.
- Unknowns lead to a targeted public lookup, native inspection or small falsifiable
  experiment. Missing firmware does not prevent all other research.
- Alternative routes preserve the user's constraints. Untested adapters or
  replacement designs remain hypotheses until relevant evidence supports them.
- Observed results, documented claims and hypotheses remain distinct. Exact model,
  revision, region and version matter where they affect compatibility.
- Rea uses its installed catalog, actual schemas, available artifacts and engines.
  Recorded traffic, packages and app behavior can provide evidence alongside
  binaries. Incomplete output is not proof of absence; retained evidence should
  be reused where possible.
- Unavailable research and missing or contradictory readback remain explicit
  limits, never findings or successful repairs.

## Delivery and enforced boundaries

The shared system instructions embed the skill at compile time with include_str.
Claude, Codex and Ollama already consume those same instructions. This supplies
the skill inside Hazlo without relying on personal agent skill registration or
adding a provider-specific loader. Available provider tools still determine
whether web research or a native action can actually run.

The existing short-summary instruction now applies to completed operations;
requested investigations and plans may include enough evidence and tradeoffs
to be reviewed. The native frontend instruction has the same scoped exception.

The skill does not expand execution permissions. Hazlo owns the existing proposal,
human approval, dispatch and verified-readback flow. Rea does not replace that
approval UI. Analysis may create workspaces or start processes; the actual
read-only classification remains authoritative. A claimed low-risk or
auto-reverting operation cannot bypass the enforced proposal boundary.

Rea's installed workflow guidance was consulted for artifact identity, retained
evidence, coverage limits and missing-engine routing. The skill does not copy
upstream setup commands or hardcode a tool count, engine or Rea tool names.
See [Rea upstream](https://github.com/morluto/rea); live installed schemas take
precedence over examples from newer repository versions.

## Independent review and behavioral checks

Sol was invoked as gpt-6.1-sol with max reasoning. It independently reviewed the
shared instruction callers and native proposal boundary, then approved the final
skill and integration. Its simulated decisions covered:

1. A working device needing cloud-free scheduling, local networking allowed,
   no firmware, and an upstream integration that excludes the model: continue
   exact-model public research without declaring impossibility or demanding teardown.
2. An authorized unknown firmware artifact, renamed native tools, a workspace
   operation requiring approval and an unavailable preferred engine: discover
   actual capabilities, prepare the existing proposal where required, and claim
   no invented architecture, offsets or successful fallback analysis.
3. An ordinary CAD move with direction missing: inspect the selection and ask
   only for the missing direction; no forced research or redesign.

These are simulated agent decisions, not hardware or native execution tests.
Claude was invoked through the installed client with model fable and effort max.
Its draft/final review metadata reports claude-fable-5-1; the initial independent
proposal also reports auxiliary claude-opus-4-8 usage. It requested clearer
query/change proposal distinctions, non-physical readback scope, explicit handling
of unavailable search and failed readback, and a prohibition on fabricated citations.
Those changes are included. Its final verdict explicitly agrees conditional on
adding the word citation to the never-invent list; that exact edit is applied.
Sol separately agrees with the final boundary and evidence rules. The main
implementer agrees with the resulting skill; no new execution mechanism or
permission tier was adopted.

The skill passes the Skill Creator validator. The library suite passes 215 tests
(64 environment/hardware checks explicitly ignored), library Clippy with warnings
denied passes, frontend typechecking/build passes, and existing chat-context and
design-flow fixtures pass. The latter checks nine workflows, provider context,
history recovery, questions, cancellation, approvals, units and escaped previews.
The production chat IPC fixture also passes 28 native catalog routing/review cases
across four AI choices and seven program groups, including approval, native
readback, cancellation, stale events, report-only actions and question privacy.
The desktop binary compilation check passes. These fixture checks do not establish
new live engineering acceptance or guarantee that every model follows the skill.

The installed alpha.5 release is unchanged. No new installer or physical repair
acceptance is claimed by this skill-design work.
