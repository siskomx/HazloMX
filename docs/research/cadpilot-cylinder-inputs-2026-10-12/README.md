# CADPilot cylinder measurements and native readback

The manual CADPilot form asks for the cylinder's **diameter (full width)** and height, with horizontal/depth/elevation positions measured from the document origin. It explains that placement refers to the box's starting corner or cylinder's base center. Positions begin blank and must be confirmed; inspection remains the starting operation when measurements are unknown.

Changing units preserves entered physical measurements for both a newly selected document and a verified-result follow-up, keeping blank inputs blank. The form translates diameter to the existing typed radius request; incoming agent or recovered proposals translate radius back to displayed diameter. Approval previews use diameter, including its millimeter equivalent. Backend schemas and agent tools retain their existing radius contract.

The nine-workflow browser fixture passes unknown-position refusal before dispatch, inch/mm/in preservation without a follow-up, a two-inch-diameter request translating to one-inch radius, incoming proposal display, disabled approval-stage fields and escaped previews. The retained screenshot was visually checked after correcting the position fieldset's layout. TypeScript/production build and all-target desktop Clippy pass.

The identified FreeCAD 1.1.4 / CADPilot 0.7.7 native adapter test passes both its existing box edit and the added cylinder operation. With radius 1 inch, height 1 inch and base position (1, 0, 0) inches, actual software readback reports one valid solid with bounds X 0–50.8 mm, Y −25.4–25.4 mm, Z 0–25.4 mm. Result validation and source-byte preservation pass. `native/` retains the actual FCStd/STEP, input and result from that gate; the screenshot uses browser IPC fixtures and does not prove native conversational use.

The current `0cdc30ad…` installer predates these source changes. Updated installer acceptance, native picker/conversational execution and live unsaved-document editing remain unfinished. The full application goal stays active.
