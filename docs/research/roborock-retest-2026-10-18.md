# Roborock investigation: installed chat retest

## Scope and method

The installed Windows desktop built from 697b162 was exercised through its real
chat window in a fresh local project. The previous conversation was preserved.
The scenario was a working Roborock Q7 M5+, retaining its parts and scheduled
cleaning, with local Wi-Fi allowed but no internet/cloud dependency.

The operator supplied the scenario, accepted the agent's proposed direction,
selected the program it requested, approved its inspected read-only native call,
and asked how to continue with the information available when no file had been
attached. No tool names, search strategy, pagination instructions, compatibility
answer, device observation or proposed solution were supplied to the agent.
There were four user messages and five assistant responses, including the
automatic native-result review. Raw transcripts and runtime receipts remain in
ignored local test output.

## Observed behavior

- The existing Codex account connected. The program initially remained at
  "Help me choose". The agent proposed Rea; selecting it automatically connected
  the bundled runtime and exposed 207 native tools without locating files or
  editing configuration.
- Session evidence contains the shared investigation skill, public web research,
  capability discovery, native catalog inspection, a proposed native call and
  verified-result reads. The agent used the native tool catalog rather than
  inventing a device-specific operation.
- It distinguished the requested model from Q7 Max, consulted the Valetudo
  supported-model documentation and Home Assistant's Roborock documentation,
  and treated compatibility and cloud independence as unconfirmed. These are
  observations of its research, not an independent hardware compatibility test.
- Its approved native request contained zero change calls and a `binary_session`
  readback. The retained report was 127,185 bytes. The agent retrieved verified
  result pages, including an offset of 65,536, without rerunning the native query.
  The former small-report failure did not recur.
- The automatic review said Rea had no open file or active analysis and reported
  missing engines, including Ghidra and Android analysis support. It did not
  claim to have analyzed the vacuum, saved anything or established that the
  user's goal was impossible.
- When asked how to continue with no attached file, it paused Rea and proposed a
  controlled schedule experiment: establish a working baseline, block only the
  vacuum's internet access while retaining local Wi-Fi and settings, observe
  cleaning/return/auto-empty, and repeat after a longer offline interval. It did
  not represent this experiment as performed.
- It proposed investigating a version-specific local interface if that experiment
  fails. It also suggested a scheduled external button actuator as an explicitly
  unverified, reversible fallback, noting limits such as room control and the
  need to check the complete cycle and auto-empty behavior.
- The final question asks whether a schedule has already been tested with
  internet blocked and Wi-Fi retained. The operator supplied no invented answer.
  The installed chat is left at that question for the user.

## Remaining friction and limits

- Selecting the suggested program still requires a user action and another chat
  message before the agent continues. Connection itself was automatic.
- The agent selected Rea before obtaining an analysis artifact. It recognized
  that limitation after the status readback and continued without treating it as
  a failed device investigation.
- Emphasis and Markdown links appear as literal Markdown in the chat. The
  approval card calls a read-only query a "change" and includes a geometry
  disclaimer that is irrelevant to software investigation.
- Native automation initially failed to capture the foreground process. After
  resetting its JavaScript session, the helper's app launch also failed. A direct
  launch of the installed app restored capture and coordinate input. Accessibility
  text remained unavailable during the successful visual test.
- No robot, router, firmware, phone app or hardware was changed. No software
  package was analyzed. Cloud-free schedules, long-term offline operation and
  model-specific local control remain unverified.

No application code, skill instructions or installer were changed for this
observation. Hazlo remained a simple chat using native tools and result readback.
