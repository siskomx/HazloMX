# Isolated CADCraft native binding patch

The user authorized testing an isolated source patch. The installed CADCraft, Hazlo runtime selection and production live writer remain unchanged. This checkpoint concerns native API behavior; it does not establish an integrated Hazlo conversation.

## Scope and first result

The source is pinned to official CADCraft 0.7.0, commit `fc4e98315a8d771b5ab832f0ade0b5e7b106df7f`. A separate Rust 1.95.0 toolchain, Cargo home, build directory and application profile stay under ignored `target/`; the host default toolchain is unchanged. Optional external fonts are not provisioned.

The [previous native experiment](cadcraft-lisp-binding-2026-10-16.md) demonstrated that entity and layer projections miss layer lineweight. The patch retains CADCraft's immutable native drawing through an `Arc`, rather than reconstructing a complete drawing from those projections. Copy-on-write changes therefore invalidate its pointer identity. A document-lifetime UID, revision, path, dirty status and editing context accompany that drawing reference.

Two prototype methods share the existing native control channel:

- `document.binding` captures an opaque token and one native drawing snapshot. A newer capture replaces the old one.
- `engine.execute_checked` compares the token and native state, consumes the token, and calls the existing engine synchronously on the GUI thread. There is no intervening host read/write round trip. The token is bookkeeping, not authentication.

The first actual source-build test run passed unchanged-drawing execution, token replacement/reuse refusal, native Undo recording and refusal after the previously unobserved lineweight change. It failed two regression tests: changed selection and an active interactive command still allowed dispatch. That failed run is retained privately.

The corrected patch includes current/previous selection and Undo context in the binding, refuses an active/suspended command or running LISP routine, and refuses checked dispatch when Undo is disabled or a group is open. All ten focused binding tests and all four existing method-catalog/protocol tests pass. The additional cases cover identical geometry in another document, previous selection, changed Undo settings and a waiting native LISP routine. The changed Rust files follow upstream formatting.

The waiting-routine regression uses the actual command-line handler and native engine in process, without Windows input automation. A person's LISP routine waits at `getpoint` with no ordinary command running. Both binding capture and the attempted checked line edit refuse without adding geometry or cancelling that routine. Supplying the person's point then completes exactly one circle at (4, 5) with radius 1. This checks the distinct LISP waiting state; it is not a live-window or model-conversation result.

The [retained source patch and tests](cadcraft-native-binding-2026-10-16/native-binding.patch) apply to the pinned source. Reproduction uses Rust 1.95.0 and the locked dependencies: run `cargo test --locked -p cadcraft-ui-egui --test control_binding --test control_methods -j 1`, followed by `cargo build --locked -p cadcraft -j 1` in the patched checkout. Keep its toolchain, Cargo home, target directory and GUI configuration separate from installed software. Raw local logs and profiles are deliberately excluded from this record.

Before the GUI launch, inspection of the framework showed that its Windows persistence uses the OS known folder and ignores redirected `APPDATA`. The separate [GUI test-profile override](cadcraft-native-binding-2026-10-16/gui-profile-isolation.patch) makes the existing nonempty `CADCRAFT_CONFIG_DIR` also select the framework settings file. With that variable absent, the framework's default behavior is preserved. The GUI test requires this override and an absolute owned configuration directory; it is not part of the native binding contract. Both retained patches independently apply to fresh pinned files and together reproduce all four changed files.

## Native application acceptance

The actual desktop application builds successfully, first with the native API patch and again with the separate settings-isolation override. The final binary reports 0.7.0 through its CLI, and the final formatting and whitespace checks pass. The [sanitized checkpoint](cadcraft-native-binding-2026-10-16/checkpoint.json) records its binary and patch hashes.

The first owned GUI attempt stops before creating a test directory or launching CADCraft because Windows is locked, despite sufficient RAM. After the user unlocks Windows, the [actual native-window trial](cadcraft-native-binding-2026-10-16/native-window.json) passes:

- Changing layer lineweight leaves the old reduced projection identical, but the native binding refuses dispatch and preserves every entity.
- A wrong token refuses. The matching token permits a 1 mm native move; replay refuses because that one-use binding was consumed.
- A fresh token permits a 0.1-inch follow-up, reaching X = 3.54 mm. The actual owned CADCraft window displays the edited circle.
- Two native Undo commands restore the intermediate and original complete entity states. Closing and reopening the identical saved source invalidates the old document binding.
- The saved source bytes match the original fixture. Framework settings are created under the owned configuration directory, while the host's framework settings file retains its prior absence/hash.
- Native quit exits successfully, the process is joined, and an independent check finds its PID absent.

An independent verifier checks all 30 saved protocol exchanges, ten complete entity readbacks and the exact four refusal messages, alongside the original source bytes and executable identity. Its first refusal-message assertion incorrectly expected a changed binding for replay; the intended consumed-token response is `no native binding`. The corrected exact sequence verifies the same retained traffic without rerunning or changing the engineering operation.

This is a fixed native TCP/control test plus observation of its actual GUI. It does not include Hazlo approvals, a model conversation, arbitrary command coverage or a production adapter. Neither installed CADCraft nor production runtime selection is replaced. The API trial closes the tested native precondition gap; adopting this patched runtime for Hazlo is a separate user choice now pending.

## Adoption boundary

This test does not enable a model-facing generic command executor or arbitrary LISP. Production adoption still needs an explicit supported runtime choice, owned endpoint/process checks, typed and bounded operations, approval binding, independent result checks, uncertain-outcome handling and actual chat acceptance. The retained drawing protects drawing data; this narrow experiment is not a guarantee covering every mutable session setting or every native command. The ordinary native command API remains available to other actors.

No upstream changes are published. Full upstream CI, WASM and cross-platform builds are outside this Windows patch trial. A production patched runtime would need maintenance until upstream provides the contract.
