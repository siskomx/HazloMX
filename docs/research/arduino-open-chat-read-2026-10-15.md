# Open Arduino buffers in chat

The shared agent catalog now includes `inspect_open_agent_documents` for the Arduino Agent program choice. It uses the existing native editor companion and authenticated local named pipe to inspect the active buffer, including unsaved code. It does not use the stock Agent disk-reading sketch tool.

With one responding editor, the tool returns that editor's fresh observation. With several, it returns only sketch names, dirty flags and opaque instance identities; the agent asks which sketch the person means and uses the identity internally for the next read. The list contains no sketch source. A requested editor that no longer responds is refused. A disconnected companion returns an empty list and setup guidance, without reading a saved sketch instead. Names and source remain explicitly untrusted data.

This is a read-only tool. Connection credentials, pipe addresses and source paths are not returned. The shared program filter restricts it to Arduino Agent; host execution checks the selected project and cancellation before returning native content. Existing saved-copy tools remain separate and require an explicit choice of that workflow. Neither inspection nor the capability description grants native editing, saving, compilation or hardware access.

## Evidence and remaining acceptance

All 172 regular desktop Rust tests pass, with 55 integration tests ignored, and desktop library Clippy passes. The MCP SDK catalog round trip advertises 29 tools; focused checks cover program scope, invalid arguments and cancellation before discovery. The 16 Node companion tests pass with the freshly compiled Rust host fixture enabled.

The transport fixture starts two authenticated companions over a controlled IDE API, uses an isolated child profile and invokes the actual chat tool through `HostTools`. It verifies list-only ambiguity, exact selection, automatic single-editor selection, unknown-instance/path refusal, private-field exclusion, fresh source/hash after two approved production backend jobs, stale-write refusal, saved-file preservation, disconnection and schema-15 recovery. It never reads the user's Arduino profile. This establishes real pipe and production host behavior, not the real Arduino IDE, a real model conversation or installed-app acceptance.

The actual Arduino window test remains pending while Windows reports a locked desktop. The next native gate is the companion's visible unsaved-buffer edit, independent readback, undo and follow-up in the real IDE. The native writer remains absent from the agent catalog until that gate passes. The full application goal remains active.
