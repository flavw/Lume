use crate::{backend,model};
use std::path::{Path,PathBuf};

#[derive(Clone,Copy)]
pub enum Resize { Pixels(u32,u32,bool), Percent(f64) }
#[derive(Clone)]
pub struct Naming {pub base: String,pub digits: usize,pub start: u64}
impl Naming {
    pub fn filename(&self,index: usize,extension: &str) -> backend::Result<String> {
        if !(1..=12).contains(&self.digits) || self.base.trim().is_empty() {return Err("Nome ou numeração inválidos.".into());}
        let number=self.start.checked_add(index as u64).ok_or("Nome ou numeração inválidos.")?;
        let name=format!("{}{:0width$}.{}",self.base,number,extension,width=self.digits);
        model::validate_filename(&name)?;
        Ok(name)
    }
}
pub fn resized(pixels: &image::RgbaImage,resize: Resize) -> backend::Result<image::RgbaImage> {
    match resize {
        Resize::Pixels(w,h,p)=>backend::batch_resize(pixels,w,h,p),
        Resize::Percent(percent)=>{
            if !percent.is_finite() || percent<=0.0 || percent>1000.0 {return Err("Porcentagem inválida.".into());}
            let w=(pixels.width() as f64*percent/100.0).round().max(1.0) as u32;
            let h=(pixels.height() as f64*percent/100.0).round().max(1.0) as u32;
            backend::batch_resize(pixels,w,h,false)
        }
    }
}
pub fn rename_file(source: &Path,name: &str) -> backend::Result<PathBuf> {
    model::validate_filename(name)?;
    let target=source.parent().unwrap_or(Path::new(".")).join(name);
    if source==target {return Ok(target);}
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from:Vec<u16>=source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to:Vec<u16>=target.as_os_str().encode_wide().chain(Some(0)).collect();
        // Unlike std::fs::rename, MoveFileW refuses to overwrite an existing file.
        if unsafe {windows_sys::Win32::Storage::FileSystem::MoveFileW(from.as_ptr(),to.as_ptr())}==0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        return Ok(target);
    }
    #[cfg(not(windows))]
    {
    // Publish a byte-identical copy without replacing any existing destination.
    let temporary=tempfile::NamedTempFile::new_in(source.parent().unwrap_or(Path::new("."))).map_err(|e|e.to_string())?;
    std::fs::copy(source,temporary.path()).map_err(|e|e.to_string())?;
    temporary.as_file().sync_all().map_err(|e|e.to_string())?;
    temporary.persist_noclobber(&target).map_err(|e|e.error.to_string())?;
    std::fs::remove_file(source).map_err(|e|e.to_string())?;
    Ok(target)
    }
}
pub fn export_resized_original(pixels: &image::RgbaImage,source: &Path,target: &Path,quality: u8) -> backend::Result<()> {
    let temporary=tempfile::NamedTempFile::new_in(target.parent().unwrap_or(Path::new("."))).map_err(|e|e.to_string())?.into_temp_path();
    std::fs::copy(source,&temporary).map_err(|e|e.to_string())?;
    // Inspect the original signature and reject animation/multipage flattening.
    crate::image_details::save_original(pixels,&temporary,quality)?;
    temporary.persist_noclobber(target).map_err(|e|e.error.to_string())?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;
    #[test]
    fn resize_keeps_format_and_preserves_existing_files() {
        let dir=tempfile::tempdir().unwrap();
        let source=dir.path().join("source.png");
        image::RgbaImage::new(40,20).save(&source).unwrap();
        let original=std::fs::read(&source).unwrap();
        let output=dir.path().join("resized.png");
        let pixels=image::RgbaImage::new(20,10);
        export_resized_original(&pixels,&source,&output,80).unwrap();
        assert_eq!(image::open(&output).unwrap().dimensions(),(20,10));
        assert_eq!(image::guess_format(&std::fs::read(&output).unwrap()).unwrap(),image::ImageFormat::Png);
        assert_eq!(std::fs::read(&source).unwrap(),original);
        let existing=std::fs::read(&output).unwrap();
        assert!(export_resized_original(&pixels,&source,&output,80).is_err());
        assert_eq!(std::fs::read(&output).unwrap(),existing);
    }
    #[test]
    fn numbering_percentage_and_collision() {
        let naming=Naming{base:"wallpaper_".into(),digits:3,start:8};
        assert_eq!(naming.filename(0,"jpg").unwrap(),"wallpaper_008.jpg");
        assert_eq!(naming.filename(2,"png").unwrap(),"wallpaper_010.png");
        let image=image::RgbaImage::new(400,200);
        assert_eq!(resized(&image,Resize::Percent(50.0)).unwrap().dimensions(),(200,100));
        assert!(resized(&image,Resize::Percent(0.0)).is_err());
        let dir=tempfile::tempdir().unwrap();let source=dir.path().join("old.jpg");
        std::fs::write(&source,b"original").unwrap();
        std::fs::write(dir.path().join("existing.jpg"),b"other").unwrap();
        assert!(rename_file(&source,"existing.jpg").is_err());
        assert_eq!(std::fs::read(&source).unwrap(),b"original");
        let result=rename_file(&source,"new.jpg").unwrap();
        assert_eq!(std::fs::read(result).unwrap(),b"original");assert!(!source.exists());
    }
}
