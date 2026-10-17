# CADCraft: a units query must not edit the drawing

While investigating read-only access to an open CADCraft drawing, the pinned
0.7.0 engine's `units` implementation was found to take a mutable drawing
reference even when its parameters were empty. The native session uses
copy-on-write drawing identity for dirty state, revisions and Undo. A query
therefore made a clean drawing dirty and invalidated the new checked binding.

The real native-control regression fails before the correction, observing
changed dirty/revision state. The correction is two insertions and three
deletions in the isolated engine's command implementation: acquire the
mutable drawing only when applying an actual numeric units parameter, and
borrow it immutably for the response. Parameter values, clamping, result
schema and real setting operations retain their existing behavior.

## Verification

The user-authorized isolated native patch trial now passes **13 binding
tests and 4 existing protocol tests**. The added cases establish:

- An empty units query preserves dirty state, revision, Undo/Redo and the
  previously captured native checked binding.
- A real units change still invalidates the old binding, records one Undo
  step and restores its original value/clean state through native Undo.
- A units query during a waiting LISP routine preserves its prompt and
  drawing; the person can finish the routine and Undo its actual edit.

The changed native files pass focused formatting. A fresh patch-application
check reproduces all five changed trial-source files from pinned upstream
commit `fc4e98315a8d771b5ab832f0ade0b5e7b106df7f`.
The [sanitized checkpoint](cadcraft-native-binding-2026-10-16/readonly-units.json)
retains the source identity, patch digest, scope and false completion flags.
Raw compiler/test logs remain ignored local artifacts. The first edit helper
refused a line-ending mismatch without changing the native implementation;
preserving the source's CRLF endings corrected that helper.

## Applying the trial

Apply the retained [native binding patch](cadcraft-native-binding-2026-10-16/native-binding.patch),
[GUI profile-isolation patch](cadcraft-native-binding-2026-10-16/gui-profile-isolation.patch)
and then the incremental [read-only units patch](cadcraft-native-binding-2026-10-16/readonly-units.patch).
The last patch extends the binding tests created by the first. Use the
existing isolated Rust 1.95 toolchain and target directory; the default
toolchain and installed CADCraft remain unchanged.

The focused native command is `cargo test --locked -p cadcraft-ui-egui
--test control_binding --test control_methods -j 1` from the isolated source.
This is an in-process native-control test, not a new network/native-window
or Hazlo conversation test. Full upstream CI and upstream publication were
not performed. No production live adapter or patched default runtime is
enabled. Adoption remains the existing pending user choice.

Hazlo application and installer source are unchanged by this supplement;
the verified installer retains its previous acceptance. This closes a
specific inspection-side-effect gap, not the full CADCraft live-chat goal.
