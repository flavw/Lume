//! Read embedded generation text without decoding pixels or invoking another process.
use std::io::Read;
const LIMIT: usize = 4 * 1024 * 1024;
fn split_zero(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let i = bytes.iter().position(|b| *b == 0)?;
    Some((&bytes[..i], &bytes[i + 1..]))
}
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}
fn inflate(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(bytes)
        .take((LIMIT + 1) as u64)
        .read_to_end(&mut out)
        .ok()?;
    (out.len() <= LIMIT).then_some(out)
}
pub fn png_text(data: &[u8]) -> Vec<(String, String)> {
    let mut rows = Vec::new();
    if !data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return rows;
    }
    let mut at = 8usize;
    let mut total = 0;
    while let Some(header) = data.get(at..at.saturating_add(8)) {
        let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        let Some(end) = at.checked_add(12).and_then(|n| n.checked_add(len)) else {
            break;
        };
        let Some(chunk) = data.get(at + 8..end.saturating_sub(4)) else {
            break;
        };
        if end > data.len() {
            break;
        }
        let kind = &header[4..8];
        if len <= LIMIT && matches!(kind, b"tEXt" | b"zTXt" | b"iTXt") {
            if let Some((key, rest)) = split_zero(chunk) {
                let text = match kind {
                    b"tEXt" => Some(latin1(rest)),
                    b"zTXt" => rest
                        .strip_prefix(&[0])
                        .and_then(inflate)
                        .map(|b| latin1(&b)),
                    b"iTXt" => (|| {
                        let (&flag, rest) = rest.split_first()?;
                        let (&method, rest) = rest.split_first()?;
                        if method != 0 || flag > 1 {
                            return None;
                        }
                        let (_, rest) = split_zero(rest)?;
                        let (_, text) = split_zero(rest)?;
                        if flag == 1 {
                            inflate(text).and_then(|b| String::from_utf8(b).ok())
                        } else {
                            String::from_utf8(text.to_vec()).ok()
                        }
                    })(),
                    _ => None,
                };
                if let Some(text) = text {
                    total += text.len();
                    if total > LIMIT {
                        break;
                    }
                    rows.push((latin1(key), text));
                }
            }
        }
        at = end;
        if kind == b"IEND" {
            break;
        }
    }
    rows
}
pub fn exif_from_image(data: &[u8]) -> Option<exif::Exif> {
    if let Ok(exif) = exif::Reader::new().read_from_container(&mut std::io::Cursor::new(data)) {
        return Some(exif);
    }
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        let mut at = 12usize;
        while let Some(header) = data.get(at..at.checked_add(8)?) {
            let len = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
            let end = at.checked_add(8)?.checked_add(len)?;
            let chunk = data.get(at + 8..end)?;
            if &header[..4] == b"EXIF" {
                return exif::Reader::new()
                    .read_raw(chunk.strip_prefix(b"Exif\0\0").unwrap_or(chunk).to_vec())
                    .ok();
            }
            at = end.checked_add(len % 2)?;
        }
    }
    None
}
pub fn user_comment(data: &[u8]) -> String {
    if let Some(text) = data.strip_prefix(b"ASCII\0\0\0") {
        return String::from_utf8_lossy(text)
            .trim_end_matches('\0')
            .to_owned();
    }
    if let Some(mut text) = data.strip_prefix(b"UNICODE\0") {
        let little = text.starts_with(&[0xff, 0xfe]);
        if text.starts_with(&[0xfe, 0xff]) || little {
            text = &text[2..];
        }
        let units: Vec<u16> = text
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| {
                if little {
                    u16::from_le_bytes([p[0], p[1]])
                } else {
                    u16::from_be_bytes([p[0], p[1]])
                }
            })
            .collect();
        return String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_owned();
    }
    String::from_utf8_lossy(data)
        .trim_end_matches('\0')
        .to_owned()
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    fn png(chunks: Vec<(&[u8], Vec<u8>)>) -> Vec<u8> {
        let mut data = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, body) in chunks {
            data.extend((body.len() as u32).to_be_bytes());
            data.extend(kind);
            data.extend(body);
            data.extend([0; 4]);
        }
        data
    }
    #[test]
    fn real_png_jpeg_webp_generation_metadata_is_read() {
        let pixels = image::RgbaImage::new(16, 16);
        for name in ["generation.png", "generation.jpg", "generation.webp"] {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name);
            let rows = crate::image_details::information(Some(&path), &pixels).unwrap();
            assert!(
                rows.iter().any(|(_, v)| v.contains("gatinho café 日本")
                    && v.contains("Seed: 42")
                    && v.contains("Negative prompt: blur")),
                "missing prompt in {name}: {rows:?}"
            );
        }
    }
    #[test]
    fn png_text_variants_preserve_prompt_and_json() {
        let prompt = "cat\nNegative prompt: blur\nSteps: 20, Seed: 42";
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(prompt.as_bytes()).unwrap();
        let compressed = z.finish().unwrap();
        let data = png(vec![
            (
                b"tEXt",
                [b"parameters\0".as_slice(), prompt.as_bytes()].concat(),
            ),
            (
                b"zTXt",
                [b"parameters\0\0".as_slice(), &compressed].concat(),
            ),
            (
                b"iTXt",
                [
                    b"workflow\0\0\0\0\0".as_slice(),
                    "{\"prompt\":\"gatinho café\"}".as_bytes(),
                ]
                .concat(),
            ),
        ]);
        let rows = png_text(&data);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].1, prompt);
        assert_eq!(rows[1].1, prompt);
        assert!(rows[2].1.contains("café"));
        assert!(png_text(&data[..12]).is_empty());
    }
    #[test]
    fn exif_unicode_comment_is_decoded() {
        let text = "gatinho 日本\nSeed: 42";
        let mut bytes = b"UNICODE\0".to_vec();
        for unit in text.encode_utf16() {
            bytes.extend(unit.to_be_bytes());
        }
        assert_eq!(user_comment(&bytes), text);
        assert_eq!(user_comment(b"ASCII\0\0\0cat"), "cat");
    }
}
