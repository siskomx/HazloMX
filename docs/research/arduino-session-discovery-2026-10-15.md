# Arduino companion session discovery

The internal Arduino editor connection can now discover authenticated companion sessions without manually copying a pipe address or token. This is connection groundwork for the existing [native job path](arduino-native-jobs-2026-10-15.md); it is not yet connected to the chat's connection controls or enabled as a live AI writer.

When installed in Arduino Agent, the companion advertises under the standard profile's `.arduinoIDE/hazlomexa-editor` directory. An explicit private launch-directory override remains available for owned fixtures and custom setups. Activation checks the native application's name, stays dormant in another IDE and does not inspect source before authentication. The standard profile must already exist and must not redirect through a junction. No program is installed, restarted or reconfigured by discovery.

Rust reads only bounded regular connection records using the existing oplock-aware reader and pinned directory checks. Each canonical record name must match its native instance and pipe namespace. Discovery authenticates and reads the actual native editor state, ignoring stale, malformed and mismatched records. A saved sketch in the same directory never substitutes for an editor response. At most 64 directory entries are scanned, probes share a fifteen-second deadline, and cancellation returns no partial selection. Records are not deleted or repaired automatically.

The result exposes a selection identity and the validated native observation. Its connection credential remains an opaque Rust handle excluded from serialization. The transport future now supports concurrent read probes while retaining its original pre-delivery refusal and uncertain-write classification.

## Verification

- All 171 regular desktop-feature Rust library tests pass, with 55 integration tests ignored. Three new cases cover live versus stale/forged records, cancellation before and after dispatch, missing registries, entry limits and preservation of records and saved source.
- All sixteen Node tests pass. Two isolated child-process tests exercise default-profile activation, refusal of a redirected profile, dormant activation in another IDE and no source access before authentication. They use private fake profiles under ignored `target/`; the person's actual profile is untouched.
- The compiled owned Rust host now discovers the actual companion pipe before running its two approved backend jobs, native readback/follow-up, stale refusal, verified-result reads, saved-byte preservation and recovery against the controlled IDE API. Connection data is absent from serialized selection metadata. Desktop library/example Clippy passes.

These checks establish the connection and protocol behavior with a controlled editor API. They do not establish actual Arduino Agent installation, visible editing/Undo or a real-provider conversation. The desktop readiness check still reports Windows locked; no native UI action or real IDE test was started in this checkpoint. The existing unlock request remains pending, and the full application goal stays active. Native writer registration, chat connection integration, real IDE acceptance and current installer acceptance remain unfinished.
