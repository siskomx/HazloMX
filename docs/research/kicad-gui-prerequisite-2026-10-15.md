# KiCad native GUI prerequisite — October 15, 2026

The existing KiCad 10.0.7 test runtime could run CLI workflows but could not start the actual PCB editor: the owned native window reported that Python could not load its filesystem encoding. The private extraction included Python DLLs and executables but omitted the standard library.

The new tests/kicad/provision_gui_runtime.py verifies the original official installer's pinned SHA-256 and restores its Python library and extension directories without overwriting existing files. The restored runtime passes isolated imports of encodings and pcbnew, and the native binding reports the expected 10.0.7 version. No global Python installation or user KiCad profile is changed.

A fresh owned board copy and isolated profile now reach KiCad's actual first-run setup. Setup is currently waiting at Updates & Privacy for the person to choose preferences. No positive IPC observation or native mutation is claimed. Raw startup observations, paths, profiles and logs remain private in ignored build output.

The new ignored Rust gate actual_live_pcb_observes_owned_editor_without_saved_fallback is compiled and ready for that explicitly launched fixture. It accepts only an owned target-directory fixture, verifies the original board bytes, runs the production host's native selection and refresh path, checks complete stable observations and path privacy, and requires unchanged board bytes with no native job/history. It has not yet run against a ready editor.

Two implementation constraints were rechecked against actual software and primary sources:

- The installed CLI does not expose api-server, even though newer online API documentation describes it. Its help command exits successfully for the unsupported subcommand, so exit status alone is not evidence of capability.
- The pinned Konnect client still sends headerless BeginCommit/EndCommit and its ordinary component move can fall back to disk. The [KiCad 10.0.7 contract](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/api/proto/common/commands/editor_commands.proto) provides explicit document headers. A live write must use compatible native transactions with exact current-state checks and no file fallback. The [native server](https://github.com/KiCad/kicad-source-mirror/blob/10.0.7/common/api/api_server.cpp) permits empty-token bootstrap, which does not establish instance/restart binding. The current read connection remains explicitly unverified for restart binding and grants no writes.

Regular library tests pass: 152 passed, 53 ignored. The newly added positive native test remains ignored pending setup. The full application goal, native PCB editing, other programs and the current installer remain unfinished.
