# CADCraft Windows CLI gate

October 11, 2026. Actual Windows x64 host execution of the official portable CADCraft 0.5.0 package, commit `315751e3ec98e708bcc831dd7f80ca0433a2e3d0`. This establishes a bounded CLI integration basis; it does not establish an implemented Hazlo Mexa workflow. [Measured assertions](result.json), [actual invocations](calls.json).

The 21,587,357-byte archive matches the release's SHA256 `ce849d6ad1145a492b9b04bdab0ff36ee0e1cf72410d5e3a491ad006ff125614`. Both Windows executables have Valid Authenticode signatures from Learning Machines Inc at this check. [Package identity](package.json), [signature observations](signatures.json), [official release](https://github.com/storytold/cadcraft/releases/tag/v0.5.0).

Passing checks use fresh test-owned files only:

- Exact CLI version and unique command catalog with required inspect, units, circle, save and document-byte commands.
- Metric and imperial circle drawings: `$INSUNITS` 4 (millimetres) and 1 (inches). DXF copy and DXF → DWG → DXF preserve these headers, one Circle and its exact extents. This does not test automatic unit conversion or feet/metres.
- SVG XML and 2400×1600 PNG outputs. The metric PNG was also visually inspected: a complete circle is framed within the image.
- An imported unsupported 3DSOLID plus a supported Circle is accepted by the reader. Conversion exits **zero**, emits a `Not saved ... 1 3DSOLID` warning and drops one entity. The original remains byte-identical. Consequently an exit code alone cannot establish losslessness, and preview export does not establish that foreign objects are rendered.
- Malformed input, unknown options and non-finite framing reject with no output at the requested destination.
- Conversion to an existing test-owned destination **overwrites it**. The application must reserve empty staging and use its existing nonreplacing publication; upstream CLI behavior cannot protect user destinations.

The CLI `info` response does not report insertion units in this observed version. Use a validated unit declaration from the imported drawing where supported and obtain explicit interpretation when units are missing or ambiguous. Do not infer units from numeric extents. Supported drawing checks above are deliberately small: equal entity counts/extents do not establish preservation of every entity property, annotation, block, layout or constraint.

Reproduce after provisioning the identified portable runtime at `target/cadcraft-probe/runtime/cadcraft-0.5.0-windows-x64-portable`: `rtk proxy python tests/cadcraft/runtime_probe.py`. Each invocation creates a fresh owned UUID directory and asserts the actual behavior. `rtk proxy pwsh -NoProfile -File tests/cadcraft/signatures.ps1` records current signature validation separately. Initial PowerShell 5 signature-module loading failed; the successful check used PowerShell 7. No signing result is inferred from the release commit signature.

Next application binding must reuse the owned-process supervisor, import/pin an explicitly selected source, inspect/preview a private copy, bind version/source identity/output format/loss expectations to one-use approval, validate outputs and retain actionable cancellation/recovery. The CLI's fixed file workflow needs no live GUI port. Approved headless edits must expose bounded typed commands; do not expose arbitrary script/command or general MCP file operations to frontend input.

Still unverified: complex drawing loss/property fixtures, actual CLI process cancellation and uncertain timeout state, GUI editing/control-port boundary, second Windows environment, native Hazlo Mexa UI and installer workflow. No GUI server, AI models or hardware control ran. No production adapter was enabled by this gate.
