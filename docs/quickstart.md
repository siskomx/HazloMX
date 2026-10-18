# Use Hazlo Mexa

Hazlo is a chat. The connected programs own their editors and documents.

1. Open or create a local project in **Conexiones**. **Carpeta del proyecto**
   shows its full folder; project files and chat history are kept there.
2. Choose your AI. Hazlo automatically checks installed Codex and Claude Code
   clients. Claude uses its existing sign-in. Codex requires **Conectar cuenta**
   for Hazlo's scoped connection. Manual client selection is available if needed.
3. Describe what you want to create or change, and what it is for. You can start
   without measurements, component references or property names. The chat helps
   choose the program and asks for missing information. Use metric or imperial
   units naturally.
4. Open your document in that program and choose it in **Conexiones**. Hazlo
   detects supported installed MCP/CLI tools and creates its local connection
   files. It checks PATH, standard installation folders and Windows installation
   records. Rea and its Node runtime are included with Hazlo. A manually selected
   portable CLI or MCP configuration is remembered for future connections.
   Registered portable runtime folders also work without a Windows installer
   entry; Hazlo creates a separate connection profile for each project. The
   registered copies must remain in their recorded locations. Arduino Agent
   must be running before its authenticated local connection can succeed.
   If a bridge is missing or an API is disabled, follow the connection message;
   manual selection remains available. Orca Remote API must be enabled by you
   with LAN access off. CLI fallback is a file workflow, not live editing.
5. Review the proposed operation and save destination, then approve it. Hazlo
   supplies the selected project folder to the AI, and shows it during native
   operation review. Asking to save "in the Hazlo project" refers to that folder,
   which can differ from the original document's folder. The AI should ask when
   the destination is missing or ambiguous. Check the proposed filename and path.
   Changes appear in the
   connected editor when its tools support live editing. Hazlo reads the native
   result and explains what changed, what was saved and what remains to do.

Some native servers discover/load tool groups on demand. Their first use can
require additional reviewed queries. **Actualizar herramientas y versiones**
refreshes discovery and release information; it does not install software.
Connection-triggered release checks are cached for 24 hours. Installed schemas,
not repository release announcements, determine the available operations.

**Runtime differences:** FreeCAD/CADPilot, selected patched CADCraft, Arduino
Agent and KiCad PCB have live-editing evidence. Arduino CLI compilation is a
file workflow. KiCad 10 live schematic editing remains deferred. Orca live
tools require the explicitly selected MCP runtime; stock Orca CLI slicing is
a file workflow. That MCP lacks project saving, so save the project in Orca.
Orca transforms also lack native Undo. Arduino's native MCP sketch writer saves
immediately; its separate open-buffer companion has different save semantics.

Attach one PNG/JPEG reference in chat when identifying or recreating a physical
part. Use a vision-capable connection and provide a known size when scale is
unknown. Hazlo does not store image bytes in its chat history; the selected AI
service can retain transmitted context. The current Claude connection is text
only. Rea supplies native software/firmware analysis, not photo-to-CAD conversion.

Verified client versions are Codex 0.159.2 and Claude Code 2.1.285/2.1.302.
Both have real signed-in chat evidence; Codex's [native edit/Save As test](research/codex-real-chat-2026-10-17.md)
uses its separate Hazlo account connection.
The existing Ollama choices remain available; local model quality is experimental.
Firmware upload, printer control and physical manufacturing readiness are not
established by an editing or slicing test. See [current acceptance](native-flow-status.md)
for the exact tested scope and outstanding account/runtime checks.
