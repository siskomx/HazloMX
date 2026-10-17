# Chat recovery after a process crash

The packaged Arduino test previously lost its final chat confirmation after
a forced shutdown, while preserving the engineering result. The simple chat
now writes validated conversation snapshots to IndexedDB, requests strict
durability, and displays its saved acknowledgement only after transaction
completion. The old localStorage copy remains a compatibility cache.

The [IndexedDB specification](https://w3c.github.io/IndexedDB/#enumdef-idbtransactiondurability)
describes strict durability as a hint intended to flush pending I/O before
the completion event. This is not a guarantee against every hardware or
power failure. The tests below verify process-crash behavior directly.

## Change and boundaries

The existing project UUID/folder/provider key and bounded conversation
format remain unchanged. Writes are ordered; aborted writes preserve the
previous committed value. Unsupported strict durability is refused rather
than acknowledged as a successful save. Restore prefers the committed
snapshot over an older compatibility cache and migrates a valid older cache
when no committed record exists. Chat waits for its scoped history load;
callbacks from previous scopes cannot replace its messages or save status.
An unreadable durable history is not automatically overwritten from cache.
Storage failures leave a visible warning to preserve important messages.

There are no new backend commands, permissions, dependencies, engineering
files, models or editor controls. History still cannot restore an approval,
resume a native session or prove the current software document's state.

## Browser checks

Run `rtk proxy node apps/desktop/tests/chat-storage.mjs`. The test uses an
owned persistent headless Edge profile under ignored test storage. It checks
completion-only acknowledgement, aborted-write preservation, refusal when
strict durability is unavailable, ordered writes after failure, invalid and
oversized input rejection, scope isolation and exact Unicode recovery after
crashing the entire owned browser process. localStorage is empty during this
recovery test, so it cannot mask a missing database record.

The initial harness waited for a response from the browser it had crashed;
the process had already gone. A subsequent harness API mistake also failed.
Both are retained as harness failures, not product passes. The corrected
test observes browser disconnection and completes successfully.

The extended `chat-history.mjs` checks the actual frontend's pending-save
status before releasing the completion callback. After acknowledgement it
deliberately replaces the cache with older text; reload still restores the
committed conversation. Existing legacy migration, provider/folder isolation,
escaping, approval exclusion and failure checks pass. The full chat-flow
browser suite, bounded-context check and production frontend build pass.

## Actual Windows WebView2

The [sanitized checkpoint](durable-chat-2026-10-16.json) identifies the tested
debug executable and source hashes. It reuses the owned three-job Arduino
project and profile; debug project preselection avoids repeating the picker.
The existing cloud provider and Arduino Agent are chosen in the actual UI.
The earlier conversation migrates, and a new read-only question receives a
two-sentence, 45-word review without internal result fields. The UI visibly
acknowledges its save before the supervised app tree is force-stopped.

Restarting the same binary/profile and choosing the same provider restores
all 14 user/assistant messages in exactly the same order, including the new
question and final reply. This is a direct message comparison, not just a
completed-job check. No new engineering jobs appear; all three completed
jobs and all seven project files remain unchanged through forced shutdown
and subsequent normal close. Owned test processes are stopped. Raw profiles,
captures and conversations remain private local artifacts.

The native run retains its compatibility cache; the separate browser test
proves recovery without that cache. No exact model tool trace was collected
for the native storage test. No power-loss, hardware or installed-package
acceptance is claimed. The verified installer predates this change and the
full native multi-program goal remains unfinished.
