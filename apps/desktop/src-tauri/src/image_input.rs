//! Bounded, inline references only: no agent-selected paths or remote image URLs.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Deserialize;

pub const MAX_BYTES: usize = 512 * 1024;
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageInput {
    pub media_type: String,
    pub data: String,
}
impl ImageInput {
    pub fn validate(&self) -> Result<(), String> {
        let invalid = || "Adjunta una imagen PNG o JPEG válida, de hasta 1536 píxeles por lado y 512 KiB.".to_string();
        if self.data.is_empty() || self.data.len() > MAX_BYTES.div_ceil(3) * 4 {
            return Err(invalid());
        }
        let bytes = STANDARD.decode(&self.data).map_err(|_| invalid())?;
        if bytes.len() > MAX_BYTES { return Err(invalid()); }
        let dimensions = match self.media_type.as_str() {
            "image/png" if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 45
                && bytes[12..16] == *b"IHDR" && bytes[bytes.len()-8..bytes.len()-4] == *b"IEND" =>
                Some((u32::from_be_bytes(bytes[16..20].try_into().unwrap()), u32::from_be_bytes(bytes[20..24].try_into().unwrap()))),
            "image/jpeg" if bytes.starts_with(&[255,216]) && bytes.ends_with(&[255,217]) => jpeg_dimensions(&bytes),
            _ => None,
        }.ok_or_else(invalid)?;
        if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 1536 || dimensions.1 > 1536 { return Err(invalid()); }
        Ok(())
    }
    pub fn data_url(&self) -> String { format!("data:{};base64,{}", self.media_type, self.data) }
}
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32,u32)> {
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes[at] != 255 { return None; }
        if bytes[at+1] == 255 { at += 1; continue; }
        let marker = bytes[at+1]; at += 2;
        if marker == 218 || marker == 217 { return None; }
        let length = u16::from_be_bytes(bytes[at..at+2].try_into().ok()?) as usize;
        if length < 2 || at + length > bytes.len() { return None; }
        if matches!(marker,192..=194) {
            if length < 8 { return None; }
            return Some((u16::from_be_bytes(bytes[at+5..at+7].try_into().ok()?) as u32,u16::from_be_bytes(bytes[at+3..at+5].try_into().ok()?) as u32));
        }
        at += length;
    }
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inline_image_boundary_rejects_paths_mismatch_size_and_malformed_headers() {
        let mut bytes=vec![255,216,255,192,0,8,8,0,1,0,1,0,255,217];
        let image=|bytes:&[u8]|ImageInput{media_type:"image/jpeg".into(),data:STANDARD.encode(bytes)};
        image(&bytes).validate().unwrap();
        assert!(image(&bytes).data_url().starts_with("data:image/jpeg;base64,"));
        bytes[9]=7; assert!(image(&bytes).validate().is_err());
        assert!(image(&[255,216,255,192,255,255]).validate().is_err());
        assert!(image(&vec![0;MAX_BYTES+1]).validate().is_err());
        assert!(ImageInput{media_type:"image/svg+xml".into(),data:STANDARD.encode(b"<svg/>")}.validate().is_err());
        assert!(ImageInput{media_type:"image/jpeg".into(),data:"https://example.invalid/private.jpg".into()}.validate().is_err());
        assert!(serde_json::from_value::<ImageInput>(serde_json::json!({"media_type":"image/jpeg","data":"x","path":"private"})).is_err());
    }
}
