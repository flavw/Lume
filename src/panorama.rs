use crate::backend;
use image::{Rgba, RgbaImage};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub enum Input {
    File(PathBuf),
    Opened { pixels: Arc<RgbaImage>, name: String },
}
impl Input {
    pub fn name(&self) -> String {
        match self {
            Self::File(path) => path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            Self::Opened { name, .. } => format!("{} · {}",name,crate::i18n::t("Imagem aberta")),
        }
    }
    pub fn description(&self) -> String {
        match self { Self::File(path) => path.display().to_string(), _ => self.name() }
    }
}

// Keep inputs and output together within a predictable memory budget.
const MAX_BYTES: u64 = 512 * 1024 * 1024;

pub fn create(files: &[Input], vertical: bool, gap: u32, color: [u8; 3]) -> backend::Result<RgbaImage> {
    if files.len() < 2 { return Err("Selecione pelo menos duas imagens.".into()); }
    let mut images = Vec::new();
    let mut input_bytes = 0u64;
    for file in files {
        let pixels = match file {
            Input::File(path) => backend::load(path)?.pixels,
            Input::Opened { pixels, .. } => pixels.clone(),
        };
        input_bytes = input_bytes.checked_add(pixels.as_raw().len() as u64)
            .ok_or("Panorama grande demais.")?;
        if input_bytes > MAX_BYTES { return Err("Panorama grande demais.".into()); }
        images.push(pixels);
    }
    let refs: Vec<_> = images.iter().map(|im| im.as_ref()).collect();
    compose(&refs, vertical, gap, color, input_bytes)
}

fn compose(images: &[&RgbaImage], vertical: bool, gap: u32, color: [u8; 3], input_bytes: u64) -> backend::Result<RgbaImage> {
    if images.len() < 2 { return Err("Selecione pelo menos duas imagens.".into()); }
    let mut along = u64::from(gap).checked_mul((images.len() - 1) as u64).ok_or("Panorama grande demais.")?;
    let mut across = 0u32;
    for image in images {
        let (w,h) = image.dimensions();
        if w == 0 || h == 0 { return Err("Dimensões inválidas".into()); }
        along = along.checked_add(u64::from(if vertical { h } else { w })).ok_or("Panorama grande demais.")?;
        across = across.max(if vertical { w } else { h });
    }
    let along = u32::try_from(along).map_err(|_| "Panorama grande demais.")?;
    let (w,h) = if vertical { (across,along) } else { (along,across) };
    let bytes = u64::from(w).checked_mul(u64::from(h)).and_then(|n| n.checked_mul(4)).ok_or("Panorama grande demais.")?;
    if bytes.checked_add(input_bytes).is_none_or(|n| n > MAX_BYTES) { return Err("Panorama grande demais.".into()); }
    let mut output = RgbaImage::from_pixel(w,h,Rgba([color[0],color[1],color[2],255]));
    let mut offset = 0u32;
    for (index,image) in images.iter().enumerate() {
        let (x,y) = if vertical { ((w-image.width())/2,offset) } else { (offset,(h-image.height())/2) };
        image::imageops::replace(&mut output,*image,i64::from(x),i64::from(y));
        if index + 1 < images.len() { offset += if vertical { image.height() } else { image.width() }; offset += gap; }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_gap_and_alpha_preserve_pixels() {
        let a = RgbaImage::from_pixel(2,2,Rgba([255,0,0,80]));
        let b = RgbaImage::from_pixel(1,4,Rgba([0,255,0,255]));
        let out = compose(&[&a,&b],false,1,[10,20,30],0).unwrap();
        assert_eq!(out.dimensions(),(4,4));
        assert_eq!(*out.get_pixel(0,1),Rgba([255,0,0,80]));
        assert_eq!(*out.get_pixel(2,2),Rgba([10,20,30,255]));
        assert_eq!(*out.get_pixel(3,3),Rgba([0,255,0,255]));
        let out = compose(&[&a,&b],true,0,[0,0,0],0).unwrap();
        assert_eq!(out.dimensions(),(2,6));
        assert_eq!(*out.get_pixel(0,2),Rgba([0,255,0,255]));
    }
    #[test]
    fn opened_image_uses_current_pixels_without_a_disk_file() {
        let pixels = Arc::new(RgbaImage::from_pixel(2,3,Rgba([12,34,56,255])));
        let source = Input::Opened { pixels, name: "clipboard".into() };
        let output = create(&[source.clone(),source],false,0,[0,0,0]).unwrap();
        assert_eq!(output.dimensions(),(4,3));
        assert_eq!(*output.get_pixel(3,2),Rgba([12,34,56,255]));
    }
    #[test]
    fn rejects_huge_gap_and_missing_images() {
        let a = RgbaImage::new(2,2);
        assert!(compose(&[&a,&a],false,u32::MAX,[0,0,0],0).is_err());
        assert!(compose(&[&a],true,0,[0,0,0],0).is_err());
    }
}
