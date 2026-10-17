# Installed AI client discovery

Hazlo checks installed Codex and Claude Code clients automatically when its chat
starts. It searches absolute PATH entries and the usual user npm/native client
locations, skips shell shims and relative/current-directory entries, deduplicates
paths, and uses the existing executable fingerprint/version checks. The probes
run off the UI thread and only pass `--version`. Manual selection remains
available; discovery does not replace an already selected client.

Discovery does not read authentication files, copy credentials, initiate a login,
register tools globally or send a model request. An installed client and a
connected account are separate states. Codex still uses Hazlo's own scoped
client home and account connection. Claude uses its client's existing sign-in.

PowerShell's command inventory on the development host finds both clients. The
verified Codex 0.159.2 is installed inside the npm package, alongside a newer
desktop-bundled client. Native Claude Code 2.1.302 is installed and reports signed
in; its restricted/MCP CLI flags are present. The bridge accepts 2.1.285 and
2.1.302; a real 2.1.302 conversation is a separate remaining acceptance check.
Other client versions remain explicitly unsupported pending protocol validation.

Checks: 206 regular Rust tests pass (62 external/native checks ignored), desktop
and examples Clippy passes, frontend production build passes, image and history
fixtures pass. The startup fixture checks automatic discovery without login or
an engineering operation. Real native-window and installer acceptance follow
this source change; this record does not claim them in advance.

The first real release-window request exposed a new rate_limit_event notification
from Claude 2.1.302. It ran no native edit. The decoder now accepts that
informational event after initialization, while still requiring a terminal
result and rejecting unknown event types. The four regular Claude decoder
tests pass; real conversation acceptance continues separately.
