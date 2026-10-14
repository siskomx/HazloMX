# Real release chat and native approval — October 14, 2026

After explicit user permission resumed native automation, the actual unbundled release app passed a real existing-cloud Arduino CLI conversation through its normal Windows UI. The app SHA-256 is `fbe607afef328e6fc0a73b41f7a2d0550a31f8c9eb01a2ddac8c331c2bf1e7f5`, from installer candidate `6a2f7ef9…`. No debug preselection, scripted model response, backend approval fixture or recording forwarder was used for this UI run. A fresh WebView profile and owned project isolated the test conversation.

## Observed workflow

The initial screen presents the simple chat and collapsed connections, without measurements or an engineering editor. Native Connections and project-folder pickers create the owned project. The provider selector exposes Codex, Claude Code, Ollama local and Ollama cloud; the existing cloud route is selected. Native executable/environment pickers select the identified Arduino CLI and already provisioned AVR toolchain.

The first user message describes a built-in LED sketch without a board or timing. The real assistant consults capabilities, explains the supported Uno scope and asks for board/timing confirmation. No job is created. The next message confirms Uno and half-second on/off timing and asks to prepare a proposal without approval. The persisted job remains AwaitingApproval and has two literal `delay(500)` calls. No HEX exists before approval. The native review shows compile-only scope, original preservation and expandable source code.

Clicking **Aprobar y pedir revisión a Ollama cloud** in the native UI starts actual compilation. The job completes with sketch/HEX/ELF and a validated CLI 1.5.1 / AVR 1.8.8 / Uno report using **924 flash bytes**. The normal completion handoff requests a new assistant turn, which reports the matching timing, output files and flash usage, explicitly limiting the result to compilation without upload or physical behavior verification. The UI turn was not independently wire-recorded; its observed explanation matches the independently read saved report. Separate backend conversation gates record successful tool calls.

A natural-language follow-up requests quarter-second timing from the verified code. Its pending proposal is denied through native **Cancelar**. The persisted state is Cancelled, no cancelled-job artifact folder exists, and all five first-result files remain byte-identical. A fresh request explicitly uses the last completed 500 ms code rather than the cancelled proposal. Its new pending source contains two `delay(250)` calls. Separate native approval completes the second compilation with **922 flash bytes**; the automatic assistant review matches that report and preserves the compile-only limitations.

## Restart and read-only result access

The owned app closes and restarts using the same isolated WebView profile. Native Open selects the same project. Selecting the same AI restores the visible conversation without a restored approval or another model turn. Result history recovers two Completed operations and the intervening Cancelled proposal in reverse chronological order. The newest **Informe de compilación** button opens Hazlo's read-only report dialog with the exact Uno/toolchain/922-byte result. Closing the report returns to history. All **14 persistent project files** match their pre-restart hashes; the ephemeral writer lock is excluded. The owned app is closed after verification.

Computer-use input needed fresh screenshot geometry, popup keyboard selection and stale-window recovery. These are automation limitations, not application failures or claimed additional gates. The first whole-project hash attempt encountered the live exclusive writer lock; the corrected comparison excludes that ephemeral lock and verifies every persistent file. Raw UI trees, conversations, logs, profiles and projects remain private under ignored `target/`.

## Remaining acceptance

This passes one real existing-cloud **unbundled release UI** workflow: native pickers, clarification, review, two UI approvals/native compilations, proposal denial, automatic result explanations, conversation/project recovery and read-only report access. It does not establish installed UI/lifecycle, ordinary-user or all-program/provider parity, running-process cancellation, upload/hardware behavior or live/unsaved editing.

Two isolated host Arduino Agent launches failed readiness before any real conversation or edit. Neither exposed the authenticated 3847 service or created the expected isolated token. The diagnostic retry used explicit private Electron user data and the runtime working directory; the cause remains undetermined. Both owned process trees stopped and the service port is closed. The compiled Agent conversation fixture remains unexecuted; existing direct/backend Agent evidence keeps its original scope.

The fresh current-installer guest package remains prepared and unlaunched. The user explicitly requested keeping that guest test pending after the available-memory check. Native host automation is resumed; the guest deferral is not a new general UI stop. The full application goal remains unfinished.
