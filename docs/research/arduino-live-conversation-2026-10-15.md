# Arduino native conversation gate — October 15, 2026

An opt-in acceptance example now exercises the existing configured cloud route against the actual open Arduino Agent buffer. It reuses the existing conversation recorder, scoped production tool catalog, native companion and approval/job pipeline. This gate is prepared but has **not passed**; no real chat request was sent in the attempts below.

## Required scenario

The owned supervisor first installs and loads the companion, completes its native editor probe and passes direct default-profile host discovery. A separate five-turn conversation then asks the model to inspect the open sketch and clarify its current pause. The person requests a quarter second, reviews an actual native proposal, and approves through the existing backend fixture. The model must read the verified receipt and current native source before a second request for a half second, a separate proposal/approval and final explanation.

Assertions bind each proposal to the observed instance, document token, version and source digest. Only the pause literal may change; no source mutation may occur before approval. Final native source, dirty state, earlier unsaved note, saved bytes, first receipt and two completed schema-15 jobs after recovery must match. A saved-copy proposal cannot satisfy this test. The shared recorder also checks program scope, exclusion of approval nonces and builder paths, and successful provider responses.

The existing cloud identity is available locally; no model, provider or dependency is installed by this change. Raw conversations, protocol records, source buffers, profiles and failures stay under ignored `target/`. Approvals remain fixtures; passing this gate would not establish Hazlo's native approval UI or installed acceptance.

## Attempts and validation

The first owned editor reached its native/default-profile prerequisites. The new conversation example then stopped before any provider request because it called the synchronous native host reader inside an async runtime. The fixture now uses `tokio::task::block_in_place` for that read, matching its multithreaded test runtime. Production provider dispatch already executes host tools on a blocking worker; no production dispatch behavior changed.

The next owned run stopped at the unchanged 1 GB physical/commit memory floor before the native probe completed. No cloud conversation was started for that stopped editor. At the later memory check Windows had about 1.9 GB free, with Java/Gradle processes using about 2.8 GB; the person has been asked to close unused work. The owned processes are terminal. These outcomes remain failed/pending evidence, not a passing provider gate.

The new example and modified supervisor compile and pass Clippy with warnings denied. Both Python launchers parse, and their help paths pass. Production source/frontend behavior is unchanged, so their prior suites were not rerun for this acceptance-only addition.

## Reproduction

Build `ollama_agent_live_chat`, `owned_arduino_editor_probe` and `owned_agent_editor_host`. Launch `tests/arduino/prepare_native_editor.py --launch --hold-for-conversation`, wait for its default-profile host check to pass, and keep that supervisor live. Set `HAZLOMEXA_REAL_AGENT_ROOT` to this owned fixture's root, then run `apps/desktop/tests/ollama-native-chat.py --run-real-cloud --program agent-live`. The recorder supplies the isolated user profile to its child. The Rust example independently validates the owned root, prerequisite and live native state before sending source to the existing provider.

Conversation mode extends only the owned fixture's maximum lifetime from 150 to 600 seconds. Memory, desktop-lock and explicit-stop monitors remain active. Stop the owned supervisor after the conversation; never retry an uncertain native edit. Live save/compile, hardware, other providers, native Hazlo UI and the current installer remain unfinished. The full application goal stays active.
