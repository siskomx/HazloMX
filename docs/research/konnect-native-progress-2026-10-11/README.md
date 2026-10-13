# Konnect native acceptance in progress

October 11, 2026. This is partial native Windows evidence, not full workflow acceptance.

The current desktop/custom-protocol debug build starts in the real Windows app. Using the Computer Use plugin's `@oai/sky` APIs, the native project folder picker creates an owned fresh project, the workflow selector exposes all eight integrations, local chat `cambia R1 a 12.34k` fills the reference/value fields, and the real executable picker accepts the identified Konnect 0.13.0 runtime.

[Status, source and executable hashes](status.json), [native app accessibility state](app-accessibility.txt) and [created project manifest](hazlomx.json) retain the observed state. No source editing, model dispatch or hardware action occurred. The app remains open on the owned project for continuation; the recorded PID must be revalidated before relying on it.

The helper needed screenshot-backed reobservation after a folder-dialog geometry error. Clicking a WebView dropdown option by its accessibility index reported a point over the popup's Chrome window; keyboard selection completed the workflow choice. These were automation observation/input issues, not passed application behavior by assumption. Later capture showed the test app occluded by the active user window, so inputs stopped there; that foreign screenshot was not retained.

KiCad runtime/source pickers, approval/native SDK execution/publication, report/netlist actions and restart/history acceptance remain unfinished. Backend and browser-fixture evidence is retained separately in the [independent readback gate](../konnect-kicad-readback-2026-10-11/README.md). The final installer and full application goal remain active.
