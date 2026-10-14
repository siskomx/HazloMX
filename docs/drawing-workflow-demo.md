# Change a drawing through chat

Open **Conexiones**, create/open a project, select CADCraft and use **Elegir CADCraft…** followed by **Seleccionar dibujo…** for your saved DXF/DWG. The identified CADCraft CLI 0.5.0 runtime is a separate prerequisite. Choose your configured AI and describe the intended change; the current chat has no geometry input form.

For example, say: “Explain the lines in this drawing. I want to move the horizontal one to the right, but I do not know its internal name.” The AI reads CADCraft's native command catalog and measured entities, resolves the handle internally and asks which line or distance is still ambiguous. Confirm “move that line 1 inch right, preserving its length and height,” then ask it to prepare a proposal.

Confirm the source's units if unspecified. File extents alone do not establish physical scale. Input measurements may use millimeters or inches independently of the drawing's units; the proposal must show the converted destination. For a circle, describe its full width (diameter) and center relative to the drawing origin. You do not need to provide a native radius parameter.

Review the Spanish change facts and expandable technical details in Hazlo. Confirm any known conversion loss before approving. CADCraft performs the reviewed native commands and saves a separate output; Hazlo compares native in-memory and saved geometry before publication. A native error is not a completed result.

After approval, let the AI read the verified result before explaining it. For a follow-up, say “move the same line another 5 mm right, starting from the copy we just changed.” It must read that completed result and prepare a separately approved operation bound to its job/report hash. The original and previous result stay available. PNG previews cannot serve as editable follow-up sources.

The [real existing-cloud drawing conversation](research/drawing-real-chat-2026-10-14.md) passes native catalog/geometry inspection, missing-input clarification, the inch-based move and metric follow-up, exact unrelated-entity preservation and recovery. It uses backend fixture approval. Broader command parameters, complex-format fidelity, other providers, current installed/native UI and live unsaved editing remain unfinished. See [first workflow](first-workflow-demo.md) for shared connection, cancellation and package limits.
