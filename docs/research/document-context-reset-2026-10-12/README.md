# Document context resets preserve measurement provenance

CADPilot now shares a reset for explicitly choosing another document, changing project identity/folder, and selecting a verified result for follow-up. It clears the source binding, object selection and draft dimensions/diameter/position. A new source starts with **Medir e inspeccionar primero**; a verified result then establishes its own source binding and measured box choices. Existing units remain a user preference rather than a guessed measurement.

The desktop project-change reset now compares the complete project ID/folder tuple, including copied folders that retain an ID. This also invalidates cached PCB/schematic discovery and temporary agent-connection display using the existing reset path. Backend source binding and publication checks remain unchanged.

The nine-workflow browser fixture passes these cases: unrelated entered cylinder dimensions/positions clear before following a saved box result; choosing another CADPilot document clears all fields and starts inspection; populated CADPilot fields clear on a same-ID folder change; existing unit conversions, unknown-position refusal, proposals/approval, source-bound follow-ups and project/provider conversation recovery still pass. TypeScript and the production build pass. These are browser IPC fixtures, not native picker or installed-release evidence.

The current `0cdc30ad…` installer predates this change. Native/installed context-switch acceptance, signed-in conversation and live unsaved-document editing remain unfinished. The full goal stays active.
