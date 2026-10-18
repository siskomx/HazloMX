# Selected save-folder clarity

Source `4307b36` supplies the selected project directory to all four chat
providers and through the shared `get_workflow_status` tool. Previously the
agent could see a project identifier without its directory, and the real Codex
Save As test required a human folder correction.

Conexiones now labels the full project folder and explains that it holds the
project files and history. Long paths wrap. Native operation review repeats the
current folder alongside the proposed calls. The shared native-agent instruction
distinguishes the project directory from the source document's directory, asks
when the destination is ambiguous, and preserves the original through the
program's existing native save tools. Hazlo remains a simple chat; no save-tool
wrapper or editor is added.

## Checked

- Backend tests: 24 passed, 13 explicitly ignored; the new scoped status test
  checks the current directory, stale connection refusal and isolation from
  unrelated source folders. Clippy passes with warnings denied.
- Chat context, nine workflow fixtures, production build and the shared chat
  fixture pass. All four providers across seven program groups receive the
  currently selected directory, including a copied-folder context switch.
  Visible connection and approval folder labels are asserted.
- The installer was rebuilt from frozen source inputs; package identity,
  binary privacy and the 38-tool release helper check pass.

## Awaiting live acceptance

The owned native test launched both apps but stopped when its desktop/memory
prerequisite failed. A diagnostic retry confirmed both processes were alive
while the prerequisite failed. No chat edit or save was submitted. The real
Codex native Save As regression and fresh Windows guest UI/lifecycle check are
still pending memory headroom. The previous alpha's acceptance retains its
original source scope and is not evidence for this rebuilt package.

Candidate installer SHA-256:
`f939609b0abb6e2aeaddf892652d79868cf2d7e95d9cd244f1631ad3676bacd0`.

KiCad 10 live schematics remain deferred. The Windows alpha remains unsigned.
