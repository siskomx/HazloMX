# Arduino readback timing and desktop prerequisite

Actual owned Arduino Agent 0.7.1 runs now reach the companion and VS Code API 1.96.0. The fixture reads the owned sketch with editor autosave off and preserves its saved bytes, but the native edit/Undo gate has **not passed**. One setup edit was refused. Another returned `true` while the immediate document API observation still had the original revision and source. Other runs, including a fresh portable profile, stopped at their supervised deadline before the companion became ready. These are failed observations, not visible-editor acceptance.

The companion now waits for bounded document notifications after a positive edit reply, only while the observed identity, name, line ending, revision and source remain exactly the prior state. It never resends the edit. Matching source with an increased revision still must pass independent host verification; a changed context, conflicting source or acknowledgement without a source change remains uncertain. The wait does not establish that delayed notifications caused the earlier real IDE failure.

The test probe now schedules its exercise after returning from extension activation, waits for a stable active owned buffer before setup, and checks setup/Undo source notifications without repeating either mutation. These harness changes still require actual unlocked IDE validation. No live Agent catalog tool is enabled.

## Verified desktop prerequisite

A read-only Windows session query reports the current session locked. The executor is not elevated, and the existing process runner does not set a hidden-window startup flag. Native activation had failed after refreshing the owned window selection. The current locked status prevents further UI work; it does not prove the cause of every earlier failure.

The owned supervisor now queries the typed Windows session API before creating a GUI process, refuses locked, disconnected or unknown readiness, and cancels its owned test tree if the desktop becomes unavailable during a run. Its actual locked-session preflight returns `desktop_not_unlocked` with `launched:false`. User authentication is never automated and no security or privacy preference is changed. Experimental graphics/background overrides were removed from the final launcher so the next test uses the application's default rendering behavior.

Microsoft documents the [session flags](https://learn.microsoft.com/en-us/windows/win32/api/wtsapi32/ns-wtsapi32-wtsinfoex_level1_w) used by this test-only prerequisite. The Windows 10/11 flag interpretation is used; this is not a general legacy-Windows launcher.

## Checks and remaining work

- All fourteen Node buffer/transport tests pass, including a delayed notification, an acknowledgement without any source change, and changed document names/line endings. The compiled Rust host also completes its two approved backend jobs, verified-result reads, saved-byte preservation and restart recovery through the actual companion transport against the controlled editor API.
- The owned supervisor builds and passes Clippy. The actual locked desktop refuses GUI launch. Probe source passes Node syntax checking.
- An owned fixture credential encountered in diagnostic output was confirmed to belong to that private profile and rotated before further runs. Subsequent log inspection reports known event names/counts instead of raw lines. Credentials, profiles, captures and raw logs remain ignored and are not part of this publication.

The user has been asked to unlock Windows. All launched owned Agent fixtures are terminal; there is no live IDE test being awaited. Visible editing, native Undo, current-state follow-up and the broader discovery/chat/installer gates remain unfinished. The full application goal stays active.
