# Drawing unit mismatches explain the correction

Both DXF-header inspection and the native CADCraft DXF/DWG unit query now use the same unit check. A mismatched inch or millimeter drawing tells the user which unit the software found and which choice to select under **Unidades del dibujo original**. It explains that circle measurements can retain their separate input unit. This is guidance, not automatic source reinterpretation or rescaling.

A declared unsupported unit asks the user to convert a copy in the original program and confirm its physical size. Native unit-query warnings have a separate message indicating that scale is not yet reliably confirmed. A warning no longer implies a known unit mismatch. Existing source preservation, request validation and approval boundaries remain in place; no native formats or unit codes were added.

Validation:

- 111 regular Rust tests pass. New checks cover matching millimeter/inch units, unitless-source behavior, both actionable mismatch messages and unsupported units. A wrong-unit DXF request rejects before capture or CLI invocation, leaving its source bytes unchanged.
- The actual identified CADCraft 0.5.0 CLI conversion/circle gate passes. Its new DWG check uses the real inch DWG produced by the gate, rejects a millimeter request with the inch-specific message, preserves that saved DWG, and accepts the confirmed inch request. Metric/inch conversions, typed native circles, loss consent and artifact-validation checks remain passing.
- Formatting, all-target desktop Clippy and all nine browser workflow fixtures pass.

The retained native DWG files and [hashes](file-hashes.json) supplement the test assertions; [status](status.json) records their scope. Runtime/version identities retain the earlier pinned CADCraft evidence. No AI or hardware call runs.

The current `0cdc30ad…` installer predates this backend message change. Native source-file pickers, signed-in conversational correction, updated package acceptance and broader drawing fidelity remain unfinished. The full application goal stays active.
