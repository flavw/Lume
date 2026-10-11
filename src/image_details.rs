use crate::backend;
use std::path::Path;
pub fn information(
    path: Option<&Path>,
    pixels: &image::RgbaImage,
) -> backend::Result<Vec<(String, String)>> {
    let mut rows = vec![
        (
            "Dimensões".into(),
            format!("{} × {} px", pixels.width(), pixels.height()),
        ),
        (
            "Transparência".into(),
            if pixels.pixels().any(|p| p[3] < 255) {
                "Sim"
            } else {
                "Não"
            }
            .into(),
        ),
    ];
    if let Some(path) = path {
        rows.insert(0, ("Arquivo".into(), path.display().to_string()));
        let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
        rows.push((
            "Tamanho".into(),
            format!(
                "{} bytes ({:.2} MiB)",
                meta.len(),
                meta.len() as f64 / 1048576.0
            ),
        ));
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        let format = image::guess_format(&data)
            .map(|f| format!("{f:?}"))
            .unwrap_or_else(|_| {
                backend::command("magick")
                    .args(["identify", "-format", "%m"])
                    .arg(path)
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                    .unwrap_or_else(|| "Desconhecido".into())
            });
        rows.push(("Formato real".into(), format));
        for (key, text) in crate::generation::png_text(&data) {
            let label = if matches!(key.as_str(), "parameters" | "prompt" | "workflow") {
                format!("Geração por IA · {key}")
            } else {
                format!("PNG · {key}")
            };
            rows.push((label, text));
        }
        if let Some(exif) = crate::generation::exif_from_image(&data) {
            for field in exif.fields() {
                rows.push((
                    format!("EXIF · {}", field.tag),
                    if field.tag == exif::Tag::UserComment {
                        if let exif::Value::Undefined(bytes, _) | exif::Value::Byte(bytes) =
                            &field.value
                        {
                            crate::generation::user_comment(bytes)
                        } else {
                            field.display_value().with_unit(&exif).to_string()
                        }
                    } else {
                        field.display_value().with_unit(&exif).to_string()
                    },
                ));
            }
        }
    } else {
        rows.push(("Origem".into(), "Documento em memória".into()));
    }
    Ok(rows)
}
/// Save using the real source format, rejecting animation/multipage data.
pub fn save_original(pixels: &image::RgbaImage, path: &Path, quality: u8) -> backend::Result<()> {
    let original = std::fs::read(path).map_err(|e| e.to_string())?;
    let output = backend::command("magick")
        .args(["identify", "-format", "%m|%n\n"])
        .arg(path)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Não foi possível identificar o formato original.".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 1 {
        return Err("Salvar no original não está disponível para imagens animadas ou multipáginas. Exporte uma cópia.".into());
    }
    let (coder, count) = lines[0]
        .split_once('|')
        .ok_or("Formato original inválido")?;
    if count != "1"
        || !matches!(
            coder,
            "JPEG" | "PNG" | "WEBP" | "AVIF" | "HEIC" | "BMP" | "TIFF" | "GIF" | "ICO"
        )
    {
        return Err("Formato original não disponível para salvar. Exporte uma cópia.".into());
    }
    let input = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|e| e.to_string())?;
    pixels
        .save_with_format(input.path(), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let temp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| e.to_string())?;
    let mut cmd = backend::command("magick");
    cmd.arg(input.path());
    if coder == "JPEG" {
        cmd.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
    }
    cmd.args(["-strip", "-quality", &quality.to_string()])
        .arg(format!("{coder}:{}", temp.path().display()));
    let result = cmd.output().map_err(|e| e.to_string())?;
    if !result.status.success() {
        return Err(String::from_utf8_lossy(&result.stderr)
            .chars()
            .take(1000)
            .collect());
    }
    if std::fs::read(path).map_err(|e| e.to_string())? != original {
        return Err("O arquivo foi alterado por outro programa. O original foi preservado.".into());
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saving_an_animation_preserves_original_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("animation.gif");
        let pixels = image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]));
        {
            let file = std::fs::File::create(&path).unwrap();
            let mut encoder = image::codecs::gif::GifEncoder::new(file);
            encoder
                .encode_frame(image::Frame::new(pixels.clone()))
                .unwrap();
            encoder
                .encode_frame(image::Frame::new(pixels.clone()))
                .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        assert!(save_original(&pixels, &path, 80).is_err());
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
    #[test]
    fn save_uses_signature_not_extension_and_information_reports_alpha() {
        let temp = tempfile::tempdir().unwrap();
        let p = temp.path().join("renamed.jpg");
        let mut pixels = image::RgbaImage::from_pixel(10, 12, image::Rgba([20, 40, 60, 128]));
        pixels
            .save_with_format(&p, image::ImageFormat::Png)
            .unwrap();
        pixels.put_pixel(0, 0, image::Rgba([255, 0, 0, 64]));
        save_original(&pixels, &p, 80).unwrap();
        assert_eq!(
            image::guess_format(&std::fs::read(&p).unwrap()).unwrap(),
            image::ImageFormat::Png
        );
        let decoded = backend::load(&p).unwrap();
        assert_eq!(decoded.pixels.get_pixel(0, 0).0, [255, 0, 0, 64]);
        let info = information(Some(&p), &decoded.pixels).unwrap();
        assert!(info.iter().any(|(k, v)| k == "Formato real" && v == "Png"));
        assert!(info.iter().any(|(k, v)| k == "Transparência" && v == "Sim"));
    }
}
