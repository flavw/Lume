use eframe::egui;

pub const DISPLAY_SIZE: f32 = 256.0;

#[derive(Clone, Copy)]
pub enum Kind { Delete, Save, Rename, Shortcuts, Preferences }

impl Kind {
    fn index(self) -> usize { self as usize }
    fn bytes(self) -> &'static [u8] {
        match self {
            Self::Delete => include_bytes!("../assets/dialogs/lume-delete.webp"),
            Self::Save => include_bytes!("../assets/dialogs/lume-save.webp"),
            Self::Rename => include_bytes!("../assets/dialogs/lume-rename.webp"),
            Self::Shortcuts => include_bytes!("../assets/dialogs/lume-shortcuts.webp"),
            Self::Preferences => include_bytes!("../assets/dialogs/lume-preferences.webp"),
        }
    }
}

#[derive(Default)]
pub struct Cache { textures: [Option<egui::TextureHandle>; 5] }

impl Cache {
    // Decode only when its dialog is first opened. Image browsing does no work here.
    pub fn texture(&mut self, ctx: &egui::Context, kind: Kind) -> egui::TextureHandle {
        self.textures[kind.index()].get_or_insert_with(|| {
            let pixels = image::load_from_memory(kind.bytes())
                .expect("Valid embedded Lume dialog artwork").into_rgba8();
            ctx.load_texture(format!("lume-dialog-{}",kind.index()),
                egui::ColorImage::from_rgba_unmultiplied([pixels.width() as usize,pixels.height() as usize], &pixels),
                egui::TextureOptions::LINEAR)
        }).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_artwork_is_transparent_webp_and_texture_loading_is_lazy() {
        for kind in [Kind::Delete,Kind::Save,Kind::Rename,Kind::Shortcuts,Kind::Preferences] {
            assert_eq!(image::guess_format(kind.bytes()).unwrap(), image::ImageFormat::WebP);
            let image=image::load_from_memory(kind.bytes()).unwrap().into_rgba8();
            assert_eq!(image.dimensions(),(512,512));
            assert!(image.pixels().any(|pixel|pixel[3]==0));
        }
        let ctx=egui::Context::default();
        let mut cache=Cache::default();
        assert_eq!(cache.textures.iter().flatten().count(),0);
        let first=cache.texture(&ctx,Kind::Save);
        assert_eq!(first.size(),[512,512]);
        assert_eq!(cache.texture(&ctx,Kind::Save).id(),first.id());
        assert_eq!(cache.textures.iter().flatten().count(),1);
    }
}
