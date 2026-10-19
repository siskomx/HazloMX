# Optional diagnostics installer check

Source: `f62e65f`, Windows x64, application version 0.1.0.

Installer SHA-256: `6305a873f76ed5c5b1574ff183c5af0f2b57dfb1c2e2716226300ec7dd501436`.
Installer size: 53,029,563 bytes. Retained locally; no new public release published.

The production build passed with all 6,766 tracked inputs unchanged. The app and helper contained no builder-profile path matches. The packaged helper passed its 38-tool protocol check.

The current-user upgrade from source `697b162` succeeded. Installed app and helper hashes matched the package identities, including the NSIS bundle marker. Checked bundled Rea files matched the build runtime. Nine saved project and connection files retained their hashes across the upgrade.

The installed app opened successfully. Native-window checks confirmed the simple chat, the existing project picker, and the TShark, USBPcap and pySerial choices with separate detection and optional setup controls. The saved local project reopened. Selecting all three diagnostics and requesting TShark detection created no optional diagnostic download files. The app was left open for manual use.

The clean Windows guest lifecycle test passed after closing the Java processes freed enough RAM. A fresh guest installed source `697b162`, verified its binaries and helper protocol, upgraded to this installer, verified the updated binaries and 38-tool helper protocol, started the app, and uninstalled it. The saved chat, project and user-data markers survived. Four exported project files independently matched their recorded hashes. The disposable guest was stopped after the test. This was a package lifecycle check; engineering workflows and physical hardware were not rerun inside the guest.

No capture drivers were installed, no physical ports were opened, and no live traffic was captured during these installer checks. Native diagnostic routing and bounded fixture checks are documented separately in [the implementation notes](native-diagnostics-implementation-2026-10-18.md). Driver installation, elevation and real hardware behavior remain untested. KiCad 10 live schematic editing remains pending.

Raw build logs, local paths, preservation manifests and receipts remain in ignored local evidence folders.
