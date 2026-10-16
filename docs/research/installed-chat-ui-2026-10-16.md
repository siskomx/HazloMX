# Installed chat: native Windows UI checkpoint

The same package built from `542591f` now opens its actual release window in a
fresh disposable Windows guest. Its installer, installed app and helper match
the [verified package identities](native-chat-installer-2026-10-16/package-identity.json).
This is a basic installed-UI checkpoint, not the complete conversation,
engineering approval or live-document acceptance gate.

The previous backend guest was no longer running when work resumed. Its
receipts remain intact. The host desktop was independently confirmed unlocked,
and available host RAM measured 4,169 MiB before preparing the UI-only guest.
It used read-only installer inputs and a separate writable evidence folder.
No engineering runtimes, account credentials or conversation history were
copied into this guest. WebView2 installed through the normal prerequisite.

Native computer-use observations and actions verified:

- The release starts with the chat and collapsed connections, inviting an idea
  without requiring measurements or internal object names.
- Connections default to Codex and “help me choose.” The dropdowns show all
  four existing AI routes and six engineering program groups.
- Cancelling the create-project folder picker leaves the app without a
  selected project. Selecting a fresh disposable folder creates the project
  through the real installed application.
- Cancelling the open-project picker preserves that selected project.
- Returning to chat retains its initial message, shows the disconnected-AI
  guidance and has no engineering forms or pending change approval.
- Closing the owned Hazlo window returns to the guest desktop.

Text injected through the outer Sandbox window did not appear in the folder
field, including after refocusing and one retry. Folder navigation and mouse
selection worked. No application text-entry bug is established by this remote
input limitation. No chat message or AI authentication was submitted.

Independent guest checks then confirmed zero jobs, an actual initialized
project, clean app closure, successful silent uninstall and absent current-user
registration. Every project file and the independent original-file sentinel
survived unchanged. A host-side hash comparison verified all three exported
project files against the guest's post-test manifest. The exact owned guest
was stopped; the Windows Sandbox inventory then reported no running guests.
Raw captures, paths, account metadata and logs remain private under `target/`.

The [sanitized receipt](native-chat-installer-2026-10-16/installed-ui-checkpoint.json)
distinguishes this checkpoint from the still-unfinished signed-in conversation,
engineering approval/result/recovery, cancellation during execution and native
follow-up/export UI checks. The application goal remains active.
