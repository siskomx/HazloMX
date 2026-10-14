# Installed Arduino: guided intent, approval and recovery

Installer `07ac4126473b67e3d83fe9a1afc3ac08d306097b8a2ab6ff15582f7d987c30b5` passes an actual native Windows Arduino CLI workflow, using the owned installed Hazlo window and Windows pickers. Its separate [guest matrix and ordinary-user evidence](../native-cadpilot-preview-installed-2026-10-13/README.md) covers package lifecycle and fixture-driven integration jobs.

## Native interaction

1. Choose **Sketch y compilación · Arduino** in the workflow list. The initial sketch is empty; its displayed setup/loop example is placeholder text.
2. Select the owned Arduino CLI 1.5.1 executable and prepared private data folder through the two native pickers. The board choice explicitly shows Arduino Uno / AVR 1.8.8.
3. Send **Quiero que parpadee el LED integrado de un Arduino Uno**. Hazlo asks for the on/off interval and explains that the bounded example uses the same interval for both. It does not choose a duration or generate code yet.
4. Answer **250**. Hazlo asks for timing with units again; the sketch stays empty. Answer **250 ms**. It creates a visible draft with `LED_BUILTIN`, output mode and two `delay(250)` calls. No compilation starts from chat.
5. Prepare the compilation. Review the exact code, Uno identity, CLI/core versions and Sketch/HEX/ELF outputs in the approval panel. Cancel this first proposal. The native UI reports cancellation; its persisted run is cancelled and has no artifact directory.
6. Prepare a fresh proposal and click **Aprobar y compilar**. The native UI reports verified published files. Independent saved-file checks match the approved sketch, validate all Intel HEX record lengths/checksums and its terminal record, count 922 firmware data bytes, and identify the ELF32 little-endian executable's AVR machine type. All 116 earlier artifact files retain their original hashes.
7. Open **Ver sketch verificado**: the read-only modal displays the saved 250 ms code. **Mostrar HEX** opens the owned result folder in File Explorer with `firmware.hex` selected. Close that Explorer window without opening or running the firmware.
8. Close and relaunch the exact installed app, reopen the existing project through the native folder picker and choose Arduino again. Its conversation, cancelled proposal and completed result recover. Open the recovered saved sketch modal successfully. No approval resumes; the draft code starts empty. All 211 persistent project files, excluding the writer lock, have identical names and hashes across reopening and result viewing.

`checks.json` retains versions, validation outcomes, counts and artifact hashes. Engineering files, installation/profile paths, logs and captures stay private under ignored owned fixtures. An initial private verification-script error (an incorrectly doubled artifact-directory prefix) was corrected without changing application or engineering files.

## Scope and remaining gaps

This uses the installed release, ordinary native controls and real compilation. No WebView debugger, backend approval driver, AI model inference, firmware upload or physical device action is involved. The chat is the bounded built-in Uno LED guide; arbitrary sketch generation, asymmetric timing and other boards are not established by this test. Native in-progress Arduino cancellation, timeout behavior, the ELF reveal button and signed-in Claude/Codex chat remain unverified here.

Recovery exposed a usability issue: `Project::history()` returns filesystem enumeration order and the frontend renders it directly. The new result is present and opens correctly, but is buried among older entries. Chronological history ordering remains an application fix, to combine with findings from the four still-unverified workflow choices (Arduino Agent, Konnect schematic, Konnect PCB and OrcaSlicer). This evidence does not close broader CAD fidelity or live unsaved editing requirements.
