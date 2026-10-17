# Automatic native catalog and repository checks

Hazlo remains a simple chat. Conexiones now contains an optional expandable list
of the connected program's actual native tool names. A CLI connection also shows
its captured native command help. These details stay out of the conversation
context; the agent fetches the actual schema it needs through native discovery.

Connecting an installed MCP or CLI starts a background release check against the
selected program's known official repositories. It does not delay the native
connection, send project data or credentials, or install software. Results are
cached in memory for 24 hours; subsequent connection/capability requests can
refresh an expired entry. Offline and failed checks remain explicitly unavailable.
Simultaneous requests do not duplicate the same pending check.

The connection's **Actualizar herramientas y versiones** button refreshes native
discovery and requests a fresh repository check. The agent's existing capability
tool exposes the checked release tag, cache age and compatibility limits. A
manual per-repository lookup takes precedence over older cached metadata. The
eight repository sources and program scopes reuse the existing provenance catalog.

MCP identities and schemas continue to refresh before proposals and dispatch.
CLI help belongs to the selected executable's checked binary hash. Replacing
that binary requires reconnecting, so an approval cannot silently apply to a
different installed program. New tools loaded by a native server are discovered
through that server; a repository release alone never grants execution capability.

Verification includes a real connection-triggered HTTPS check of Arduino CLI's
official release endpoint: it reports `v1.5.1` without a manual release-tool call.
The connected native server in that check is explicitly a protocol fixture, not
an Arduino engineering acceptance substitute. Offline tests cover program scope,
pending-request deduplication, cache reuse, explicit refresh and manual-result
priority. Browser fixtures exercise the expandable catalog and refresh action
across four AI choices and six program groups. No program, provider or model is
installed by these checks.

The earlier `5a9a30c` installer build completed with unchanged tracked inputs and
zero builder-profile matches in its shipped binaries. It is retained privately as
the update-test baseline; it predates these automatic checks. The final installer
is rebuilt from this updated source before Windows guest acceptance.
