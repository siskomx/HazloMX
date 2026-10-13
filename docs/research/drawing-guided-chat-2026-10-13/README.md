# Guided circle measurements

Guided chat now gathers a circle's full width and horizontal/vertical center position after source discovery. Each answer requires a number and an explicit metric or imperial unit. Unknown dimensions stay unknown; invalid or unsupported inputs preserve the draft. Negative center positions are accepted, while diameter must be positive.

The caller converts confirmed values into the draft's current measurement unit, using the existing conversion and proposal flow. Chat does not prepare or approve an operation. Pending clarification clears on project/source/runtime/provider/workflow/new-conversation changes. Running or awaiting-approval jobs refuse draft edits.

Focused parser assertions and the nine-workflow headless browser regression passed, covering multi-turn circle entry, converted draft values, missing/invalid inputs, no implicit preparation and new-conversation reset. TypeScript and the production frontend build pass. Screenshots remain private in the ignored test output.

The final regression rerun also passed after redundant reset statements were removed. The installer rebuild is pending. Native guided-chat interaction, signed-in interpretation and this package's second-Windows acceptance are not established by these browser fixtures.
