# Long conversations and result review — October 14, 2026

The real CADCraft release UI uncovered a conversation-length failure: two approved native edits completed, but the second automatic result explanation exceeded the 8 KiB request bound. That native gate remains **failed**, with its original observation retained privately. The source correction passes focused, production-browser and legacy-history checks; a rebuilt native/installed retest remains required.

## Observed native behavior

The identified unbundled app `fbe607af…` used the existing Ollama cloud route directly, with native project/runtime/source pickers and no scripted replies or fixture approvals. The AI inspected the selected circle, explained its actual dimensions and asked for missing direction/distance before proposing an edit. A native approval moved it one inch right; automatic result review correctly described the verified center at `(25.4, 0)` mm and the saved-copy limitation.

A subsequent 5 mm proposal was denied through the native Cancel button. It published no artifacts. A fresh result-bound proposal was approved and completed at `(30.4, 0)` mm, but its automatic AI review failed before dispatch because the assembled conversation exceeded 8 KiB. This is a failed conversation gate despite successful geometry.

An independent CADCraft CLI reread of the original and both saved outputs confirms the complete entity records: only the circle's horizontal center changes, its diameter remains 20 mm and its vertical position and other properties remain unchanged. Original bytes and all four first-result files remain identical. Fifteen persistent project files survive owned test-process cleanup. Raw UI trees, projects, profiles, logs and the original failed receipt remain under ignored `target/`.

## Source correction

- Keep the existing 8 KiB backend request bound and the complete current request/document/job context. Never shorten those fields to fit.
- When recovered conversation is too large, omit complete older assistant explanations first, preserving user requests and confirmed constraints where they fit. If still necessary, omit the oldest user messages. Retained messages keep their original order and text.
- Mark omitted context explicitly for the model and in the chat. The model must reread the native document or verified result and clarify missing constraints instead of assuming omitted facts. Automatic result review remains bound to the explicitly approved job and cannot propose or execute another change.
- Preserve the visible conversation independently. Current chat storage accepts up to 512 KiB and 128 messages; quota/size failures show a warning and preserve the last stored value. The legacy parser retains its original 8 KiB default. No history migration, approval restoration, automatic retry or provider expansion is introduced.

This is bounded context selection, not a semantic summary or unlimited memory. Omitted constraints may require clarification. Original messages stay visible during the session and recover when storage succeeds within the stated limits.

## Verification and limits

[Focused assertions](../../apps/desktop/tests/chat-context.mjs) cover UTF-8 byte bounds, preservation of current job/source/request, user constraints before verbose replies, ordered complete messages, oversized individual replies, newest-user retention, unchanged input history and separate storage/legacy limits.

The [production browser matrix](../../apps/desktop/tests/chat-flow.mjs) passes all four AI choices, six program groups and 36 approval/result-review handoffs. The new regression reproduces a long conversation, dispatches exactly one bounded automatic review, keeps all earlier user requests and visible messages, saves history beyond 8 KiB and restores it in the matching project folder/provider without a model request or approval replay. The first recovery assertion reopened the fixture's default folder instead of the copied-project folder; that failure is retained, and the corrected test explicitly checks both isolation and matching-folder recovery. [Legacy-history checks](../../apps/desktop/tests/chat-history.mjs) and the TypeScript/production build also pass.

The corrected source has not passed native or installed acceptance. Escape stopped native UI automation again; no later UI input was issued. The previously tested installer `6a2f7ef9…` predates this correction and remains a separately verified baseline. Real Agent conversation, broader program/provider UI, running-operation UI cancellation and live/unsaved editing remain unfinished. Three additional isolated host Agent launch variants also fail authenticated readiness and stop cleanly; they establish no inference, edit or host startup fix.

The [sanitized receipt](chat-context-window-2026-10-14.json) separates the failed native run from the passing source checks. The full application goal remains active.
