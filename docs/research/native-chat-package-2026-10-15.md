# Current native-chat Windows package checkpoint

The current Windows NSIS package builds successfully from commit `014aa06`.
It includes the conversational software-choice entry, the Arduino native editor
companion and approved save route, and the KiCad live PCB move/export routes.
The [package identity](native-chat-package-2026-10-15/package-identity.json)
records binary hashes without local paths or captures.

## Verified

- The normal privacy-remapped `npm run package` command completed successfully,
  including the frozen KiCad worker prerequisite, frontend, release helper,
  release desktop application and NSIS bundle.
- All eight existing installed-connector harnesses were rebuilt against the
  current source. The protocol harness advertises all 33 current shared tools;
  the earlier binary advertised only 26.
- The current release helper passed the rebuilt protocol harness: catalog and
  project binding, unknown-selection refusal, no agent-accessible approval tool,
  and no engineering jobs created by the check. This checks the release helper
  transport with a current harness; it does not prove installed native editing.
- The disposable guest fixture was prepared with the current package and all
  eight native backend harnesses. Its previous-package upgrade input was
  verified by hash. The fixture omits all UI drivers, model inference and
  hardware control. Private inputs and output remain ignored.

## Acceptance still pending

The guest did not launch. Available RAM fell below the unchanged 3072 MiB
launch requirement after preparation, reaching about 1202 MiB during the bounded
launch check. No fresh guest installation, upgrade, native backend workflow,
startup or uninstall result is claimed for this package.

The separate host lifecycle attempt refused an existing user installation
before installation or project modification. It did not replace that install
and is not a lifecycle pass.

Native Hazlo/Arduino and Hazlo/KiCad UI acceptance remains pending while a
Windows Security prompt requires human handling. No security permission prompt
was acted on. Existing backend native-conversation evidence remains separate
from approval/picker/native UI acceptance. Other program/provider gaps in the
application goal remain open.
