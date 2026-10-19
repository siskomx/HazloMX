//! Inspect native ASCII DXF before a CLI open can follow external references.
const BLOCKED: &str = "La copia DXF se conserva, pero necesita revisar sus dependencias o formato antes de reabrirla. No se cargaron referencias externas ni se repitió la edición.";

pub(crate) fn safe_native_open(bytes: &[u8]) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > 128 * 1024 || bytes.contains(&0) {
        return Err(BLOCKED.into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_| BLOCKED)?;
    let mut lines = text.lines();
    let mut section = false;
    let mut section_name = false;
    let mut eof = false;
    let mut record = String::new();
    while let Some(code) = lines.next() {
        let value = lines.next().ok_or(BLOCKED)?.trim();
        let code: u16 = code.trim().parse().map_err(|_| BLOCKED)?;
        if code > 1071 || eof || value.chars().any(char::is_control) {
            return Err(BLOCKED.into());
        }
        if section_name {
            if code != 2
                || !matches!(
                    value,
                    "HEADER"
                        | "CLASSES"
                        | "TABLES"
                        | "BLOCKS"
                        | "ENTITIES"
                        | "OBJECTS"
                        | "THUMBNAILIMAGE"
                )
            {
                return Err(BLOCKED.into());
            }
            section_name = false;
            continue;
        }
        if code == 0 {
            // Native parsing decodes DXF escapes even in record names. Refuse
            // encoded structure instead of interpreting it differently here.
            if value.contains("\\U+") || value.contains("\\M+") {
                return Err(BLOCKED.into());
            }
            record = value.to_ascii_uppercase();
            match record.as_str() {
                "SECTION" if !section => {
                    section = true;
                    section_name = true;
                }
                "ENDSEC" if section => section = false,
                "EOF" if !section => eof = true,
                "SECTION" | "ENDSEC" | "EOF" => return Err(BLOCKED.into()),
                "IMAGEDEF" | "PDFDEFINITION" | "DGNDEFINITION" | "DWFDEFINITION"
                | "ACAD_PROXY_ENTITY" | "ACAD_PROXY_OBJECT" | "VBA_PROJECT" | "OLE2FRAME"
                | "OLEFRAME" => return Err(BLOCKED.into()),
                _ if !section => return Err(BLOCKED.into()),
                _ => (),
            }
        } else if !section {
            return Err(BLOCKED.into());
        }
        // The reviewed native OPEN loads BLOCK Xrefs automatically. Symbol names
        // with a pipe also depend on an Xref, even when its block was omitted.
        if record == "BLOCK"
            && (code == 1 && !value.is_empty()
                || code == 70 && value.parse::<u16>().map_err(|_| BLOCKED)? & 12 != 0)
            || matches!(code, 2 | 6 | 7 | 8)
                && (value.contains('|')
                    || value.to_ascii_uppercase().contains("\\U+007C")
                    || value.contains("\\M+"))
        {
            return Err(BLOCKED.into());
        }
    }
    if !eof || section || section_name {
        return Err(BLOCKED.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(record: &str) -> Vec<u8> {
        format!("0\nSECTION\n2\nBLOCKS\n{record}0\nENDSEC\n0\nEOF\n").into_bytes()
    }
    #[test]
    fn rejects_native_open_dependencies_before_any_path_is_followed() {
        safe_native_open(&file("0\nBLOCK\n2\nOwned\n70\n0\n1\n\n")).unwrap();
        safe_native_open(&file("0\nBLOCK_RECORD\n2\nOwned\n70\n4\n")).unwrap();
        safe_native_open(&file("0\nLAYER\n2\nPieza\\U+00E1\n")).unwrap();
        assert!(safe_native_open(&file("0\nBL\\U+004FCK\n70\n4\n1\nprivate.dxf\n")).is_err());
        assert!(safe_native_open(&file("0\nLAYER\n2\nSource\\U+007cLayer\n")).is_err());
        for record in [
            "0\nBLOCK\n70\n4\n1\nprivate.dxf\n",
            "0\nBLOCK\n70\n12\n1\n\\\\example.invalid\\private.dxf\n",
            "0\nBLOCK\n70\n0\n1\nrelative.dxf\n",
            "0\nBLOCK\n70\n4\n1\n\n",
            "0\nBLOCK\n70\n-1\n",
            "0\nBLOCK\n70\ninvalid\n",
            "0\nLAYER\n2\nSource|Layer\n",
            "0\nIMAGEDEF\n1\nprivate.png\n",
            "0\nPDFDEFINITION\n1\nprivate.pdf\n",
            "0\nACAD_PROXY_OBJECT\n",
        ] {
            let error = safe_native_open(&file(record)).unwrap_err();
            assert!(!error.contains("private") && !error.contains("example.invalid"));
        }
    }
    #[test]
    fn refuses_ambiguous_or_binary_input_and_preserves_native_fixture_bytes() {
        let fixture =
            include_bytes!("../../../../docs/research/cadcraft-runtime-2026-10-11/metric.dxf");
        let before = fixture.to_vec();
        safe_native_open(fixture).unwrap();
        assert_eq!(fixture.as_slice(), before);
        for bad in [
            b"".as_slice(),
            b"AutoCAD Binary DXF\r\n\x1a\0",
            b"0\nEOF\n0\nBLOCK\n",
            b"0\nSECTION\n2\nBLOCKS\n0\nEOF\n",
            b"0\nSECTION\n2\nUNKNOWN\n0\nENDSEC\n0\nEOF\n",
            b"0\nSECTION\n2\nBLOCKS\n0\nENDSEC\n0\n",
            b"0\nSECTION\n2\nBLOCKS\n1072\nvalue\n0\nENDSEC\n0\nEOF\n",
        ] {
            assert!(safe_native_open(bad).is_err());
        }
    }
}
