# Everyday explanations of verified results

The actual packaged recovery conversation correctly found the latest save
but exposed internal JSON fields. Hazlo's existing system instruction now
asks for a direct review in two to four short sentences, at most 90 words,
without tool preambles or field lists. It gives ordinary-language examples
for saving and pending changes. It preserves explicit requests for code or
technical details and the distinction between historical reports and the
current document.

The change is in the single instruction already used by Claude Code,
Codex and both Ollama routes. There is no new provider, output filter,
rewriter or editor UI. The frontend prompt and bounded history remain
unchanged.

## Actual model check

The [sanitized checkpoint](result-language-2026-10-16.json) records an owned
copy of the earlier three-job Arduino project and its retained conversation.
The [focused example](../../apps/desktop/src-tauri/examples/ollama_recovered_report.rs)
uses the real existing cloud profile and the production program-scoped
tool loop. The existing loopback body recorder retains full requests and
responses locally; only the checkpoint's counts and booleans are published.

The first attempt removes internal field names but produces six sentences
and 139 words, failing the brevity gate. The wording is then tightened to
exclude preambles/lists and set a word bound. A fresh two-turn attempt passes:

- An ordinary request for the latest completed result returns four sentences
  and 71 words without JSON keys, booleans, tool names or opaque identifiers.
- An explicit request for technical details still returns the correct native
  save confirmation, absence of a code edit in that save step and clean state.
- Recorded successful tool results prove that both turns freshly consult
  workflow status and the verified report. Only those read-only tools run.
- No jobs are created. Independent hashes match all seven original and
  copied project files before and after the test. Builder-home paths are
  absent from provider requests. The test process is terminal.

Ten shared Ollama tool-chat tests pass, with one intentionally ignored native
gate. The focused example builds and passes Clippy with warnings denied.
The frontend production build and bounded-context/history check pass.

## Reproduction and limits

Build the focused example with `cargo build -p hazlomexa --example
ollama_recovered_report`. It requires a disposable directory under workspace
`target/`, an owned copied project in `project/`, `conversation/`, and two
UTF-8 request envelopes in `prompt-1.json`/`prompt-2.json`. Those envelopes
come from the existing frontend `agentPrompt`: first an ordinary saved-report
review, then an explicit technical question. Pass that directory and the
owned recorder port as the two example arguments. The recorder forwards only
to the existing local Ollama service. Raw response JSON stays in the owned
directory. This example performs real inference using the existing account;
ordinary Cargo tests do not invoke it.

This is backend model acceptance, not a native-window or installed-app test.
The same instruction reaches the other existing routes in source, but their
real accounts were not tested here. One successful model sample does not
guarantee every future response's style. The verified installer predates this
wording change; another package build is deferred while core work continues.
CADCraft adoption, unavailable native capabilities, provider authentication
and forced-shutdown chat durability remain separately unfinished.
