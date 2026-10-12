use crate::model::Crop;
use image::{ImageDecoder, RgbaImage};
use std::{
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

pub type Result<T> = std::result::Result<T, String>;

pub fn tool(name: &str) -> PathBuf {
    let env_name = format!("LUMEVIEW_{}", name.to_uppercase());
    if let Some(path) = std::env::var_os(env_name) {
        return path.into();
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    for base in [
        exe.parent().unwrap_or(Path::new(".")).to_owned(),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")),
    ] {
        for relative in [
            format!("tools/{name}/{filename}"),
            format!("tools/{filename}"),
        ] {
            let p = base.join(relative);
            if p.is_file() {
                return p;
            }
        }
        if name == "magick" {
            let p = base.join("tools/imagemagick").join(&filename);
            if p.is_file() {
                return p;
            }
        }
    }
    filename.into()
}

pub(crate) fn command(name: &str) -> Command {
    let mut c = Command::new(tool(name));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    c
}

fn checked(c: &mut Command) -> Result<Vec<u8>> {
    let o = c.output().map_err(|e| {
        format!(
            "Não foi possível executar {:?}: {e}. Consulte Ajuda → Sobre → Dependências.",
            c.get_program()
        )
    })?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr)
            .chars()
            .take(1500)
            .collect());
    }
    Ok(o.stdout)
}

#[derive(Clone)]
pub struct Loaded {
    pub pixels: Arc<RgbaImage>,
    pub jpeg: Option<JpegInfo>,
}

pub fn load(path: &Path) -> Result<Loaded> {
    if let Some(r) = crate::comic::load(path) {
        return r;
    }
    load_disk(path)
}
pub(crate) fn load_disk(path: &Path) -> Result<Loaded> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let jpeg = if data.starts_with(&[0xff, 0xd8]) {
        jpeg_info(&data).ok()
    } else {
        None
    };
    let format = image::guess_format(&data).ok();
    if matches!(
        format,
        Some(
            image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Png
                | image::ImageFormat::Gif
                | image::ImageFormat::Bmp
                | image::ImageFormat::Tiff
                | image::ImageFormat::Ico
        )
    ) {
        return load_native(&data, jpeg);
    }
    load_magick(path, jpeg)
}

fn load_native(data: &[u8], jpeg: Option<JpegInfo>) -> Result<Loaded> {
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_decoder()
        .map_err(|e| e.to_string())?;
    let orientation = decoder.orientation().map_err(|e| e.to_string())?;
    let icc = decoder.icc_profile().map_err(|e| e.to_string())?;
    let mut decoded = image::DynamicImage::from_decoder(decoder).map_err(|e| e.to_string())?;
    decoded.apply_orientation(orientation);
    let mut pixels = decoded.into_rgba8();
    if let Some(icc) = icc {
        // Profiles are handled in-process; ordinary JPEG/WebP never spawn ImageMagick.
        if let Ok(profile) = moxcms::ColorProfile::new_from_slice(&icc) {
            if profile.color_space == moxcms::DataColorSpace::Rgb {
                if let Ok(transform) = profile.create_transform_8bit(
                    moxcms::Layout::Rgba,
                    &moxcms::ColorProfile::new_srgb(),
                    moxcms::Layout::Rgba,
                    moxcms::TransformOptions::default(),
                ) {
                    let mut output = vec![0; pixels.as_raw().len()];
                    transform
                        .transform(pixels.as_raw(), &mut output)
                        .map_err(|e| e.to_string())?;
                    pixels = RgbaImage::from_raw(pixels.width(), pixels.height(), output)
                        .ok_or("Perfil ICC inválido")?;
                }
            }
        }
    }
    Ok(Loaded {
        pixels: Arc::new(pixels),
        jpeg,
    })
}

pub fn load_magick(path: &Path, jpeg: Option<JpegInfo>) -> Result<Loaded> {
    // Fallback only for formats without a native decoder, notably AVIF/HEIC.
    // Raw RGBA avoids the former encode-PNG/decode-PNG round trip.
    let path = std::path::absolute(path).map_err(|e| e.to_string())?;
    let source = format!("{}[0]", path.to_string_lossy());
    let raw = checked(
        command("magick")
            .args(["-limit", "memory", "512MiB", "-limit", "map", "1GiB"])
            .arg(source)
            .args([
                "-auto-orient",
                "-colorspace",
                "sRGB",
                "-depth",
                "8",
                "-format",
                "%w %h\n",
                "-write",
                "info:-",
                "rgba:-",
            ]),
    )?;
    let newline = raw
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("Dimensões ausentes")?;
    let header = String::from_utf8_lossy(&raw[..newline]);
    let dims: Vec<u32> = header
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect();
    if dims.len() != 2 {
        return Err("Dimensões inválidas".into());
    }
    let pixels = RgbaImage::from_raw(dims[0], dims[1], raw[newline + 1..].to_vec())
        .ok_or("Dados RGBA inválidos")?;
    Ok(Loaded {
        pixels: Arc::new(pixels),
        jpeg,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Format {
    Jpeg,
    Webp,
    Png,
}
impl Format {
    pub fn ext(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Png => "png",
        }
    }
    pub fn coder(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Png => "png",
        }
    }
}

pub fn export(pixels: &RgbaImage, dest: &Path, format: Format, quality: u8) -> Result<()> {
    export_impl(pixels, dest, format, quality, true)
}
pub fn export_new(pixels: &RgbaImage, dest: &Path, format: Format, quality: u8) -> Result<()> {
    export_impl(pixels, dest, format, quality, false)
}
fn export_impl(
    pixels: &RgbaImage,
    dest: &Path,
    format: Format,
    quality: u8,
    overwrite: bool,
) -> Result<()> {
    let input = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|e| format!("Criar PNG temporário: {e}"))?;
    pixels
        .save_with_format(input.path(), image::ImageFormat::Png)
        .map_err(|e| format!("Gravar PNG temporário: {e}"))?;
    let temp = tempfile::Builder::new()
        .prefix(".lumeview-")
        .suffix(".tmp")
        .tempfile_in(dest.parent().unwrap_or(Path::new(".")))
        .map_err(|e| format!("Criar arquivo de saída: {e}"))?;
    let target = format!("{}:{}", format.coder(), temp.path().to_string_lossy());
    let mut c = command("magick");
    c.arg(input.path());
    if format == Format::Jpeg {
        c.args(["-background", "white", "-alpha", "remove", "-alpha", "off"]);
    }
    c.args(["-strip", "-quality", &quality.to_string()]);
    if format == Format::Webp {
        c.args(["-define", "webp:method=6"]);
    }
    checked(c.arg(target))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| format!("Sincronizar saída: {e}"))?;
    if overwrite {
        temp.persist(dest)
            .map_err(|e| format!("Substituir destino: {}", e.error))?;
    } else {
        temp.persist_noclobber(dest)
            .map_err(|e| format!("Salvar destino: {}", e.error))?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub mcu: (u32, u32),
    pub orientation: u32,
}

pub fn jpeg_info(data: &[u8]) -> Result<JpegInfo> {
    if data.get(..2) != Some(&[0xff, 0xd8]) {
        return Err("Não é um JPEG.".into());
    }
    let orientation = exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(data))
        .ok()
        .and_then(|e| {
            e.get_field(exif::Tag::Orientation, exif::In::PRIMARY)
                .and_then(|f| f.value.get_uint(0))
        })
        .unwrap_or(1);
    let mut pos = 2;
    while pos + 4 <= data.len() {
        if data[pos] != 0xff {
            return Err("Cabeçalho JPEG inválido.".into());
        }
        while pos < data.len() && data[pos] == 0xff {
            pos += 1;
        }
        let marker = *data.get(pos).ok_or("JPEG truncado")?;
        pos += 1;
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let len = u16::from_be_bytes([
            *data.get(pos).ok_or("JPEG truncado")?,
            *data.get(pos + 1).ok_or("JPEG truncado")?,
        ]) as usize;
        if len < 2 || pos + len > data.len() {
            return Err("Segmento JPEG inválido.".into());
        }
        if matches!(marker, 0xc0..=0xc2) {
            let s = &data[pos + 2..pos + len];
            if s.len() < 6 || s.len() < 6 + s[5] as usize * 3 {
                return Err("SOF inválido".into());
            }
            let h = u16::from_be_bytes([s[1], s[2]]) as u32;
            let w = u16::from_be_bytes([s[3], s[4]]) as u32;
            let mut mx = 1;
            let mut my = 1;
            for component in s[6..].as_chunks::<3>().0.iter().take(s[5] as usize) {
                mx = mx.max((component[1] >> 4) as u32);
                my = my.max((component[1] & 15) as u32);
            }
            if w == 0 || h == 0 || mx > 4 || my > 4 {
                return Err("Dimensões JPEG inválidas".into());
            }
            return Ok(JpegInfo {
                width: w,
                height: h,
                mcu: (mx * 8, my * 8),
                orientation,
            });
        }
        pos += len;
    }
    Err("JPEG sem cabeçalho compatível.".into())
}

// Undo retains the bytes on disk, independent of image re-encoding.
pub struct FileUndo {
    pub path: PathBuf,
    pub backup: tempfile::NamedTempFile,
    expected: u64,
}
fn fingerprint(data: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    data.hash(&mut h);
    h.finish()
}
impl FileUndo {
    pub fn restore(self) -> Result<PathBuf> {
        let current = std::fs::read(&self.path).map_err(|e| e.to_string())?;
        if fingerprint(&current) != self.expected {
            return Err("O arquivo foi alterado por outro programa após o crop. A restauração foi cancelada para preservar essas alterações.".into());
        }
        let temp = tempfile::Builder::new()
            .prefix(".lumeview-restore-")
            .tempfile_in(self.path.parent().unwrap_or(Path::new(".")))
            .map_err(|e| e.to_string())?;
        std::fs::copy(self.backup.path(), temp.path()).map_err(|e| e.to_string())?;
        std::fs::set_permissions(
            temp.path(),
            std::fs::metadata(&self.path)
                .map_err(|e| e.to_string())?
                .permissions(),
        )
        .map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(&self.path).map_err(|e| e.error.to_string())?;
        Ok(self.path)
    }
}

pub fn lossless_crop(path: &Path, crop: Crop) -> Result<FileUndo> {
    let original = std::fs::read(path).map_err(|e| e.to_string())?;
    let info = jpeg_info(&original)?;
    if info.orientation != 1 {
        return Err("Este JPEG tem orientação EXIF. O crop sem perdas desta versão exige orientação normal (1). Use o recorte comum para esta imagem.".into());
    }
    let crop = crop.aligned(info.width, info.height, info.mcu);
    let mut backup = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    backup.write_all(&original).map_err(|e| e.to_string())?;
    let temp = tempfile::Builder::new()
        .prefix(".lumeview-crop-")
        .suffix(".jpg")
        .tempfile_in(path.parent().unwrap_or(Path::new(".")))
        .map_err(|e| e.to_string())?;
    let geometry = format!("{}x{}+{}+{}", crop.w, crop.h, crop.x, crop.y);
    checked(
        command("jpegtran")
            .args(["-copy", "all", "-crop", &geometry, "-outfile"])
            .arg(temp.path())
            .arg(path),
    )?;
    let result = std::fs::read(temp.path()).map_err(|e| e.to_string())?;
    let actual = jpeg_info(&result)?;
    if actual.width != crop.w || actual.height != crop.h {
        return Err(
            "O codec produziu um recorte diferente da seleção. O original foi preservado.".into(),
        );
    }
    // Do not overwrite a file changed by another program during processing.
    if std::fs::read(path).map_err(|e| e.to_string())? != original {
        return Err("O arquivo foi alterado por outro programa. O recorte foi cancelado.".into());
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    std::fs::set_permissions(
        temp.path(),
        std::fs::metadata(path)
            .map_err(|e| e.to_string())?
            .permissions(),
    )
    .map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.error.to_string())?;
    Ok(FileUndo {
        path: path.to_owned(),
        backup,
        expected: fingerprint(&result),
    })
}

/// Rotate JPEG coefficients and save a numbered copy, preserving the source.
pub fn save_lossless_rotation(path: &Path, degrees: u16) -> Result<PathBuf> {
    if !matches!(degrees, 90 | 180 | 270) {
        return Err("Rotação inválida.".into());
    }
    let original = std::fs::read(path).map_err(|e| e.to_string())?;
    let info = jpeg_info(&original)?;
    if info.orientation != 1 {
        return Err("Este JPEG tem orientação EXIF. Use a rotação comum para esta imagem.".into());
    }
    let parent = path.parent().unwrap_or(Path::new("."));
    let temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    checked(command("jpegtran").args(["-copy", "all", "-perfect", "-rotate", &degrees.to_string(), "-outfile"]).arg(temp.path()).arg(path))
        .map_err(|e| format!("Não foi possível girar este JPEG sem perdas e sem cortar bordas. O original foi preservado. {e}"))?;
    let result = jpeg_info(&std::fs::read(temp.path()).map_err(|e| e.to_string())?)?;
    let expected = if degrees == 180 {
        (info.width, info.height)
    } else {
        (info.height, info.width)
    };
    if (result.width, result.height) != expected {
        return Err("Dimensões inesperadas. O original foi preservado.".into());
    }
    if std::fs::read(path).map_err(|e| e.to_string())? != original {
        return Err("O arquivo foi alterado por outro programa. Rotação cancelada.".into());
    }
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path
        .extension()
        .unwrap_or(std::ffi::OsStr::new("jpg"))
        .to_string_lossy();
    let mut working = temp;
    for index in 0u64.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            index.to_string()
        };
        let dest = parent.join(format!("{stem}_rotate{suffix}.{ext}"));
        match working.persist_noclobber(&dest) {
            Ok(_) => return Ok(dest),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => working = e.file,
            Err(e) => return Err(e.error.to_string()),
        }
    }
    unreachable!()
}

/// Save a lossless selection beside its source without changing or overwriting it.
pub fn save_lossless_crop(path: &Path, crop: Crop) -> Result<PathBuf> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let working = tempfile::Builder::new()
        .prefix(".lumeview-selection-")
        .suffix(".jpg")
        .tempfile_in(parent)
        .map_err(|e| e.to_string())?;
    let working = working.into_temp_path();
    std::fs::copy(path, &working).map_err(|e| e.to_string())?;
    let undo = lossless_crop(&working, crop)?;
    drop(undo);
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let extension = path
        .extension()
        .unwrap_or(std::ffi::OsStr::new("jpg"))
        .to_string_lossy();
    let mut output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    output
        .write_all(&std::fs::read(&working).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    output.as_file().sync_all().map_err(|e| e.to_string())?;
    let mut working = output;
    for index in 0u64.. {
        let suffix = if index == 0 {
            String::new()
        } else {
            index.to_string()
        };
        let dest = parent.join(format!("{stem}_crop{suffix}.{extension}"));
        match working.persist_noclobber(&dest) {
            Ok(_) => return Ok(dest),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => working = e.file,
            Err(e) => return Err(e.error.to_string()),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lossless_rotation_preserves_source_and_rejects_imperfect_edges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.jpg");
        export(
            &RgbaImage::from_pixel(64, 32, image::Rgba([80, 120, 160, 255])),
            &path,
            Format::Jpeg,
            80,
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        for (degrees, name, dims) in [
            (90, "page_rotate.jpg", (32, 64)),
            (270, "page_rotate1.jpg", (32, 64)),
            (180, "page_rotate2.jpg", (64, 32)),
        ] {
            let saved = save_lossless_rotation(&path, degrees).unwrap();
            assert_eq!(saved.file_name().unwrap(), name);
            let info = jpeg_info(&std::fs::read(saved).unwrap()).unwrap();
            assert_eq!((info.width, info.height), dims);
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
        export(
            &RgbaImage::from_pixel(31, 31, image::Rgba([80, 120, 160, 255])),
            &path,
            Format::Jpeg,
            80,
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(save_lossless_rotation(&path, 90).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
    }
    #[test]
    fn lossless_copies_are_numbered_and_preserve_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("page.jpg");
        export(
            &RgbaImage::from_pixel(64, 64, image::Rgba([80, 120, 160, 255])),
            &path,
            Format::Jpeg,
            80,
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        let crop = Crop {
            x: 0,
            y: 0,
            w: 32,
            h: 32,
        };
        for name in ["page_crop.jpg", "page_crop1.jpg", "page_crop2.jpg"] {
            let saved = save_lossless_crop(&path, crop).unwrap();
            assert_eq!(saved.file_name().unwrap(), name);
            let info = jpeg_info(&std::fs::read(saved).unwrap()).unwrap();
            assert_eq!((info.width, info.height), (32, 32));
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }
    #[test]
    fn native_decoder_preserves_exif_orientation_and_alpha() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("portrait.jpg");
        export(
            &RgbaImage::from_pixel(32, 16, image::Rgba([20, 40, 80, 255])),
            &path,
            Format::Jpeg,
            80,
        )
        .unwrap();
        let jpg = std::fs::read(&path).unwrap();
        let exif = [
            b'E', b'x', b'i', b'f', 0, 0, b'I', b'I', 42, 0, 8, 0, 0, 0, 1, 0, 0x12, 1, 3, 0, 1, 0,
            0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
        ];
        let mut oriented = vec![255, 216, 255, 225];
        oriented.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
        oriented.extend_from_slice(&exif);
        oriented.extend_from_slice(&jpg[2..]);
        std::fs::write(&path, &oriented).unwrap();
        assert_eq!(load(&path).unwrap().pixels.dimensions(), (16, 32));
        assert_eq!(jpeg_info(&oriented).unwrap().orientation, 6);
        let png = dir.path().join("alpha.not-png");
        let rgba = RgbaImage::from_pixel(4, 3, image::Rgba([15, 30, 40, 100]));
        rgba.save_with_format(&png, image::ImageFormat::Png)
            .unwrap();
        assert_eq!(load(&png).unwrap().pixels.as_ref(), &rgba);
    }

    #[test]
    #[ignore = "Manual release benchmark, needs LUMEVIEW_BENCH_DIR"]
    fn benchmark_navigation_decode() {
        let dir = PathBuf::from(std::env::var_os("LUMEVIEW_BENCH_DIR").expect("Set LUMEVIEW_BENCH_DIR"));
        for name in ["large.jpg", "large.webp"] {
            let path = dir.join(name);
            let mut old = 0.0;
            let mut native = 0.0;
            for _ in 0..3 {
                let start = std::time::Instant::now();
                let png = checked(command("magick").arg(&path).args([
                    "-auto-orient",
                    "-colorspace",
                    "sRGB",
                    "-depth",
                    "8",
                    "png:-",
                ]))
                .unwrap();
                let previous = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
                    .unwrap()
                    .to_rgba8();
                old += start.elapsed().as_secs_f64();
                let start = std::time::Instant::now();
                let current = load(&path).unwrap();
                native += start.elapsed().as_secs_f64();
                assert_eq!(current.pixels.dimensions(), previous.dimensions());
                std::hint::black_box(current);
            }
            println!(
                "BENCH {name}: previous {:.1} ms, native {:.1} ms, {:.1}x faster (3 iterations)",
                old / 3.0 * 1000.0,
                native / 3.0 * 1000.0,
                old / native
            );
        }
    }
    #[test]
    fn jpeg_sampling_and_truncation() {
        let b = [
            255, 216, 255, 192, 0, 17, 8, 0, 100, 0, 200, 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1,
        ];
        let j = jpeg_info(&b).unwrap();
        assert_eq!((j.width, j.height, j.mcu), (200, 100, (16, 16)));
        assert!(jpeg_info(&b[..15]).is_err());
        assert!(jpeg_info(b"not jpeg").is_err());
    }
    #[test]
    fn codecs_lossless_crop_undo_and_no_clobber() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fotografia com espaço.jpg");
        let pixels = RgbaImage::from_fn(128, 96, |x, y| {
            let v = ((x * 2 + y) % 255) as u8;
            image::Rgba([v, v, v, 255])
        });
        export_new(&pixels, &path, Format::Jpeg, 80).unwrap();
        let original = std::fs::read(&path).unwrap();
        assert!(export_new(&pixels, &path, Format::Jpeg, 80).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let before = load(&path).unwrap().pixels;
        let c = Crop {
            x: 16,
            y: 16,
            w: 80,
            h: 64,
        };
        let undo = lossless_crop(&path, c).unwrap();
        let after = load(&path).unwrap().pixels;
        assert_eq!(
            &*after,
            &image::imageops::crop_imm(&*before, c.x, c.y, c.w, c.h).to_image()
        );
        undo.restore().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let webp = dir.path().join("image.webp");
        export_new(&pixels, &webp, Format::Webp, 80).unwrap();
        assert_eq!(load(&webp).unwrap().pixels.dimensions(), (128, 96));
        let avif = dir.path().join("image.avif");
        checked(command("magick").arg(&path).arg(&avif)).unwrap();
        assert_eq!(load(&avif).unwrap().pixels.dimensions(), (128, 96));
        let renamed = dir.path().join("formato-renomeado.bin");
        std::fs::rename(&avif, &renamed).unwrap();
        assert_eq!(load(&renamed).unwrap().pixels.dimensions(), (128, 96));
        let mut alpha = pixels.clone();
        for p in alpha.pixels_mut() {
            p.0 = [255, 0, 0, 0];
        }
        let white = dir.path().join("white.jpg");
        export(&alpha, &white, Format::Jpeg, 80).unwrap();
        assert!(load(&white)
            .unwrap()
            .pixels
            .pixels()
            .all(|p| p[0] > 250 && p[1] > 250 && p[2] > 250));
    }
    #[test]
    fn undo_refuses_external_changes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("original.jpg");
        export(
            &RgbaImage::from_pixel(64, 64, image::Rgba([20, 40, 80, 255])),
            &p,
            Format::Jpeg,
            80,
        )
        .unwrap();
        let undo = lossless_crop(
            &p,
            Crop {
                x: 0,
                y: 0,
                w: 32,
                h: 32,
            },
        )
        .unwrap();
        std::fs::write(&p, b"external change").unwrap();
        assert!(undo.restore().is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"external change");
    }
}

/// Prepare a print snapshot; retain ownership while the system dialog is open.
pub fn print_image(pixels: &RgbaImage) -> Result<tempfile::TempPath> {
    let snapshot = tempfile::Builder::new()
        .prefix("lumeview-print-")
        .suffix(".png")
        .tempfile()
        .map_err(|e| e.to_string())?;
    pixels
        .save_with_format(snapshot.path(), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(snapshot.into_temp_path())
}

pub fn open_print_dialog(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::{
            System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED},
            UI::Shell::ShellExecuteW,
        };
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        });
        let verb: Vec<u16> = "print".encode_utf16().chain(Some(0)).collect();
        let file: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                1,
            )
        } as isize;
        if result <= 32 {
            return Err(format!(
                "Windows could not open the image print dialog (error {result})."
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("Printing is available only on Windows.".into())
    }
}
