# Uncoached Roborock chat observation and Windows build

Application source: a843549. The user requested a real test of the new skill with
the Roborock Q7 M5+ scenario, local Wi-Fi allowed and no internet/cloud, followed
by a rebuild and local installation. They explicitly asked the operator to let
Hazlo encounter its own roadblocks and only take notes.

## Test method

An owned debug desktop was built from the committed source. The exact final skill
was present in the executable and actual Codex session instructions. A new empty
project used the installed signed-in Codex client. It began in the normal
software-selection chat; the operator selected Rea only after Hazlo proposed it.
Rea then connected automatically and exposed its observed 207-tool catalog without
a configuration-file picker. This count describes this runtime, not the skill.

The operator supplied only these natural-language messages:

1. A working Q7 M5+, retain its components, scheduled cleaning without internet
   or cloud, local Wi-Fi allowed: how would we do it?
2. Yes, start with Rea.
3. What happened? How do we continue?
4. Go ahead.
5. And now?

No source links, tool names, suggested workaround, model compatibility answer,
firmware, invented device observation or phone platform was supplied. The two
native status queries were inspected and approved in Hazlo's UI. No behavior,
prompt or report-size limit was changed during the observation.

## Observed behavior

Hazlo researched publicly available compatibility information and inspected its
available program capabilities. It distinguished the Q7 M5+ from Q7 Max and treated
an unsupported integration as one closed route rather than an impossible goal.
It suggested observing whether existing schedules survive an internet block and
restart before changing firmware. That was a proposed experiment; no router or
robot test was performed. It then chose Rea as a software-analysis route.

The first UI-approved proposal had no change calls and used binary_session with
empty arguments as its readback. Hazlo rejected the native report because it
exceeded the report-size limit. On the neutral follow-up, the agent acknowledged
the size failure, found no recoverable retained result, and did not call it a
vacuum failure or claim successful analysis.

After a neutral go-ahead, Hazlo prepared a narrower, separately reviewed query:
current_document with empty arguments, again with no change calls. The native
result was target_unavailable: no application was open for analysis. The agent
then stopped repeating the query and asked whether the user uses Roborock or
Xiaomi Home, on Android or iPhone. The operator did not invent that answer, and
the observation ended at that missing input.

## Roadblocks and follow-up notes, left unchanged

- The broad Rea session report exceeds Hazlo's native report limit.
- A connected tool catalog does not mean an analysis target is loaded. No robot,
  app package or firmware artifact was available in this test.
- The native error JSON was shown directly in the main chat status. Its technical
  content disrupts the otherwise plain-language interface.
- Failed queries needed a neutral user follow-up before the agent explained them.
- The initial proposal described identifying a selected file before one had been
  confirmed. The missing target became explicit only after the narrower query.

The skill produced useful research, alternatives, native discovery and an honest
next question. It did not produce a working controller or establish cloud-free
scheduling, firmware compatibility, physical safety or a repaired vacuum. No
robot, firmware or network changes were executed. Normal local project, connection
and job-journal files were created by Hazlo. KiCad 10 live schematics remain deferred.

## Installer acceptance

The local Windows 0.1.0 installer was rebuilt from a843549. Installer SHA-256:

`d4efc75ddf389dc29aa8750c4935c94a71c1cb949eb39554a0de716ac9d6b7ae`.

Package identity and the release helper's 38-tool protocol checks pass. A fresh
Windows guest passes current-user installation, same-version update, update from
the previous alpha.5 build, process startup, uninstall and user-file preservation.
Three independently exported project files match their expected hashes. The guest
was stopped. These are lifecycle checks, not guest visual or engineering tests.

The initial offline guest lacked WebView2 and the prerequisite installer exited
with code 2. A fresh guest with networking for prerequisites passed the lifecycle
checks. The package still requires an existing WebView2 runtime or network access
to obtain it; this observation did not change the installer prerequisite policy.

Local installation verifies the installed app/helper against package identity,
bundled Rea files and embedded skill. Four existing project/catalog files survived
unchanged. This is a local candidate installation, not a new public release.

## Installed-app handoff

The production window opens with the simple chat. Its existing Codex account
connects, and bundled Rea automatically discovers 207 native tools without a
configuration or executable picker.

The debug observation's history did not appear automatically in the installed
app: the two WebView origins store separate browser databases. For this handoff,
the saved conversation database was copied into a separate local profile, with
only its origin metadata adapted. Conversation keys and values were verified
byte-identical. Both original test data and the normal installed profile remain
intact. This was a private data-restoration step, not a new import feature or a
fix to application code. Ordinary project selection alone did not transfer that
debug history; retain this as a follow-up note.

The restored installed UI displays all five user prompts and their original
answers, ending at the unanswered control-app/phone-platform question. No new
prompt was submitted and no failed proposal was rerun during the handoff.
