# Private locations in agent result reads

PCB verification reports include an internal source folder. `read_verified_result` replaces that field with a private-folder marker before returning report content to an agent. The native report on disk, its verification findings and artifact validation remain unchanged.

`sha256` identifies the actual verified saved report and remains the binding for follow-up proposals. `content_sha256` identifies the returned text. `private_paths_redacted` tells callers when the report is a projection. Other report formats and explicitly selected sketch code retain their existing content behavior. This change is specific to the internal PCB verification folder; it is not a general secret scrubber.

The published-fixture regression confirms the folder is hidden, findings stay identical, both hashes have their stated meanings, and saved bytes remain unchanged. All 117 regular backend tests pass serially, with 43 native cases explicitly ignored; desktop/custom-protocol Clippy passes. The actual connector harness now asserts redaction during PCB follow-ups. Its new native run and rebuilt-package acceptance remain pending.

The packaging launcher also adds process-local Rust path remapping so compiler source locations do not include the builder's home directory. It preserves encoded custom flags and refuses ambiguous plain RUSTFLAGS. The rebuild is in progress; binary path scanning and installer acceptance are still required. No machine-wide compiler setting changes.
