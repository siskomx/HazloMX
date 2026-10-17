# Isolated CADCraft native binding patch

The user authorized testing an isolated source patch. The installed CADCraft, Hazlo runtime selection and production live writer remain unchanged. This checkpoint concerns native API behavior; it does not establish an integrated Hazlo conversation.

## Scope and first result

The source is pinned to official CADCraft 0.7.0, commit `fc4e98315a8d771b5ab832f0ade0b5e7b106df7f`. A separate Rust 1.95.0 toolchain, Cargo home, build directory and application profile stay under ignored `target/`; the host default toolchain is unchanged. Optional external fonts are not provisioned.

The [previous native experiment](cadcraft-lisp-binding-2026-10-16.md) demonstrated that entity and layer projections miss layer lineweight. The patch retains CADCraft's immutable native drawing through an `Arc`, rather than reconstructing a complete drawing from those projections. Copy-on-write changes therefore invalidate its pointer identity. A document-lifetime UID, revision, path, dirty status and editing context accompany that drawing reference.

Two prototype methods share the existing native control channel:

- `document.binding` captures an opaque token and one native drawing snapshot. A newer capture replaces the old one.
- `engine.execute_checked` compares the token and native state, consumes the token, and calls the existing engine synchronously on the GUI thread. There is no intervening host read/write round trip. The token is bookkeeping, not authentication.

The first actual source-build test run passed unchanged-drawing execution, token replacement/reuse refusal, native Undo recording and refusal after the previously unobserved lineweight change. It failed two regression tests: changed selection and an active interactive command still allowed dispatch. That failed run is retained privately.

The corrected patch includes current/previous selection and Undo context in the binding, refuses an active/suspended command or running LISP routine, and refuses checked dispatch when Undo is disabled or a group is open. All nine focused binding tests and all four existing method-catalog/protocol tests pass. The additional cases cover identical geometry in another document, previous selection and changed Undo settings. The changed Rust files follow upstream formatting.

The [retained source patch and tests](cadcraft-native-binding-2026-10-16/native-binding.patch) apply to the pinned source. Reproduction uses Rust 1.95.0 and the locked dependencies: run `cargo test --locked -p cadcraft-ui-egui --test control_binding --test control_methods -j 1`, followed by `cargo build --locked -p cadcraft -j 1` in the patched checkout. Keep its toolchain, Cargo home, target directory and GUI configuration separate from installed software. Raw local logs and profiles are deliberately excluded from this record.

Before the GUI launch, inspection of the framework showed that its Windows persistence uses the OS known folder and ignores redirected `APPDATA`. The separate [GUI test-profile override](cadcraft-native-binding-2026-10-16/gui-profile-isolation.patch) makes the existing nonempty `CADCRAFT_CONFIG_DIR` also select the framework settings file. With that variable absent, the framework's default behavior is preserved. The GUI test requires this override and an absolute owned configuration directory; it is not part of the native binding contract. Both retained patches independently apply to fresh pinned files and together reproduce all four changed files.

## Native application acceptance

The actual desktop application builds successfully, first with the native API patch and again with the separate settings-isolation override. The final binary reports 0.7.0 through its CLI, and the final formatting and whitespace checks pass. The [sanitized checkpoint](cadcraft-native-binding-2026-10-16/checkpoint.json) records its binary and patch hashes.

The owned GUI probe stops at its desktop prerequisite before creating a test directory or launching CADCraft: Windows reports an active but locked session, with approximately 6.1 GiB available RAM. The user is asked to unlock Windows. Actual visible edit/readback/follow-up, two Undo steps, reopen refusal, settings-file isolation and graceful native exit therefore remain pending; the source-build/unit results do not replace those checks. No GUI test result is claimed.

## Adoption boundary

This test does not enable a model-facing generic command executor or arbitrary LISP. Production adoption still needs an explicit supported runtime choice, owned endpoint/process checks, typed and bounded operations, approval binding, independent result checks, uncertain-outcome handling and actual chat acceptance. The retained drawing protects drawing data; this narrow experiment is not a guarantee covering every mutable session setting or every native command. The ordinary native command API remains available to other actors.

No upstream changes are published. Full upstream CI, WASM and cross-platform builds are outside this Windows patch trial. A production patched runtime would need maintenance until upstream provides the contract.
