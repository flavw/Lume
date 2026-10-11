#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod backend;
mod comic;
mod generation;
mod i18n;
mod icons;
mod image_details;
mod loader;
mod model;
mod preferences;
mod tiles;
use i18n::{t, Language};

use backend::{Format, Loaded};
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use image::RgbaImage;
use model::{Crop, Fit};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const RATIOS: [(&str, Option<f32>); 7] = [
    ("Livre", None),
    ("1:1", Some(1.0)),
    ("16:9", Some(16.0 / 9.0)),
    ("9:16", Some(9.0 / 16.0)),
    ("10:16", Some(10.0 / 16.0)),
    ("8:16", Some(8.0 / 16.0)),
    ("Original", Some(-1.0)),
];
const FITS: [(&str, Fit); 7] = [
    ("1  Tamanho original (1:1)", Fit::Original),
    ("2  Ajustar apenas imagens grandes", Fit::Large),
    ("3  Ajustar todas à janela", Fit::All),
    ("4  Esticar à janela", Fit::Stretch),
    ("5  Ajustar largura", Fit::Width),
    ("6  Ajustar altura", Fit::Height),
    ("7  Ajustar lado menor", Fit::SmallSide),
];

#[derive(Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
enum Theme {
    #[default]
    Dark,
    Light,
    #[serde(alias = "Blue")]
    Neon,
    System,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
struct Settings {
    fit: Fit,
    theme: Theme,
    language: Language,
    title_color: [u8; 3],
    screen_color: [u8; 3],
    checkerboard: bool,
    title_size: f32,
    title_position: u8,
    title_alignment: u8,
    jpeg_quality: u8,
    webp_quality: u8,
    ratio: usize,
    slide_seconds: f32,
    show_toolbar: bool,
    show_status_bar: bool,
    auto_restart: bool,
    menu_transparency: u8,
    manual_ratio: [f32; 2],
    neon_color: [u8; 3],
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            fit: Fit::Large,
            theme: Theme::Dark,
            language: Language::English,
            title_color: [0, 255, 0],
            screen_color: [0, 0, 0],
            checkerboard: true,
            title_size: 14.0,
            title_position: 0,
            title_alignment: 0,
            jpeg_quality: 80,
            webp_quality: 80,
            ratio: 3,
            slide_seconds: 4.0,
            show_toolbar: false,
            show_status_bar: false,
            auto_restart: false,
            menu_transparency: 0,
            manual_ratio: [2.0, 1.0],
            neon_color: [0, 255, 200],
        }
    }
}

enum PendingAction {
    Open(PathBuf),
    Load(PathBuf),
    Navigate(isize),
    Paste,
    Delete(bool),
    Close,
}

enum Dialog {
    ManualRatio {
        width: String,
        height: String,
        lossless: bool,
    },
    Unsaved {
        action: Option<PendingAction>,
        cancel_selected: bool,
    },
    FolderEnd {
        selected: usize,
    },
    Rename(String, String, bool),
    Resize(u32, u32, bool),
    Delete {
        permanent: bool,
        cancel_selected: bool,
    },
    Help,
    About,
    Information,
    SaveOriginal(PathBuf),
    SaveAs {
        format: Format,
        quality: u8,
    },
    Dependencies,
    Settings,
    Search(String),
    Batch {
        files: Vec<PathBuf>,
        dest: PathBuf,
        format: Format,
        quality: u8,
    },
    Overwrite(PathBuf, Format),
}
enum Message {
    Book(backend::Result<Arc<comic::Book>>),
    Export(backend::Result<()>),
    Saved(PathBuf, backend::Result<()>),
    Information(backend::Result<Vec<(String, String)>>),
    Lossless(backend::Result<PathBuf>),
    Restore(backend::Result<(PathBuf, Loaded)>),
    BatchProgress(usize, usize),
    BatchDone(Vec<String>),
}

struct App {
    pixel_ratio: f32,
    tiles: tiles::Renderer,
    settings: Settings,
    book: Option<Arc<comic::Book>>,
    logo: egui::TextureHandle,
    checker_texture: egui::TextureHandle,
    files: Vec<PathBuf>,
    index: usize,
    path: Option<PathBuf>,
    image: Option<Arc<RgbaImage>>,
    has_alpha: bool,
    file_bytes: Option<u64>,
    created: Option<String>,
    image_info: Vec<(String, String)>,
    texture: Option<egui::TextureHandle>,
    jpeg: Option<backend::JpegInfo>,
    dirty: bool,
    history: Vec<(Arc<RgbaImage>, bool)>,
    redo: Vec<(Arc<RgbaImage>, bool)>,
    file_undo: Option<backend::FileUndo>,
    selection: Option<Crop>,
    lossless: bool,
    ratio: Option<f32>,
    drag_start: Option<(Pos2, Crop, u8)>,
    fit: Fit,
    wheel_fit: Fit,
    zoom: f32,
    align_width_top: bool,
    pan: Vec2,
    canvas: Rect,
    shown: Rect,
    fullscreen: bool,
    hide_cursor: bool,
    show_art_title: bool,
    pause_down: bool,
    dialog: Option<Dialog>,
    settings_page: usize,
    error: Option<String>,
    busy: bool,
    loader: loader::Loader,
    loading: Option<(u64, Instant)>,
    texture_cache: VecDeque<(u64, egui::TextureHandle)>,
    menu_bottom: f32,
    tx: mpsc::Sender<Message>,
    rx: mpsc::Receiver<Message>,
    slide: bool,
    paused: bool,
    last_slide: Instant,
    last_navigation: Instant,
    notice: Option<(String, Instant)>,
    zoom_notice: Option<(String, Instant)>,
    batch_progress: Option<(usize, usize)>,
    batch_cancel: Option<Arc<AtomicBool>>,
    clipboard: Option<arboard::Clipboard>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings: Settings = cc
            .storage
            .and_then(|s| s.get_string("settings"))
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        i18n::set_language(settings.language);
        cc.egui_ctx.set_theme(match settings.theme {
            Theme::Dark | Theme::Neon => egui::ThemePreference::Dark,
            Theme::Light => egui::ThemePreference::Light,
            Theme::System => egui::ThemePreference::System,
        });
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            pixel_ratio: cc.egui_ctx.pixels_per_point(),
            tiles: tiles::Renderer::default(),
            fit: settings.fit,
            wheel_fit: settings.fit,
            settings,
            book: None,
            checker_texture: cc.egui_ctx.load_texture(
                "transparency-checker",
                egui::ColorImage {
                    size: [2, 2],
                    pixels: vec![
                        Color32::from_gray(170),
                        Color32::from_gray(220),
                        Color32::from_gray(220),
                        Color32::from_gray(170),
                    ],
                },
                egui::TextureOptions::NEAREST_REPEAT,
            ),
            logo: cc.egui_ctx.load_texture(
                "lume-cat",
                egui::ColorImage::from_rgba_unmultiplied([128, 128], &cat_icon().rgba),
                egui::TextureOptions::LINEAR,
            ),
            files: vec![],
            index: 0,
            path: None,
            image: None,
            has_alpha: false,
            file_bytes: None,
            created: None,
            image_info: vec![],
            texture: None,
            jpeg: None,
            dirty: false,
            history: vec![],
            redo: vec![],
            file_undo: None,
            selection: None,
            lossless: false,
            ratio: None,
            drag_start: None,
            zoom: 1.0,
            align_width_top: false,
            pan: Vec2::ZERO,
            canvas: Rect::NOTHING,
            shown: Rect::NOTHING,
            fullscreen: cc
                .egui_ctx
                .input(|i| i.viewport().fullscreen.unwrap_or(false)),
            hide_cursor: false,
            show_art_title: true,
            pause_down: false,
            dialog: None,
            settings_page: 0,
            error: None,
            busy: false,
            loader: loader::Loader::new(cc.egui_ctx.clone()),
            loading: None,
            texture_cache: VecDeque::new(),
            menu_bottom: 25.0,
            tx,
            rx,
            slide: false,
            paused: false,
            last_slide: Instant::now(),
            last_navigation: Instant::now(),
            notice: None,
            zoom_notice: None,
            batch_progress: None,
            batch_cancel: None,
            clipboard: None,
        };
        if let Some(p) = std::env::args_os().nth(1) {
            app.open(PathBuf::from(p));
        }
        app
    }
    fn source_path(&self) -> Option<PathBuf> {
        self.book
            .as_ref()
            .map(|b| b.source.clone())
            .or_else(|| self.path.clone())
    }
    fn display_name(&self) -> String {
        if let Some(b) = &self.book {
            return format!(
                "{} — {}",
                b.source.file_name().unwrap_or_default().to_string_lossy(),
                b.names.get(self.index).cloned().unwrap_or_default()
            );
        }
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| t("Novo documento"))
    }
    fn toast(&mut self, s: impl Into<String>) {
        self.notice = Some((s.into(), Instant::now()));
    }
    fn fail(&mut self, s: impl Into<String>) {
        self.error = Some(s.into());
    }
    fn confirm_abandon(&mut self, action: PendingAction) -> bool {
        if !self.dirty {
            return false;
        }
        self.slide = false;
        self.dialog = Some(Dialog::Unsaved {
            action: Some(action),
            cancel_selected: true,
        });
        true
    }
    fn close_program(&mut self, ctx: &egui::Context) {
        if !self.confirm_abandon(PendingAction::Close) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn menu_visuals(&self, ui: &mut egui::Ui) {
        let alpha = ((100 - self.settings.menu_transparency.min(100)) as f32 * 2.55).round() as u8;
        let visuals = ui.visuals_mut();
        let c = visuals.panel_fill;
        visuals.panel_fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
        let c = visuals.window_fill;
        visuals.window_fill = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
    }
    fn open(&mut self, p: PathBuf) {
        if self.confirm_abandon(PendingAction::Open(p.clone())) {
            return;
        }
        if !p.is_file() {
            self.fail("Arquivo não encontrado.");
            return;
        }
        let p = std::path::absolute(&p).unwrap_or(p);
        self.loader.invalidate();
        self.texture_cache.clear();
        if comic::supported(&p) {
            self.loading = None;
            self.busy = true;
            let tx = self.tx.clone();
            let ctx = self.loader.context();
            std::thread::spawn(move || {
                let _ = tx.send(Message::Book(comic::Book::open(p)));
                ctx.request_repaint();
            });
            return;
        }
        self.book = None;
        self.files = model::folder_files(&p);
        self.index = self.files.iter().position(|x| x == &p).unwrap_or(0);
        self.request_load(p);
    }
    fn request_load(&mut self, p: PathBuf) {
        if self.confirm_abandon(PendingAction::Load(p.clone())) {
            if let Some(path) = &self.path {
                self.index = self
                    .files
                    .iter()
                    .position(|f| f == path)
                    .unwrap_or(self.index);
            }
            return;
        }
        if self.busy {
            return;
        }
        self.fit = self.settings.fit;
        self.wheel_fit = self.settings.fit;
        self.zoom = 1.0;
        self.pan = Vec2::ZERO;
        self.zoom_notice = None;
        self.last_navigation = Instant::now();
        self.tiles.clear();
        self.selection = None;
        self.lossless = false;
        let (id, cached) = self.loader.request(p.clone());
        if let Some(im) = cached {
            let ctx = self.loader.context();
            self.install_prepared(&ctx, p, im);
        } else {
            self.loading = Some((id, Instant::now()));
        }
    }
    fn install(&mut self, ctx: &egui::Context, p: PathBuf, loaded: Loaded) {
        self.loader.invalidate();
        self.texture_cache.clear();
        let im = self.loader.prepare(loaded);
        self.install_prepared(ctx, p, im);
    }
    fn install_prepared(&mut self, ctx: &egui::Context, p: PathBuf, im: Arc<loader::Prepared>) {
        self.loading = None;
        self.tiles.clear();
        self.path = if self.book.is_none() { Some(p) } else { None };
        self.jpeg = if self.book.is_none() {
            im.loaded.jpeg
        } else {
            None
        };
        self.has_alpha = im.has_alpha;
        self.file_bytes = im.file_bytes;
        self.created = im.created.map(|time| {
            chrono::DateTime::<chrono::Local>::from(time)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        });
        self.image = Some(im.loaded.pixels.clone());
        self.dirty = false;
        self.history.clear();
        self.redo.clear();
        self.selection = None;
        self.pan = Vec2::ZERO;
        self.align_width_top = self.fit == Fit::Width;

        if let Some((_, texture)) = self
            .texture_cache
            .iter()
            .find(|(serial, _)| *serial == im.serial)
        {
            self.texture = Some(texture.clone());
        } else {
            let texture = ctx.load_texture(
                "image",
                egui::ImageData::Color(im.display.clone()),
                egui::TextureOptions::LINEAR,
            );
            self.texture_cache.push_back((im.serial, texture.clone()));
            self.texture = Some(texture);
            while self.texture_cache.len() > 3 {
                self.texture_cache.pop_front();
            }
        }
        let len = self.files.len();
        if len > 1 {
            let mut paths = vec![
                self.files[(self.index + 1) % len].clone(),
                self.files[(self.index + len - 1) % len].clone(),
            ];
            if len > 2 {
                paths.push(self.files[(self.index + 2) % len].clone());
            }
            self.loader.prefetch(paths);
        }
    }
    fn update_texture(&mut self, ctx: &egui::Context) {
        self.tiles.clear();
        if let Some(im) = &self.image {
            self.has_alpha = im.pixels().any(|p| p[3] < 255);
            let limit = ctx.input(|i| i.max_texture_side) as u32;
            let small = if im.width() > limit || im.height() > limit {
                Some(image::imageops::thumbnail(&**im, limit, limit))
            } else {
                None
            };
            let display = small.as_ref().unwrap_or(im);
            let ci = egui::ColorImage::from_rgba_unmultiplied(
                [display.width() as usize, display.height() as usize],
                display.as_raw(),
            );
            // Never mutate a texture retained for the original cached image.
            self.texture = Some(ctx.load_texture("edited", ci, egui::TextureOptions::LINEAR));
        }
    }
    fn messages(&mut self, ctx: &egui::Context) {
        while let Ok(ready) = self.loader.ready.try_recv() {
            if self.loading.as_ref().map(|(id, _)| *id) != Some(ready.id) {
                continue;
            }
            self.loading = None;
            match ready.result {
                Ok(im) => self.install_prepared(ctx, ready.path, im),
                Err(e) => self.fail(e),
            }
        }
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Message::Saved(path, result) => {
                    self.busy = false;
                    match result {
                        Ok(()) => {
                            self.toast("Imagem salva.");
                            self.dirty = false;
                            self.open(path);
                        }
                        Err(e) => self.fail(e),
                    }
                }
                Message::Information(result) => {
                    self.busy = false;
                    match result {
                        Ok(rows) => {
                            self.image_info = rows;
                            self.dialog = Some(Dialog::Information);
                        }
                        Err(e) => self.fail(e),
                    }
                }
                Message::Book(r) => {
                    self.busy = false;
                    match r {
                        Ok(b) => {
                            self.files = b.pages.clone();
                            self.index = 0;
                            self.book = Some(b);
                            self.request_load(self.files[0].clone());
                        }
                        Err(e) => self.fail(e),
                    }
                }
                Message::Export(r) => {
                    self.busy = false;
                    match r {
                        Ok(()) => self.toast("Imagem exportada."),
                        Err(e) => self.fail(e),
                    }
                }
                Message::Lossless(r) => {
                    self.busy = false;
                    match r {
                        Ok(saved) => {
                            self.selection = None;
                            self.lossless = false;
                            if let Some(source) = self.path.clone() {
                                self.files = model::folder_files(&source);
                                self.index =
                                    self.files.iter().position(|p| p == &source).unwrap_or(0);
                            }
                            self.toast(format!(
                                "Imagem salva: {}",
                                saved.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                        Err(e) => self.fail(e),
                    }
                }
                Message::Restore(r) => {
                    self.busy = false;
                    match r {
                        Ok((p, im)) => {
                            self.install(ctx, p, im);
                            self.toast("Arquivo original restaurado.");
                        }
                        Err(e) => self.fail(e),
                    }
                }
                Message::BatchProgress(n, total) => self.batch_progress = Some((n, total)),
                Message::BatchDone(errors) => {
                    self.busy = false;
                    self.batch_progress = None;
                    let cancelled = self
                        .batch_cancel
                        .take()
                        .map(|c| c.load(Ordering::Relaxed))
                        .unwrap_or(false);
                    if errors.is_empty() {
                        self.toast(if cancelled {
                            "Lote cancelado. Arquivos concluídos foram mantidos."
                        } else {
                            "Lote concluído."
                        });
                    } else {
                        self.fail(format!("Falhas no lote:\n{}", errors.join("\n")));
                    }
                }
            }
        }
    }
    fn navigate(&mut self, delta: isize) {
        if self.dialog.is_none() && self.confirm_abandon(PendingAction::Navigate(delta)) {
            return;
        }
        if self.dialog.is_some() {
            return;
        }
        if self.files.is_empty() {
            return;
        }
        if self.book.is_none()
            && !self.settings.auto_restart
            && model::at_folder_end(self.index, delta, self.files.len())
        {
            self.dialog = Some(Dialog::FolderEnd { selected: 0 });
            return;
        }
        self.index = model::next_index(self.index, delta, self.files.len());
        self.request_load(self.files[self.index].clone());
        self.last_slide = Instant::now();
    }
    fn choose_open(&mut self) {
        if let Some(p) = rfd::FileDialog::new()
            .add_filter(
                "Imagens",
                &[
                    "jpg", "jpeg", "png", "webp", "avif", "gif", "bmp", "tif", "tiff", "heic",
                    "ico", "cbz",
                ],
            )
            .pick_file()
        {
            self.open(p);
        }
    }
    fn dimensions(&self) -> (u32, u32) {
        self.image
            .as_ref()
            .map(|i| i.dimensions())
            .unwrap_or((1, 1))
    }
    fn rotate_lossless(&mut self, degrees: u16) {
        if self.busy || self.loading.is_some() || self.dirty || self.jpeg.is_none() {
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        self.busy = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Message::Lossless(backend::save_lossless_rotation(
                &path, degrees,
            )));
        });
    }
    fn crop_choices(&mut self, ui: &mut egui::Ui, lossless: bool) {
        for (n, (label, ratio)) in RATIOS.iter().enumerate() {
            if item(ui, label, "") {
                self.settings.ratio = n;
                self.begin_crop(*ratio, lossless);
            }
        }
        ui.separator();
        if item(ui, "Proporção manual", "") {
            self.dialog = Some(Dialog::ManualRatio {
                width: self.settings.manual_ratio[0].to_string(),
                height: self.settings.manual_ratio[1].to_string(),
                lossless,
            });
        }
    }
    fn invert_crop_ratio(&mut self) {
        if let Some(ratio) = self.ratio {
            self.begin_crop(Some(1.0 / ratio), self.lossless);
        }
    }
    fn begin_crop(&mut self, ratio: Option<f32>, lossless: bool) {
        if self.image.is_none() || self.busy || self.fullscreen {
            return;
        }
        if lossless {
            if self.dirty {
                self.fail("O recorte JPEG sem perdas exige a imagem original, sem edições em memória. Recarregue com F5.");
                return;
            }
            match self.jpeg {
                None => {
                    self.fail("O recorte sem perdas está disponível apenas para arquivos JPEG.");
                    return;
                }
                Some(j) if j.orientation != 1 => {
                    self.fail("Esta versão exige JPEG com orientação EXIF normal (1) para crop sem perdas. Use o recorte comum.");
                    return;
                }
                _ => {}
            }
        }
        let (w, h) = self.dimensions();
        self.ratio = ratio.map(|r| if r < 0.0 { w as f32 / h as f32 } else { r });
        self.lossless = lossless;
        self.selection = Some(Crop::centered(w, h, self.ratio));
        self.align_selection();
        self.toast(if lossless {
            "Recorte JPEG sem perdas: Ctrl+Y salva uma cópia na mesma pasta; Esc cancela."
        } else {
            "Recorte: Ctrl+Y aplica em memória; Esc cancela."
        });
    }
    fn align_selection(&mut self) {
        let (w, h) = self.dimensions();
        if let Some(c) = self.selection {
            self.selection = Some(if self.lossless {
                self.jpeg
                    .map(|j| c.aligned(w, h, j.mcu))
                    .unwrap_or(c.bounded(w, h))
            } else {
                c.bounded(w, h)
            });
        }
    }
    fn edit(&mut self, ctx: &egui::Context, f: impl FnOnce(&RgbaImage) -> RgbaImage) {
        if self.busy {
            return;
        }
        if let Some(im) = self.image.take() {
            let new = f(&im);
            self.history.push((im, self.dirty));
            while self.history.len() > 1
                && (self.history.len() > 8
                    || self
                        .history
                        .iter()
                        .map(|(im, _)| im.as_raw().len())
                        .sum::<usize>()
                        > 256 * 1024 * 1024)
            {
                self.history.remove(0);
            }
            self.redo.clear();
            self.image = Some(Arc::new(new));
            self.dirty = true;
            self.selection = None;
            self.pan = Vec2::ZERO;
            self.update_texture(ctx);
        }
    }
    fn apply_crop(&mut self, ctx: &egui::Context) {
        if self.fullscreen {
            return;
        }
        let Some(c) = self.selection else {
            return;
        };
        if self.busy {
            return;
        }
        if self.lossless {
            let Some(p) = self.path.clone() else {
                return;
            };
            self.busy = true;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let r = backend::save_lossless_crop(&p, c);
                let _ = tx.send(Message::Lossless(r));
            });
        } else {
            self.edit(ctx, |im| {
                image::imageops::crop_imm(im, c.x, c.y, c.w, c.h).to_image()
            });
        }
    }
    fn undo(&mut self, ctx: &egui::Context) {
        if self.busy {
            return;
        }
        if let Some((im, dirty)) = self.history.pop() {
            if let Some(current) = self.image.replace(im) {
                self.redo.push((current, self.dirty));
            }
            self.dirty = dirty;
            self.selection = None;
            self.update_texture(ctx);
        } else if self
            .file_undo
            .as_ref()
            .map(|u| Some(&u.path) == self.path.as_ref())
            .unwrap_or(false)
        {
            let undo = self.file_undo.take().unwrap();
            self.busy = true;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let r = undo
                    .restore()
                    .and_then(|p| backend::load(&p).map(|im| (p, im)));
                let _ = tx.send(Message::Restore(r));
            });
        }
    }
    fn redo(&mut self, ctx: &egui::Context) {
        if let Some((im, dirty)) = self.redo.pop() {
            if let Some(current) = self.image.replace(im) {
                self.history.push((current, self.dirty));
            }
            self.dirty = dirty;
            self.selection = None;
            self.update_texture(ctx);
        }
    }
    fn show_information(&mut self) {
        let Some(pixels) = self.image.clone() else {
            return;
        };
        let path = self.path.clone().or_else(|| {
            self.book
                .as_ref()
                .and_then(|b| b.pages.get(self.index).cloned())
        });
        let tx = self.tx.clone();
        let ctx = self.loader.context();
        self.busy = true;
        std::thread::spawn(move || {
            let _ = tx.send(Message::Information(image_details::information(
                path.as_deref(),
                &pixels,
            )));
            ctx.request_repaint();
        });
    }
    fn save_original(&mut self) {
        if self.image.is_none() || self.busy {
            return;
        }
        let format = match self
            .path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("jpg" | "jpeg") => Format::Jpeg,
            Some("webp") => Format::Webp,
            _ => Format::Png,
        };
        let quality = if format == Format::Webp {
            self.settings.webp_quality
        } else {
            self.settings.jpeg_quality
        };
        self.dialog = Some(Dialog::SaveAs { format, quality });
    }

    fn export_dialog(&mut self, format: Format) {
        if self.image.is_none() || self.busy {
            return;
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .unwrap_or_default()
            .to_string_lossy();
        let mut d = rfd::FileDialog::new()
            .set_file_name(format!("{stem}-export.{}", format.ext()))
            .add_filter(format.coder(), &[format.ext()]);
        if let Some(p) = &self.path {
            if let Some(parent) = p.parent() {
                d = d.set_directory(parent);
            }
        }
        if let Some(p) = d.save_file() {
            let p = p.with_extension(format.ext());
            if p.exists() {
                self.dialog = Some(Dialog::Overwrite(p, format));
            } else {
                self.export_to(p, format);
            }
        }
    }
    fn export_to(&mut self, p: PathBuf, format: Format) {
        let Some(im) = self.image.clone() else {
            return;
        };
        self.busy = true;
        let tx = self.tx.clone();
        let q = if format == Format::Jpeg {
            self.settings.jpeg_quality
        } else {
            self.settings.webp_quality
        };
        std::thread::spawn(move || {
            let _ = tx.send(Message::Export(backend::export(&im, &p, format, q)));
        });
    }
    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        if self.fullscreen {
            self.selection = None;
            self.drag_start = None;
            self.lossless = false;
        }
        if self.fit == Fit::Width {
            self.align_width_top = true;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
    }
    fn zoom_by(&mut self, f: f32) {
        let (w, h) = self.dimensions();
        let s = model::display_scale(
            self.fit,
            (w, h),
            (self.canvas.width(), self.canvas.height()),
            self.zoom,
            self.pixel_ratio,
        )
        .0;
        self.zoom = (s * self.pixel_ratio * f).clamp(0.01, 64.0);
        if self.fit != Fit::Custom {
            self.wheel_fit = self.fit;
        }
        self.fit = Fit::Custom;
        self.show_zoom_notice();
    }
    fn set_fit(&mut self, fit: Fit) {
        self.fit = fit;
        if fit != Fit::Custom {
            self.wheel_fit = fit;
        }
        self.pan = Vec2::ZERO;
        self.align_width_top = fit == Fit::Width;
        self.show_zoom_notice();
    }
    fn show_zoom_notice(&mut self) {
        let label = FITS
            .iter()
            .find(|(_, fit)| *fit == self.fit)
            .map(|(label, _)| label.to_string())
            .unwrap_or_else(|| format!("Zoom manual · {:.0}%", self.zoom * 100.0));
        self.zoom_notice = Some((label, Instant::now()));
    }
    fn rename(&mut self, name: &str) {
        if let Err(e) = model::validate_filename(name) {
            self.fail(e);
            return;
        }
        let Some(p) = self.path.clone() else {
            return;
        };
        let dest = p.with_file_name(name);
        if dest == p {
            return;
        }
        if dest.exists() {
            self.fail("Já existe um arquivo com esse nome. Nenhum arquivo foi substituído.");
            return;
        }
        match std::fs::rename(&p, &dest) {
            Ok(()) => {
                if let Some(u) = &mut self.file_undo {
                    if u.path == p {
                        u.path = dest.clone();
                    }
                }
                self.files = model::folder_files(&dest);
                self.index = self.files.iter().position(|f| f == &dest).unwrap_or(0);
                self.path = Some(dest);
                self.toast("Arquivo renomeado. O conteúdo e o formato não foram convertidos.");
            }
            Err(e) => self.fail(e.to_string()),
        }
    }
    fn clipboard_copy(&mut self) {
        if let Some(im) = &self.image {
            let selected = self
                .selection
                .map(|c| image::imageops::crop_imm(&**im, c.x, c.y, c.w, c.h).to_image());
            let im = selected.as_ref().unwrap_or(im);
            let r = (|| {
                if self.clipboard.is_none() {
                    self.clipboard = Some(arboard::Clipboard::new()?);
                }
                self.clipboard
                    .as_mut()
                    .unwrap()
                    .set_image(arboard::ImageData {
                        width: im.width() as usize,
                        height: im.height() as usize,
                        bytes: std::borrow::Cow::Borrowed(im.as_raw()),
                    })
            })();
            match r {
                Ok(()) => self.toast("Imagem copiada."),
                Err(e) => self.fail(e.to_string()),
            }
        }
    }
    fn clipboard_paste(&mut self, ctx: &egui::Context) {
        if self.confirm_abandon(PendingAction::Paste) {
            return;
        }
        match arboard::Clipboard::new().and_then(|mut c| c.get_image()) {
            Ok(im) => {
                if let Some(pixels) =
                    RgbaImage::from_raw(im.width as u32, im.height as u32, im.bytes.into_owned())
                {
                    self.path = None;
                    self.file_bytes = None;
                    self.created = None;
                    self.book = None;
                    self.files.clear();
                    self.jpeg = None;
                    self.file_undo = None;
                    self.loader.cancel();
                    self.loading = None;
                    self.image = Some(Arc::new(pixels));
                    self.history.clear();
                    self.redo.clear();
                    self.selection = None;
                    self.dirty = true;
                    self.pan = Vec2::ZERO;
                    self.index = 0;
                    self.lossless = false;
                    self.align_width_top = self.fit == Fit::Width;
                    self.zoom_notice = None;
                    self.update_texture(ctx);
                }
            }
            Err(e) => self.fail(e.to_string()),
        }
    }
    fn move_file(&mut self, copy: bool) {
        let Some(p) = self.path.clone() else {
            return;
        };
        if let Some(dir) = rfd::FileDialog::new()
            .set_title(if copy {
                "Copiar para pasta"
            } else {
                "Mover para pasta"
            })
            .pick_folder()
        {
            let dest = dir.join(p.file_name().unwrap_or_default());
            if dest.exists() {
                self.fail("O destino já existe. Nenhum arquivo foi substituído.");
                return;
            }
            let result = (|| -> std::io::Result<()> {
                // create_new prevents clobbering a destination created after the check.
                let mut input = std::fs::File::open(&p)?;
                let mut output = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&dest)?;
                std::io::copy(&mut input, &mut output)?;
                output.sync_all()?;
                if !copy {
                    std::fs::remove_file(&p)?;
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    if !copy {
                        if let Some(u) = &mut self.file_undo {
                            if u.path == p {
                                u.path = dest.clone();
                            }
                        }
                        self.open(dest);
                    }
                    self.toast(if copy {
                        "Arquivo copiado."
                    } else {
                        "Arquivo movido."
                    });
                }
                Err(e) => self.fail(e.to_string()),
            }
        }
    }
    fn delete(&mut self, permanent: bool) {
        if self.confirm_abandon(PendingAction::Delete(permanent)) {
            return;
        }
        let Some(p) = self.path.clone() else {
            return;
        };
        let result = if permanent {
            std::fs::remove_file(&p).map_err(|e| e.to_string())
        } else {
            trash::delete(&p).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                self.files.retain(|f| f != &p);
                self.image = None;
                self.texture = None;
                self.path = None;
                self.file_bytes = None;
                self.created = None;
                self.file_undo = None;
                if !self.files.is_empty() {
                    self.index = self.index.min(self.files.len() - 1);
                    self.request_load(self.files[self.index].clone());
                }
            }
            Err(e) => self.fail(e),
        }
    }
    fn start_batch(&mut self, files: Vec<PathBuf>, dest: PathBuf, format: Format, quality: u8) {
        if files.is_empty() {
            return;
        }
        self.busy = true;
        self.batch_progress = Some((0, files.len()));
        let tx = self.tx.clone();
        let cancel = Arc::new(AtomicBool::new(false));
        self.batch_cancel = Some(cancel.clone());
        std::thread::spawn(move || {
            let mut errors = vec![];
            let total = files.len();
            for (n, p) in files.iter().enumerate() {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let mut target = dest
                    .join(p.file_stem().unwrap_or_default())
                    .with_extension(format.ext());
                let mut count = 1;
                while target.exists() {
                    target = dest.join(format!(
                        "{}-{count}.{}",
                        p.file_stem().unwrap_or_default().to_string_lossy(),
                        format.ext()
                    ));
                    count += 1;
                }
                let r = backend::load(p)
                    .and_then(|im| backend::export_new(&im.pixels, &target, format, quality));
                if let Err(e) = r {
                    errors.push(format!("{}: {e}", p.display()));
                }
                let _ = tx.send(Message::BatchProgress(n + 1, total));
            }
            let _ = tx.send(Message::BatchDone(errors));
        });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        // egui-winit consumes the press of Ctrl+V for text paste, even if
        // the clipboard only holds an image. Its release still carries modifiers.
        let paste = events.iter().any(|e| match e {
            egui::Event::Paste(_) => true,
            egui::Event::Key {
                key: Key::V,
                pressed: false,
                modifiers,
                ..
            } => modifiers.command || modifiers.ctrl,
            _ => false,
        });
        if paste
            && self.dialog.is_none()
            && self.error.is_none()
            && !self.busy
            && !ctx.wants_keyboard_input()
        {
            self.clipboard_paste(ctx);
        }
        for e in events {
            if self.dialog.is_none()
                && self.error.is_none()
                && !self.busy
                && !ctx.wants_keyboard_input()
            {
                match &e {
                    egui::Event::Copy => {
                        self.clipboard_copy();
                        continue;
                    }
                    egui::Event::Cut
                        if ctx.input(|i| {
                            model::shift_delete_cut(
                                i.modifiers.shift,
                                i.modifiers.ctrl || i.modifiers.command,
                                i.modifiers.alt,
                            )
                        }) =>
                    {
                        if self.loading.is_none() && self.path.is_some() {
                            self.dialog = Some(Dialog::Delete {
                                permanent: true,
                                cancel_selected: false,
                            });
                        }
                        continue;
                    }
                    egui::Event::Paste(_) => continue,
                    _ => {}
                }
            }
            let egui::Event::Key {
                key,
                pressed: true,
                modifiers: m,
                ..
            } = e
            else {
                continue;
            };
            if self.error.is_some() {
                if key == Key::Escape || key == Key::Enter {
                    self.error = None;
                }
                continue;
            }
            if self.dialog.is_some() {
                if key == Key::Escape {
                    self.dialog = None;
                }
                continue;
            }
            if ctx.wants_keyboard_input() {
                continue;
            }
            if self.slide && self.paused {
                self.paused = false;
                self.last_slide = Instant::now();
            }
            if key == Key::Escape {
                if let Some(cancel) = &self.batch_cancel {
                    cancel.store(true, Ordering::Relaxed);
                    self.toast("Cancelando após a imagem atual");
                    continue;
                }
                if self.selection.take().is_some() {
                    self.lossless = false;
                } else if self.fullscreen {
                    self.toggle_fullscreen(ctx);
                    self.slide = false;
                } else {
                    self.close_program(ctx);
                }
                continue;
            }
            if key == Key::Enter || (key == Key::F && !m.ctrl && !m.alt && !m.command) {
                self.toggle_fullscreen(ctx);
                continue;
            }
            if key == Key::T && !m.ctrl && !m.alt && !m.command {
                if self.fullscreen {
                    self.show_art_title = !self.show_art_title;
                }
                continue;
            }
            if self.busy {
                continue;
            }
            // Keep navigation responsive, but never edit the previously displayed file
            // while a different file is being decoded.
            if self.loading.is_some()
                && !matches!(
                    key,
                    Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::ArrowDown
                        | Key::Space
                        | Key::Backspace
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Home
                        | Key::End
                        | Key::Num0
                        | Key::Num1
                        | Key::Num2
                        | Key::Num3
                        | Key::Num4
                        | Key::Num5
                        | Key::Num6
                        | Key::Num7
                        | Key::Num8
                        | Key::Num9
                        | Key::F5
                        | Key::U
                        | Key::O
                        | Key::F1
                )
            {
                continue;
            }
            if m.ctrl || m.command {
                if let Some(n) = key_number(key) {
                    let amount = if n == 0 { 10 } else { n };
                    self.navigate(if m.shift {
                        -(amount as isize)
                    } else {
                        amount as isize
                    });
                    continue;
                }
                match key {
                    Key::O => self.choose_open(),
                    Key::A => {
                        let (w, h) = self.dimensions();
                        if self.selection == Some(Crop { x: 0, y: 0, w, h }) {
                            self.selection = None;
                        } else {
                            self.selection = Some(Crop { x: 0, y: 0, w, h });
                        }
                    }
                    Key::C | Key::Insert => self.clipboard_copy(),
                    Key::V => self.clipboard_paste(ctx),
                    Key::Y => self.apply_crop(ctx),
                    Key::Z => self.undo(ctx),
                    Key::J => self.redo(ctx),

                    Key::H => {
                        self.set_fit(Fit::Original);
                    }
                    Key::G => self.edit(ctx, |im| {
                        image::DynamicImage::ImageRgba8(im.clone())
                            .grayscale()
                            .to_rgba8()
                    }),
                    Key::R => {
                        let (w, h) = self.dimensions();
                        self.dialog = Some(Dialog::Resize(w, h, true));
                    }
                    Key::S => {
                        if m.shift {
                            self.export_dialog(Format::Webp)
                        } else {
                            self.save_original()
                        }
                    }
                    Key::L if m.shift => {
                        self.begin_crop(RATIOS[self.settings.ratio.min(RATIOS.len() - 1)].1, true)
                    }
                    Key::N => {
                        if let Some(p) = &self.path {
                            let mut c =
                                std::process::Command::new(std::env::current_exe().unwrap());
                            c.arg(p);
                            if let Err(e) = c.spawn() {
                                self.fail(e.to_string());
                            }
                        }
                    }
                    Key::M => {
                        if !self.files.is_empty() {
                            let n = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .subsec_nanos() as usize
                                % self.files.len();
                            self.navigate(n as isize - self.index as isize);
                        }
                    }
                    Key::F => self.dialog = Some(Dialog::Search(String::new())),
                    Key::W => {
                        self.slide = !self.slide;
                        self.paused = false;
                        self.last_slide = Instant::now();
                    }
                    Key::Home => self.navigate(-(self.index as isize)),
                    Key::End => self.navigate(self.files.len() as isize - 1 - self.index as isize),
                    Key::PageUp => self.navigate(-1),
                    Key::PageDown => self.navigate(1),
                    Key::Space if self.selection.is_none() => self.navigate(5),
                    Key::Backspace if self.selection.is_none() => self.navigate(-5),
                    Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                        self.arrow(key, m)
                    }
                    _ => {}
                }
            } else {
                match key {
                    Key::F1 => self.dialog = Some(Dialog::Help),
                    Key::F2 | Key::F6 => {
                        if let Some(p) = &self.path {
                            self.dialog = Some(Dialog::Rename(
                                p.file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                p.extension()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                                true,
                            ));
                        }
                    }
                    Key::F5 | Key::U => {
                        if let Some(p) = self.source_path() {
                            self.open(p);
                        }
                    }
                    Key::F7 => self.move_file(false),
                    Key::F8 => self.move_file(true),
                    Key::F11 => self.hide_cursor = !self.hide_cursor,
                    Key::P => self.dialog = Some(Dialog::Settings),
                    Key::I => self.show_information(),
                    Key::S => self.export_dialog(Format::Jpeg),
                    Key::Delete => {
                        if self.path.is_some() {
                            self.dialog = Some(Dialog::Delete {
                                permanent: m.shift,
                                cancel_selected: false,
                            });
                        }
                    }
                    Key::Plus | Key::Equals => self.zoom_by(1.25),
                    Key::Minus => self.zoom_by(0.8),
                    Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => {
                        self.arrow(key, m)
                    }
                    Key::Space | Key::PageDown => self.navigate(1),
                    Key::Backspace | Key::PageUp => self.navigate(-1),
                    Key::Home => {
                        if self.fit == Fit::Custom || self.fit == Fit::Original {
                            self.pan.x = 0.0;
                        } else {
                            self.navigate(-(self.index as isize));
                        }
                    }
                    Key::End => self.navigate(self.files.len() as isize - 1 - self.index as isize),
                    _ => {
                        if let Some(n) = key_number(key) {
                            if (1..=7).contains(&n) {
                                self.set_fit(FITS[n - 1].1);
                            }
                        }
                    }
                }
            }
        }
    }
    fn arrow(&mut self, key: Key, m: egui::Modifiers) {
        let (dx, dy) = match key {
            Key::ArrowLeft => (-1, 0),
            Key::ArrowRight => (1, 0),
            Key::ArrowUp => (0, -1),
            _ => (0, 1),
        };
        if let Some(mut c) = self.selection {
            let (w, h) = self.dimensions();
            let amount = if m.shift { 10 } else { 1 };
            let (dx, dy) = (dx * amount, dy * amount);
            if m.ctrl || m.command {
                let r = c.w as f32 / c.h as f32;
                let nh = if dy != 0 {
                    (c.h as i32 + dy).max(1) as u32
                } else {
                    ((c.w as i32 + dx).max(1) as f32 / r).round().max(1.0) as u32
                };
                c.h = nh.min(h - c.y);
                c.w = (c.h as f32 * r).round().max(1.0) as u32;
                if c.w > w - c.x {
                    c.w = w - c.x;
                    c.h = (c.w as f32 / r).round().max(1.0) as u32;
                }
            } else if m.alt {
                c.w = (c.w as i64 + dx as i64).clamp(1, (w - c.x) as i64) as u32;
                c.h = (c.h as i64 + dy as i64).clamp(1, (h - c.y) as i64) as u32;
                self.ratio = None;
            } else {
                let step = if self.lossless {
                    self.jpeg
                        .map(|j| if dx != 0 { j.mcu.0 } else { j.mcu.1 })
                        .unwrap_or(1) as i64
                } else {
                    1
                };
                c.x = (c.x as i64 + dx as i64 * step).clamp(0, (w - c.w) as i64) as u32;
                c.y = (c.y as i64 + dy as i64 * step).clamp(0, (h - c.h) as i64) as u32;
            }
            self.selection = Some(c);
            self.align_selection();
        } else if dx != 0 {
            self.navigate(dx as isize * if m.ctrl { 5 } else { 1 });
        } else {
            self.pan.y -= dy as f32 * 40.0;
        }
    }

    fn menus(&mut self, ctx: &egui::Context) {
        let menu = egui::Area::new(egui::Id::new("menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let font = egui::TextStyle::Button.resolve(ui.style());
                let menu_width = ["Arquivo", "Editar", "Imagem", "Exibir", "Ajuda"]
                    .iter()
                    .map(|label| {
                        ui.painter()
                            .layout_no_wrap(t(label), font.clone(), Color32::WHITE)
                            .size()
                            .x
                            + 2.0 * ui.spacing().button_padding.x
                            + ui.spacing().item_spacing.x
                    })
                    .sum::<f32>();
                ui.set_width(if self.settings.show_toolbar {
                    10.0 * 30.0
                } else {
                    menu_width
                });
                self.menu_visuals(ui);
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(egui::Margin::symmetric(4, 2))
                    .show(ui, |ui| {
                        if self.busy || self.loading.is_some() {
                            ui.disable();
                        }
                        egui::menu::bar(ui, |ui| {
                            ui.menu_button(t("Arquivo"), |ui| {
                                if item(ui, "Abrir", "Ctrl+O") {
                                    self.choose_open();
                                }
                                if item(ui, "Salvar imagem", "Ctrl+S") {
                                    self.save_original();
                                }
                                if item(ui, "Renomear", "F2") {
                                    if let Some(p) = &self.path {
                                        self.dialog = Some(Dialog::Rename(
                                            p.file_stem()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .into(),
                                            p.extension()
                                                .unwrap_or_default()
                                                .to_string_lossy()
                                                .into_owned(),
                                            true,
                                        ));
                                    }
                                }
                                ui.separator();
                                if item(
                                    ui,
                                    &format!("Exportar JPEG (Qld {}%)", self.settings.jpeg_quality),
                                    "S",
                                ) {
                                    self.export_dialog(Format::Jpeg);
                                }
                                if item(
                                    ui,
                                    &format!("Exportar WebP (Qld {}%)", self.settings.webp_quality),
                                    "Ctrl+Shift+S",
                                ) {
                                    self.export_dialog(Format::Webp);
                                }
                                if item(ui, "Exportar PNG (sem perdas)", "") {
                                    self.export_dialog(Format::Png);
                                }
                                if item(ui, "Converter em lote", "") {
                                    self.dialog = Some(Dialog::Batch {
                                        files: vec![],
                                        dest: self
                                            .path
                                            .as_ref()
                                            .and_then(|p| p.parent())
                                            .unwrap_or(Path::new("."))
                                            .into(),
                                        format: Format::Webp,
                                        quality: 80,
                                    });
                                }
                                ui.separator();
                                if item(ui, "Mover arquivo", "F7") {
                                    self.move_file(false);
                                }
                                if item(ui, "Copiar arquivo", "F8") {
                                    self.move_file(true);
                                }
                                if item(ui, "Enviar à lixeira", "Del") {
                                    self.dialog = Some(Dialog::Delete {
                                        permanent: false,
                                        cancel_selected: false,
                                    });
                                }
                                if item(ui, "Excluir permanentemente", "Shift+Del") {
                                    self.dialog = Some(Dialog::Delete {
                                        permanent: true,
                                        cancel_selected: false,
                                    });
                                }
                                ui.separator();
                                if item(ui, "Sair", "Esc") {
                                    self.close_program(ctx);
                                }
                            });
                            ui.menu_button(t("Editar"), |ui| {
                                if item(ui, "Desfazer", "Ctrl+Z") {
                                    self.undo(ctx);
                                }
                                if item(ui, "Refazer", "Ctrl+J") {
                                    self.redo(ctx);
                                }
                                ui.separator();
                                if item(ui, "Copiar imagem / seleção", "Ctrl+C") {
                                    self.clipboard_copy();
                                }
                                if item(ui, "Colar como nova imagem", "Ctrl+V") {
                                    self.clipboard_paste(ctx);
                                }
                                if item(ui, "Selecionar imagem inteira", "Ctrl+A") {
                                    let (w, h) = self.dimensions();
                                    self.selection = Some(Crop { x: 0, y: 0, w, h });
                                }
                                if item(ui, "Cancelar seleção", "Esc") {
                                    self.selection = None;
                                    self.lossless = false;
                                }
                                ui.separator();
                                if item(ui, "Procurar na pasta", "Ctrl+F") {
                                    self.dialog = Some(Dialog::Search(String::new()));
                                }
                                if item(ui, "Preferências", "P") {
                                    self.dialog = Some(Dialog::Settings);
                                }
                            });
                            ui.menu_button(t("Imagem"), |ui| {
                                if item(ui, "Informações da imagem", "I") {
                                    self.show_information();
                                }
                                ui.separator();
                                ui.menu_button(t("Recortar"), |ui| {
                                    self.crop_choices(ui, false);
                                });
                                let jpeg_crop = self.jpeg.is_some() && !self.dirty;
                                ui.add_enabled_ui(jpeg_crop, |ui| {
                                    ui.menu_button(t("Recorte JPEG sem perdas"), |ui| {
                                        self.crop_choices(ui, true);
                                    });
                                });
                                if item(ui, "Aplicar recorte", "Ctrl+Y") {
                                    self.apply_crop(ctx);
                                }
                                if item(ui, "Inverter proporção", "") {
                                    self.invert_crop_ratio();
                                }
                                ui.separator();
                                ui.add_enabled_ui(self.jpeg.is_some() && !self.dirty, |ui| {
                                    ui.menu_button(t("Rotação JPEG sem perdas"), |ui| {
                                        for (label, degrees) in [
                                            ("Girar 90° à direita", 90),
                                            ("Girar 90° à esquerda", 270),
                                            ("Girar 180°", 180),
                                        ] {
                                            if item(ui, label, "") {
                                                self.rotate_lossless(degrees);
                                            }
                                        }
                                    });
                                });
                                ui.separator();
                                if item(ui, "Girar 90° à direita", "") {
                                    self.edit(ctx, image::imageops::rotate90);
                                }
                                if item(ui, "Girar 90° à esquerda", "") {
                                    self.edit(ctx, image::imageops::rotate270);
                                }
                                if item(ui, "Espelhar horizontalmente", "") {
                                    self.edit(ctx, image::imageops::flip_horizontal);
                                }
                                if item(ui, "Espelhar verticalmente", "") {
                                    self.edit(ctx, image::imageops::flip_vertical);
                                }
                                if item(ui, "Escala de cinza", "Ctrl+G") {
                                    self.edit(ctx, |im| {
                                        image::DynamicImage::ImageRgba8(im.clone())
                                            .grayscale()
                                            .to_rgba8()
                                    });
                                }
                                if item(ui, "Redimensionar", "Ctrl+R") {
                                    let (w, h) = self.dimensions();
                                    self.dialog = Some(Dialog::Resize(w, h, true));
                                }
                            });
                            ui.menu_button(t("Exibir"), |ui| {
                                ui.checkbox(
                                    &mut self.settings.show_toolbar,
                                    t("Barra de ferramentas"),
                                );
                                ui.checkbox(
                                    &mut self.settings.show_status_bar,
                                    t("Barra de status"),
                                );
                                ui.separator();
                                if item(ui, "Recarregar imagem e pasta", "F5") {
                                    if let Some(p) = self.source_path() {
                                        self.open(p);
                                    }
                                }
                                if item(
                                    ui,
                                    if self.slide {
                                        "Parar apresentação"
                                    } else {
                                        "Iniciar apresentação"
                                    },
                                    "Ctrl+W",
                                ) {
                                    self.slide = !self.slide;
                                    self.last_slide = Instant::now();
                                }
                                if self.slide
                                    && item(
                                        ui,
                                        if self.paused { "Continuar" } else { "Pausar" },
                                        "",
                                    )
                                {
                                    self.paused = !self.paused;
                                }

                                ui.separator();

                                if item(ui, "Tela cheia / janela", "Enter / F") {
                                    self.toggle_fullscreen(ctx);
                                }
                                ui.menu_button(t("Zoom"), |ui| {
                                    if item(ui, "Ampliar", "+ / =") {
                                        self.zoom_by(1.25);
                                    }
                                    if item(ui, "Reduzir", "−") {
                                        self.zoom_by(0.8);
                                    }
                                    ui.separator();
                                    for (label, fit) in FITS {
                                        if item(ui, label, "") {
                                            self.set_fit(fit);
                                        }
                                    }
                                });
                            });
                            ui.menu_button(t("Ajuda"), |ui| {
                                if item(ui, "Atalhos", "F1") {
                                    self.dialog = Some(Dialog::Help);
                                }
                                if item(ui, "Sobre", "") {
                                    self.dialog = Some(Dialog::About);
                                }
                            });
                        });
                    });
            });
        self.menu_bottom = menu.response.rect.bottom();
    }

    fn toolbar(&mut self, ctx: &egui::Context) {
        if !self.settings.show_toolbar {
            return;
        }
        egui::Area::new(egui::Id::new("compact_toolbar"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::new(0.0, self.menu_bottom))
            .show(ctx, |ui| {
                self.menu_visuals(ui);
                ui.set_width(10.0 * 30.0);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.spacing_mut().interact_size.y = 30.0;
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(
                            !self.busy
                                && self.loading.is_none()
                                && self.dialog.is_none()
                                && self.error.is_none(),
                            |ui| {
                                ui.horizontal(|ui| {
                                    let loaded = self.image.is_some();
                                    if tool_button(ui, "Abrir", true) {
                                        self.choose_open();
                                    }
                                    if tool_button(ui, "Salvar", loaded) {
                                        self.save_original();
                                    }
                                    if tool_button(ui, "Anterior", !self.files.is_empty()) {
                                        self.navigate(-1);
                                    }
                                    if tool_button(ui, "Próxima", !self.files.is_empty()) {
                                        self.navigate(1);
                                    }
                                    if tool_button(ui, "−", loaded) {
                                        self.zoom_by(0.8);
                                    }
                                    if tool_button(ui, "+", loaded) {
                                        self.zoom_by(1.25);
                                    }
                                    if tool_button(ui, "1:1", loaded) {
                                        self.set_fit(Fit::Original);
                                    }
                                    if tool_button(ui, "Ajustar", loaded) {
                                        self.set_fit(Fit::All);
                                    }

                                    if tool_button(
                                        ui,
                                        if self.slide {
                                            "Parar apresentação"
                                        } else {
                                            "Iniciar apresentação"
                                        },
                                        !self.files.is_empty(),
                                    ) {
                                        self.slide = !self.slide;
                                        self.last_slide = Instant::now();
                                    }
                                    if tool_button(ui, "Informações da imagem", loaded) {
                                        self.show_information();
                                    }
                                });
                            },
                        );
                    });
            });
    }

    fn status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .show_animated(ctx, self.settings.show_status_bar, |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt("status_scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if self.busy || self.loading.is_some() {
                                ui.spinner();
                                ui.label(t("Carregando"));
                            }
                            if let Some(im) = &self.image {
                                let (w, h) = im.dimensions();
                                ui.add_sized(
                                    [180.0, 18.0],
                                    egui::Label::new(self.display_name()).truncate(),
                                )
                                .on_hover_text(self.display_name());
                                ui.separator();
                                ui.label(format!("{w} × {h} px"));
                                ui.separator();
                                if !self.files.is_empty() {
                                    ui.label(format!("{}/{}", self.index + 1, self.files.len()));
                                    ui.separator();
                                }
                                // Use this frame's remaining canvas size, including the optional bars.
                                let rect = self.canvas;
                                let (sx, sy) = model::display_scale(
                                    self.fit,
                                    (w, h),
                                    (rect.width(), rect.height()),
                                    self.zoom,
                                    self.pixel_ratio,
                                );
                                ui.label(if (sx - sy).abs() < 0.001 {
                                    format!("{:.1}%", sx * self.pixel_ratio * 100.0)
                                } else {
                                    format!(
                                        "{:.1}% × {:.1}%",
                                        sx * self.pixel_ratio * 100.0,
                                        sy * self.pixel_ratio * 100.0
                                    )
                                });
                                ui.separator();
                                if let Some(bytes) = self.file_bytes {
                                    ui.label(format!("{}: {}", t("Arquivo"), human_bytes(bytes)));
                                }
                                ui.label(format!("RAM: {}", human_bytes(w as u64 * h as u64 * 4)));
                                ui.separator();
                                ui.label(format!(
                                    "{}: {}",
                                    t("Criado em"),
                                    self.created.as_deref().unwrap_or("—")
                                ))
                                .on_hover_text(t("Data de criação do arquivo no sistema"));
                                if self.has_alpha {
                                    ui.label(t("Transparência"));
                                }
                                if self.dirty {
                                    ui.separator();
                                    ui.label(t("Modificada"));
                                }
                            } else {
                                ui.label(t("Nenhuma imagem aberta"));
                            }
                        });
                    });
            });
    }

    fn canvas(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(Color32::from_rgb(self.settings.screen_color[0],self.settings.screen_color[1],self.settings.screen_color[2]))).show(ctx,|ui|{
            let rect=ui.max_rect();self.canvas=rect;
            let response=ui.allocate_rect(rect,if self.loading.is_none(){Sense::click_and_drag()}else{Sense::hover()});
            let display_image=self.image.clone();
            if let (Some(im),Some(texture))=(&display_image,&self.texture) {
                let(w,h)=im.dimensions();
                let(sx,sy)=model::display_scale(self.fit,(w,h),(rect.width(),rect.height()),self.zoom,self.pixel_ratio);
                let size=Vec2::new(w as f32*sx,h as f32*sy);
                let limit=Vec2::new(((size.x-rect.width())/2.0).max(0.0),((size.y-rect.height())/2.0).max(0.0));
                if self.align_width_top {self.pan.y=limit.y;self.align_width_top=false;}
                self.pan.x=self.pan.x.clamp(-limit.x,limit.x);self.pan.y=self.pan.y.clamp(-limit.y,limit.y);
                let shown=Rect::from_center_size(rect.center()+self.pan,size);let shown=Rect::from_min_size(Pos2::new((shown.min.x*ctx.pixels_per_point()).round()/ctx.pixels_per_point(),(shown.min.y*ctx.pixels_per_point()).round()/ctx.pixels_per_point()),shown.size());self.shown=shown;
                if self.settings.checkerboard && self.has_alpha {
                    ui.painter().image(self.checker_texture.id(),shown,Rect::from_min_max(Pos2::ZERO,Pos2::new(size.x/32.0,size.y/32.0)),Color32::WHITE);
                }
                let preview_scale=texture.size()[0]as f32/w as f32;
                let settled=self.loading.is_none() && self.last_navigation.elapsed()>=Duration::from_millis(150);
                if self.last_navigation.elapsed()<Duration::from_millis(150) {ctx.request_repaint_after(Duration::from_millis(150).saturating_sub(self.last_navigation.elapsed()));}
                if self.loading.is_none() && ((settled && self.fit==Fit::Large && sx*ctx.pixels_per_point()<0.999)||sx*ctx.pixels_per_point()>preview_scale*1.01||sy*ctx.pixels_per_point()>texture.size()[1]as f32/h as f32*1.01){
                    self.tiles.draw(ctx,ui.painter(),im.clone(),shown,rect,texture.id());
                }else{ui.painter().image(texture.id(),shown,Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)),Color32::WHITE);}
                if let Some(c)=self.selection {
                    let sel=Rect::from_min_size(shown.min+Vec2::new(c.x as f32*sx,c.y as f32*sy),Vec2::new(c.w as f32*sx,c.h as f32*sy));
                    let dark=Color32::from_black_alpha(145);
                    for r in [Rect::from_min_max(shown.min,Pos2::new(shown.max.x,sel.min.y)),Rect::from_min_max(Pos2::new(shown.min.x,sel.max.y),shown.max),Rect::from_min_max(Pos2::new(shown.min.x,sel.min.y),Pos2::new(sel.min.x,sel.max.y)),Rect::from_min_max(Pos2::new(sel.max.x,sel.min.y),Pos2::new(shown.max.x,sel.max.y))]{ui.painter().rect_filled(r,0.0,dark);}
                    ui.painter().rect_stroke(sel,0.0,Stroke::new(1.0_f32,Color32::WHITE),egui::StrokeKind::Inside);
                    for fraction in [1.0/3.0,2.0/3.0] {
                        let x=sel.left()+sel.width()*fraction;
                        let y=sel.top()+sel.height()*fraction;
                        for line in [[Pos2::new(x,sel.top()),Pos2::new(x,sel.bottom())],[Pos2::new(sel.left(),y),Pos2::new(sel.right(),y)]] {
                            ui.painter().line_segment(line,Stroke::new(2.0_f32,Color32::from_black_alpha(120)));
                            ui.painter().line_segment(line,Stroke::new(1.0_f32,Color32::from_white_alpha(180)));
                        }
                    }
                    let handle=(sel.width().min(sel.height())*0.12).clamp(8.0,24.0);
                    for (pos,dx,dy) in [(sel.left_top(),1.0,1.0),(sel.right_top(),-1.0,1.0),(sel.left_bottom(),1.0,-1.0),(sel.right_bottom(),-1.0,-1.0)] {
                        let points=vec![pos+Vec2::new(dx*handle,0.0),pos,pos+Vec2::new(0.0,dy*handle)];
                        ui.painter().add(egui::Shape::line(points.clone(),Stroke::new(7.0_f32,Color32::from_black_alpha(150))));
                        ui.painter().add(egui::Shape::line(points,Stroke::new(5.0_f32,Color32::WHITE)));
                    }
                    for (pos,horizontal) in [(sel.center_top(),true),(sel.center_bottom(),true),(sel.left_center(),false),(sel.right_center(),false)] {
                        let d=if horizontal{Vec2::new(handle,0.0)}else{Vec2::new(0.0,handle)};
                        ui.painter().line_segment([pos-d,pos+d],Stroke::new(7.0_f32,Color32::from_black_alpha(150)));
                        ui.painter().line_segment([pos-d,pos+d],Stroke::new(5.0_f32,Color32::WHITE));
                    }
                    let actual=c.w as f32/c.h as f32;
                    let label=format!("{} × {}{}",c.w,c.h,if self.lossless{format!(" · JPEG sem perdas · proporção {actual:.4}")}else{String::new()});
                    let label=t(&label);
                    let pos=sel.min+Vec2::new(6.0,6.0);
                    let font=egui::FontId::monospace(14.0);
                    for offset in [Vec2::new(-1.0,0.0),Vec2::new(1.0,0.0),Vec2::new(0.0,-1.0),Vec2::new(0.0,1.0)] {
                        ui.painter().text(pos+offset,egui::Align2::LEFT_TOP,&label,font.clone(),Color32::BLACK);
                    }
                    ui.painter().text(pos,egui::Align2::LEFT_TOP,label,font,Color32::WHITE);
                    if response.drag_started_by(egui::PointerButton::Primary) {
                        if let Some(p)=response.interact_pointer_pos(){
                            let corner=[sel.left_top(),sel.right_top(),sel.left_bottom(),sel.right_bottom()].iter().position(|q|q.distance(p)<14.0);
                            let edge=if p.y>=sel.top()-10.0&&p.y<=sel.bottom()+10.0&&(p.x-sel.left()).abs()<10.0{Some(6)}else if p.y>=sel.top()-10.0&&p.y<=sel.bottom()+10.0&&(p.x-sel.right()).abs()<10.0{Some(7)}else if p.x>=sel.left()-10.0&&p.x<=sel.right()+10.0&&(p.y-sel.top()).abs()<10.0{Some(8)}else if p.x>=sel.left()-10.0&&p.x<=sel.right()+10.0&&(p.y-sel.bottom()).abs()<10.0{Some(9)}else{None};
                            self.drag_start=Some((p,c,corner.map(|n|n as u8+2).or(edge).unwrap_or(if sel.contains(p){1}else{0})));
                        }
                    }
                    if response.dragged_by(egui::PointerButton::Primary){if let(Some((start,original,kind)),Some(p))=(self.drag_start,response.interact_pointer_pos()) {
                        let dx=((p.x-start.x)/sx).round()as i64;let dy=((p.y-start.y)/sy).round()as i64;
                        let mut c=original;
                        if kind==1{c.x=(original.x as i64+dx).clamp(0,(w-c.w)as i64)as u32;c.y=(original.y as i64+dy).clamp(0,(h-c.h)as i64)as u32;}
                        else if kind>=6 {c=original.resize_side(kind,if kind==6||kind==7{dx}else{dy},w,h,self.ratio);}
                        else {
                            let anchor=if kind>=2{match kind{2=>(original.x+original.w,original.y+original.h),3=>(original.x,original.y+original.h),4=>(original.x+original.w,original.y),_=>(original.x,original.y)}}else{(((start.x-shown.min.x)/sx).clamp(0.0,w as f32)as u32,((start.y-shown.min.y)/sy).clamp(0.0,h as f32)as u32)};
                            let px=((p.x-shown.min.x)/sx).clamp(0.0,w as f32)as u32;let py=((p.y-shown.min.y)/sy).clamp(0.0,h as f32)as u32;
                            let mut cw=anchor.0.abs_diff(px).max(1);let mut ch=anchor.1.abs_diff(py).max(1);
                            if let Some(r)=self.ratio {
                                if cw as f32/ch as f32>r{cw=(ch as f32*r).round().max(1.0)as u32;}else{ch=(cw as f32/r).round().max(1.0)as u32;}
                            }
                            c=Crop{x:if px<anchor.0{anchor.0.saturating_sub(cw)}else{anchor.0},y:if py<anchor.1{anchor.1.saturating_sub(ch)}else{anchor.1},w:cw,h:ch};
                        }
                        self.selection=Some(c);self.align_selection();
                    }}
                }else if response.dragged(){self.pan+=ctx.input(|i|i.pointer.delta());}
                if response.drag_stopped(){self.drag_start=None;}
                if response.hovered() {
                    let (scroll, modifiers)=ctx.input(|i|(i.raw_scroll_delta.y,i.modifiers));
                    if scroll!=0.0 {
                        match model::wheel_action(self.wheel_fit,modifiers.ctrl,modifiers.alt) {
                            model::WheelAction::Zoom=>self.zoom_by(if scroll>0.0{1.1}else{1.0/1.1}),
                            model::WheelAction::Pan=>self.pan.y=(self.pan.y+scroll).clamp(-limit.y,limit.y),
                            model::WheelAction::Navigate=>self.navigate(if scroll>0.0{-1}else{1}),
                        }
                    }
                }
                if response.double_clicked(){self.toggle_fullscreen(ctx);}
                response.context_menu(|ui|{
                    self.menu_visuals(ui);
                    if self.loading.is_some(){ui.disable();}
                    if self.fullscreen {
                        if item(ui,"Sair da tela cheia","Enter / F / Esc"){self.toggle_fullscreen(ctx);}
                        if item(ui,"Imagem anterior","←"){self.navigate(-1);}
                        if item(ui,"Próxima imagem","→"){self.navigate(1);}
                        ui.menu_button(t("Modo de zoom"),|ui|{for (label,fit) in FITS {if item(ui,label,""){self.set_fit(fit);}}});
                        if item(ui,if self.slide{"Parar apresentação"}else{"Iniciar apresentação"},"Ctrl+W"){self.slide = !self.slide;self.last_slide=Instant::now();}
                        if item(ui,"Fechar programa",""){self.close_program(ctx);}
                        return;
                    }
                    ui.menu_button(t("Recortar"), |ui| {
                        self.crop_choices(ui, false);
                    });
                    if self.jpeg.is_some() {
                    ui.add_enabled_ui(!self.dirty, |ui| {
                        ui.menu_button(t("Recorte JPEG"), |ui| {
                            self.crop_choices(ui, true);
                        });
                    });
                    }
                    if item(ui,"Inverter proporção",""){self.invert_crop_ratio();}
                    if item(ui,"Aplicar recorte","Ctrl+Y"){self.apply_crop(ctx);}
                    if item(ui,"Copiar","Ctrl+C"){self.clipboard_copy();}
                });
            }else if !self.busy && self.loading.is_none() {
                egui::Area::new(egui::Id::new("welcome"))
                    .anchor(egui::Align2::CENTER_CENTER,Vec2::ZERO).show(ctx,|ui|{
                        ui.vertical_centered(|ui|{
                            ui.label(egui::RichText::new("Lume").size(32.0).color(Color32::WHITE));
                            ui.label(egui::RichText::new(t("Abra uma imagem ou cole da área de transferência.")).color(Color32::LIGHT_GRAY));
                            ui.add_space(16.0);
                            ui.horizontal(|ui|{
                                if ui.button(t("Abrir imagem")).clicked(){self.choose_open();}
                                if ui.button(t("Colar como nova imagem")).clicked(){self.clipboard_paste(ctx);}
                            });
                            ui.add_space(12.0);
                            ui.label(egui::RichText::new(t("Arraste um arquivo para a janela • Ctrl+O abrir • Ctrl+V colar")).small().color(Color32::GRAY));
                        });
                    });
            }
            if self.fullscreen && self.show_art_title {
                let title=self.display_name();
                let [r,g,b]=self.settings.title_color;
                let color=Color32::from_rgb(r,g,b);
                let outline=if (r as u16+g as u16+b as u16)<384 {Color32::WHITE}else{Color32::BLACK};
                let horizontal=match self.settings.title_alignment {1=>egui::Align::Center,2=>egui::Align::Max,_=>egui::Align::Min};
                let vertical=match self.settings.title_position {1=>egui::Align::Center,2=>egui::Align::Max,_=>egui::Align::Min};
                let anchor=egui::Align2([horizontal,vertical]);
                let pos=anchor.pos_in_rect(&rect.shrink(12.0));
                let font=egui::FontId::proportional(self.settings.title_size.clamp(10.0,36.0));
                for offset in [Vec2::new(-1.0,0.0),Vec2::new(1.0,0.0),Vec2::new(0.0,-1.0),Vec2::new(0.0,1.0)] {
                    ui.painter().text(pos+offset,anchor,&title,font.clone(),outline);
                }
                ui.painter().text(pos,anchor,title,font,color);
            }
            if let Some((label, started))=&self.zoom_notice {
                if started.elapsed()<Duration::from_secs(3) {
                    egui::Area::new(egui::Id::new("zoom_notice"))
                        .anchor(egui::Align2::CENTER_TOP,Vec2::new(0.0,70.0))
                        .interactable(false).order(egui::Order::Foreground).show(ctx,|ui|{
                            egui::Frame::NONE.fill(Color32::from_black_alpha(220))
                                .corner_radius(6.0).inner_margin(14.0).show(ui,|ui|{
                                    let galley=ui.painter().layout_no_wrap(t(label),egui::FontId::proportional(24.0),Color32::WHITE);
                                    let (_,bounds)=ui.allocate_space(galley.size());
                                    ui.painter().galley(bounds.min,galley.clone(),Color32::WHITE);
                                    ui.painter().galley(bounds.min+Vec2::new(0.5,0.0),galley,Color32::WHITE);

                                });
                        });
                    ctx.request_repaint_after(Duration::from_secs(3)-started.elapsed().min(Duration::from_secs(3)));
                }
            }
            if self.busy || self.loading.as_ref().map(|(_,t)|t.elapsed()>Duration::from_millis(250)).unwrap_or(false) {
                let text=self.batch_progress.map(|(n,t)|format!("Convertendo {n}/{t}")).unwrap_or_else(||"Processando".into());
                ui.painter().text(rect.left_bottom()+Vec2::new(12.0,-12.0),egui::Align2::LEFT_BOTTOM,i18n::t(&text),egui::FontId::proportional(14.0),Color32::WHITE);
            }else if let Some((s,t))=&self.notice {if t.elapsed()<Duration::from_secs(4){ui.painter().text(rect.left_bottom()+Vec2::new(12.0,-12.0),egui::Align2::LEFT_BOTTOM,i18n::t(s),egui::FontId::proportional(13.0),Color32::LIGHT_GRAY);}}
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(e) = self.error.clone() {
            egui::Window::new(t("Aviso"))
                .title_bar(false)
                .frame(
                    egui::Frame::window(&ctx.style())
                        .inner_margin(16.0)
                        .corner_radius(8),
                )
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(560.0);
                    if dialog_header(ui, "Aviso") {
                        self.error = None;
                    }
                    ui.label(t(&e));
                    if ui.button(t("OK")).clicked() {
                        self.error = None;
                    }
                });
            return;
        }
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let mut keep = true;
        let title = match &dialog {
            Dialog::Unsaved { .. } => "Alterações não salvas",
            Dialog::FolderEnd { .. } => "Fim da pasta",
            Dialog::Rename(..) => "Renomear arquivo",
            Dialog::ManualRatio { .. } => "Proporção manual",
            Dialog::Resize(..) => "Redimensionar",
            Dialog::Delete { .. } => "Excluir arquivo",
            Dialog::Help => "Lume · Atalhos",
            Dialog::About => "Sobre o Lume",
            Dialog::Information => "Informações da imagem",
            Dialog::SaveOriginal(_) => "Salvar no formato original",
            Dialog::SaveAs { .. } => "Salvar imagem",
            Dialog::Dependencies => "Dependências",
            Dialog::Settings => "Preferências",
            Dialog::Search(_) => "Procurar na pasta",
            Dialog::Batch { .. } => "Converter em lote",
            Dialog::Overwrite(..) => "Substituir arquivo",
        };
        egui::Window::new(t(title)).title_bar(false).frame(egui::Frame::window(&ctx.style()).inner_margin(16.0).corner_radius(8)).collapsible(false).resizable(matches!(dialog,Dialog::Help|Dialog::Search(_))).anchor(egui::Align2::CENTER_CENTER,Vec2::ZERO).show(ctx,|ui|{
            ui.set_max_width(650.0);
            ui.spacing_mut().item_spacing=Vec2::new(10.0,10.0);
            ui.spacing_mut().button_padding=Vec2::new(14.0,7.0);
            ui.add_space(6.0);
            if dialog_header(ui,title) {keep=false;}
            match &mut dialog {
                Dialog::Unsaved { action, cancel_selected } => {
                    ui.set_width(440.0);
                    ui.label(t("Esta imagem tem alterações não salvas. Deseja descartá-las?"));
                    let (tab, enter) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::Tab) || i.consume_key(egui::Modifiers::SHIFT, Key::Tab), i.consume_key(egui::Modifiers::NONE, Key::Enter)));
                    if tab { *cancel_selected = !*cancel_selected; }
                    ui.horizontal(|ui| {
                        let discard = ui.button(t("Descartar alterações"));
                        if !*cancel_selected { discard.request_focus(); }
                        if discard.clicked() || (enter && !*cancel_selected) {
                            keep = false;
                            self.dirty = false;
                            if let Some(action) = action.take() {
                                match action {
                                    PendingAction::Open(p) => self.open(p),
                                    PendingAction::Load(p) => {
                                        self.index = self.files.iter().position(|f| f == &p).unwrap_or(self.index);
                                        self.request_load(p);
                                    },
                                    PendingAction::Navigate(delta) => self.navigate(delta),
                                    PendingAction::Paste => self.clipboard_paste(ctx),
                                    PendingAction::Delete(permanent) => self.delete(permanent),
                                    PendingAction::Close => self.close_program(ctx),
                                }
                            }
                        }
                        let cancel = ui.button(t("Cancelar"));
                        if *cancel_selected { cancel.request_focus(); }
                        if cancel.clicked() || (enter && *cancel_selected) { keep = false; }
                    });
                },
                Dialog::FolderEnd { selected } => {
                    let (next, previous, enter) = ctx.input_mut(|i| {
                        let next=i.consume_key(egui::Modifiers::NONE,Key::Tab)||i.consume_key(egui::Modifiers::NONE,Key::ArrowRight)||i.consume_key(egui::Modifiers::NONE,Key::ArrowDown);
                        let previous=i.consume_key(egui::Modifiers::SHIFT,Key::Tab)||i.consume_key(egui::Modifiers::NONE,Key::ArrowLeft)||i.consume_key(egui::Modifiers::NONE,Key::ArrowUp);
                        let enter=i.consume_key(egui::Modifiers::NONE,Key::Enter);
                        (next,previous,enter)
                    });
                    if next {*selected=(*selected+1)%3;}
                    if previous {*selected=(*selected+2)%3;}
                    ui.set_width(450.0);
                    ui.label(egui::RichText::new(t("Você chegou à última imagem desta pasta.")).strong().size(17.0));
                    ui.label(t("Deseja voltar à primeira imagem ou abrir outra imagem?"));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        for (n,label) in ["Reiniciar","Abrir outra imagem","Cancelar"].iter().enumerate() {
                            let button=ui.button(t(label));
                            if *selected==n {button.request_focus();}
                            if button.clicked() || (enter && *selected==n) {
                                keep=false;
                                match n {
                                    0=>{self.index=0;self.request_load(self.files[0].clone());self.last_slide=Instant::now();},
                                    1=>self.choose_open(),
                                    _=>{}
                                }
                            }
                        }
                    });
                    ui.small(t("Tab ou setas: escolher · Enter: confirmar · Esc: cancelar"));
                },
                Dialog::Rename(name,extension,focus)=>{
                    ui.label(t("Novo nome:"));
                    let r=ui.add(egui::TextEdit::singleline(name).desired_width(400.0));
                    if *focus{
                        r.request_focus();let mut state=egui::TextEdit::load_state(ctx,r.id).unwrap_or_default();
                        state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0),egui::text::CCursor::new(name.chars().count()))));
                        state.store(ctx,r.id);*focus=false;
                    }
                    ui.label(t("Extensão:"));
                    ui.add(egui::TextEdit::singleline(extension).desired_width(100.0));
                    ui.small(t("Alterar a extensão não converte o formato da imagem."));
                    if ui.button(t("Renomear")).clicked()||ctx.input(|i|i.key_pressed(Key::Enter)){let extension=extension.trim().trim_start_matches('.');let filename=if extension.is_empty(){name.clone()}else{format!("{name}.{extension}")};self.rename(&filename);keep=false;}
                },
                Dialog::ManualRatio { width, height, lossless } => {
                    ui.set_width(420.0);
                    ui.label(t("Proporção largura : altura"));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(width).desired_width(100.0));
                        if icons::invert_ratio_button(ui).clicked() { std::mem::swap(width, height); }
                        ui.add(egui::TextEdit::singleline(height).desired_width(100.0));
                    });
                    let parsed = width.trim().replace(',', ".").parse::<f32>().ok().zip(height.trim().replace(',', ".").parse::<f32>().ok()).filter(|(w,h)| w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0 && (w/h).is_finite() && w/h > 0.0);
                    let apply = ui.add_enabled(parsed.is_some(),egui::Button::new(t("Aplicar"))).clicked() || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE,Key::Enter));
                    if let Some((w,h)) = parsed.filter(|_| apply) {
                        self.settings.manual_ratio = [w,h];
                        self.begin_crop(Some(w/h), *lossless);
                        keep = false;
                    }
                },
                Dialog::Resize(w,h,locked)=>{
                    let(ow,oh)=self.dimensions();let oldw=*w;let oldh=*h;
                    ui.horizontal(|ui|{ui.label(t("Largura"));ui.add(egui::DragValue::new(w).range(1..=30000));ui.label(t("Altura"));ui.add(egui::DragValue::new(h).range(1..=30000));});
                    ui.checkbox(locked,t("Manter proporção"));
                    if *locked{if *w!=oldw{*h=(*w as f64*oh as f64/ow as f64).round().max(1.0)as u32;}else if *h!=oldh{*w=(*h as f64*ow as f64/oh as f64).round().max(1.0)as u32;}}
                    if ui.button(t("Aplicar")).clicked(){if *w as u64 * *h as u64>100_000_000{self.fail("Limite desta versão: 100 megapixels.");}else{let(w,h)=(*w,*h);self.edit(ctx,|im|image::imageops::resize(im,w,h,image::imageops::FilterType::Lanczos3));keep=false;}}
                },
                Dialog::Delete { permanent, cancel_selected }=>{
                    // Own the two-button keyboard cycle so Tab never escapes this dialog.
                    let (tab, enter)=ctx.input_mut(|i| {
                        let tab=i.consume_key(egui::Modifiers::NONE,Key::Tab)
                            ||i.consume_key(egui::Modifiers::SHIFT,Key::Tab);
                        let enter=i.consume_key(egui::Modifiers::NONE,Key::Enter);
                        (tab,enter)
                    });
                    if tab{*cancel_selected = !*cancel_selected;}
                    ui.set_width(440.0);
                    ui.label(egui::RichText::new(t(if *permanent{"Excluir permanentemente este arquivo?"}else{"Enviar este arquivo à lixeira?"})).size(18.0).strong());
                    if let Some(p)=&self.path{dialog_file(ui,p);}
                    ui.add_space(4.0);
                    ui.separator();
                    ui.horizontal(|ui|{
                        let mut button=egui::Button::new(t(if *permanent{"Excluir permanentemente"}else{"Enviar à lixeira"}));
                        if *permanent{button=button.fill(Color32::from_rgb(150,42,42));}
                        let confirm=ui.add(button);
                        if !*cancel_selected{confirm.request_focus();}
                        if confirm.clicked()||(enter&&!*cancel_selected){self.delete(*permanent);keep=false;}
                        let cancel=ui.button(t("Cancelar"));
                        if *cancel_selected{cancel.request_focus();}
                        if cancel.clicked()||(enter&&*cancel_selected){keep=false;}
                    });
                    ui.small(t("Enter: confirmar opção · Tab: alternar · Esc: cancelar"));
                },
                Dialog::Overwrite(p,f)=>{ui.set_width(440.0);ui.label(t("Substituir arquivo"));dialog_file(ui,p);if ui.button(t("Substituir")).clicked(){self.export_to(p.clone(),*f);keep=false;}},
                Dialog::Information=>{
                    ui.set_width(600.0);
                    if ui.button(t("Copiar tudo")).clicked(){
                        let text=self.image_info.iter().map(|(key,value)|format!("{}:\n{}",t(key),value)).collect::<Vec<_>>().join("\n\n");
                        match arboard::Clipboard::new().and_then(|mut clipboard|clipboard.set_text(text)){Ok(())=>self.toast(t("Metadados copiados.")),Err(e)=>self.fail(e.to_string())}
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(420.0).show(ui,|ui|{
                        let mut copied=None;
                        egui::Grid::new("image_info").striped(true).show(ui,|ui|{
                            for(key,value)in &self.image_info{
                                ui.horizontal(|ui|{
                                    if (key.starts_with("Geração por IA")||key=="EXIF · UserComment")&&icons::tool_button(ui,"Copiar",true){copied=Some(value.clone());}
                                    ui.add_sized([110.0,0.0],egui::Label::new(t(key)).wrap());
                                });
                                ui.add_sized([440.0,0.0],egui::Label::new(value).wrap().selectable(true));ui.end_row();
                            }
                        });
                        if let Some(text)=copied{match arboard::Clipboard::new().and_then(|mut clipboard|clipboard.set_text(text)){Ok(())=>self.toast(t("Conteúdo de IA copiado.")),Err(e)=>self.fail(e.to_string())}}

                    });
                },
                Dialog::SaveAs {format,quality}=>{
                    ui.set_width(430.0);
                    ui.label(t("Formato"));
                    ui.horizontal(|ui| {ui.radio_value(format,Format::Jpeg,"JPG");ui.radio_value(format,Format::Png,"PNG");ui.radio_value(format,Format::Webp,"WebP");});
                    ui.add_space(8.0);
                    if *format==Format::Png {ui.small(t("PNG preserva a imagem sem perdas; não usa qualidade percentual."));}
                    else {ui.add(egui::Slider::new(quality,1..=100).text(t("Qualidade")).suffix("%"));}
                    ui.separator();
                    if ui.button(t("Salvar")).clicked() {
                        if *format==Format::Jpeg {self.settings.jpeg_quality = *quality;}
                        if *format==Format::Webp {self.settings.webp_quality = *quality;}
                        self.export_dialog(*format);keep=false;
                    }
                    if let Some(path)=self.path.clone() {
                        if ui.button(t("Salvar no formato original")).clicked(){self.dialog=Some(Dialog::SaveOriginal(path));keep=false;}
                    }
                },
                Dialog::SaveOriginal(path)=>{
                    ui.set_width(440.0);ui.label(t("Substituir o arquivo atual pelas alterações?"));dialog_file(ui,path);
                    ui.small(t("O formato real será mantido. Formatos com perdas serão recomprimidos e os metadados serão removidos."));
                    if ui.button(t("Salvar")).clicked(){
                        if let Some(pixels)=self.image.clone(){let path=path.clone();let tx=self.tx.clone();let ctx=ctx.clone();let quality=if path.extension().is_some_and(|e|e.eq_ignore_ascii_case("webp")){self.settings.webp_quality}else{self.settings.jpeg_quality};self.busy=true;
                            std::thread::spawn(move||{let result=image_details::save_original(&pixels,&path,quality);let _=tx.send(Message::Saved(path,result));ctx.request_repaint();});}
                        keep=false;
                    }
                },
                Dialog::Settings=>{
                    ui.set_min_width(760.0);ui.set_max_width(760.0);
                    ui.horizontal_top(|ui| {
                        ui.set_max_height(400.0);
                        ui.allocate_ui_with_layout(Vec2::new(170.0,400.0),egui::Layout::top_down(egui::Align::Min),|ui|{
                            for (index,name) in ["Aparência","Visualização","Tela cheia / Apresentação","Exportação","Idioma"].iter().enumerate(){
                                ui.selectable_value(&mut self.settings_page,index,t(name));
                            }
                        });
                        ui.add_sized([1.0,400.0],egui::Separator::default().vertical());
                        ui.allocate_ui_with_layout(Vec2::new(560.0,400.0),egui::Layout::top_down(egui::Align::Min),|ui|{
                            egui::ScrollArea::vertical().max_height(400.0).show(ui,|ui|{
                                match self.settings_page {
                                    0=>{
                                        ui.heading(t("Aparência"));ui.separator();
                                        ui.checkbox(&mut self.settings.checkerboard,t("Fundo quadriculado para transparência"));
                                        ui.label(t("Transparência dos menus"));
                                        ui.add(egui::Slider::new(&mut self.settings.menu_transparency,0..=100).suffix("%"));
                                        ui.label(t("Tema"));
                                        for (theme,name) in [(Theme::Dark,"Escuro"),(Theme::Light,"Claro"),(Theme::Neon,"Neon + preto"),(Theme::System,"Sistema")]{if ui.radio_value(&mut self.settings.theme,theme,t(name)).clicked() && theme==Theme::Neon {self.settings.screen_color=[0,0,0];}}
                                        if self.settings.theme==Theme::Neon {ui.horizontal(|ui| {ui.label(t("Cor neon"));preferences::color_button(ui,&t("Escolher"),&mut self.settings.neon_color);});}
                                    }
                                    1=>{
                                        ui.heading(t("Visualização"));ui.separator();
                                        ui.checkbox(&mut self.settings.auto_restart,t("Reiniciar automaticamente ao chegar ao fim da pasta"));
                                        ui.separator();ui.label(t("Modo inicial"));
                                        for (label,fit) in FITS {ui.radio_value(&mut self.settings.fit,fit,t(label));}
                                    }
                                    2=>{
                                        ui.heading(t("Tela cheia / Apresentação"));ui.separator();ui.label(t("Modo de zoom (1–7)"));
                                        for (label,fit) in FITS {if ui.radio(self.wheel_fit==fit,t(label)).clicked(){self.set_fit(fit);}}
                                        ui.separator();
                                        ui.horizontal(|ui|{ui.label(t("Cor da tela"));preferences::color_button(ui,&t("Escolher"),&mut self.settings.screen_color);});
                                        ui.horizontal(|ui|{ui.label(t("Cor do título em tela cheia"));preferences::color_button(ui,&t("Escolher"),&mut self.settings.title_color);});
                                        ui.horizontal(|ui|{ui.label(t("Tamanho do texto"));ui.add(egui::DragValue::new(&mut self.settings.title_size).range(10.0..=36.0).suffix(" px"));});
                                        ui.horizontal(|ui|{ui.label(t("Posição do texto"));for (i,name) in ["Topo","Meio","Embaixo"].iter().enumerate(){ui.radio_value(&mut self.settings.title_position,i as u8,t(name));}});
                                        ui.horizontal(|ui|{ui.label(t("Alinhamento do texto"));for (i,name) in ["Esquerda","Centro","Direita"].iter().enumerate(){ui.radio_value(&mut self.settings.title_alignment,i as u8,t(name));}});
                                        ui.checkbox(&mut self.show_art_title,t("Mostrar título da arte (T, somente em tela cheia)"));
                                        ui.checkbox(&mut self.hide_cursor,t("Ocultar cursor em tela cheia (F11)"));
                                        ui.horizontal(|ui|{ui.label(t("Intervalo da apresentação"));ui.add(egui::DragValue::new(&mut self.settings.slide_seconds).range(1.0..=30.0).speed(0.5));ui.label(t("segundos"));});
                                    }
                                    3=>{
                                        ui.heading(t("Qualidade de exportação"));ui.separator();
                                        ui.label("JPEG");ui.add(egui::Slider::new(&mut self.settings.jpeg_quality,1..=100).suffix("%"));
                                        ui.label("WebP");ui.add(egui::Slider::new(&mut self.settings.webp_quality,1..=100).suffix("%"));
                                    }
                                    _=>{
                                        ui.heading(t("Idioma"));ui.separator();
                                        ui.radio_value(&mut self.settings.language,Language::Portuguese,"Português");
                                        ui.radio_value(&mut self.settings.language,Language::English,"English");
                                    }
                                }
                            });
                        });
                    });
                    ui.separator();ui.small(t("Preferências são salvas ao fechar o aplicativo."));
                },
                Dialog::Dependencies=>{
                    ui.label(t("JPEG/WebP são abertos diretamente. ImageMagick atende formatos adicionais e exportação sob demanda. jpegtran realiza recorte e rotação JPEG sem recompressão."));
                    ui.label(format!("magick: {}",backend::tool("magick").display()));
                    ui.label(format!("jpegtran: {}",backend::tool("jpegtran").display()));
                    ui.label(t("Linux: instale ImageMagick 7 com suporte AVIF/WebP e libjpeg-turbo. Variáveis LUME_MAGICK e LUME_JPEGTRAN permitem indicar os executáveis."));
                },
                Dialog::About=>{
 ui.horizontal(|ui|{ui.image((self.logo.id(),Vec2::splat(96.0)));ui.vertical(|ui|{
 ui.heading("Lume");ui.label(format!("{} · {} bits",env!("CARGO_PKG_VERSION"),usize::BITS));
 ui.label(t("Um visualizador de imagens simples e rápido."));
 ui.label(t("Este gatinho se chama Lume."));
 ui.label(t("Um pequeno companheiro para iluminar suas imagens. Miau!"));ui.label(t("Software open source · CC0 1.0"));
 });});ui.add_space(8.0);
 if ui.button(t("Dependências")).clicked(){self.dialog=Some(Dialog::Dependencies);keep=false;}
 }
                Dialog::Help=>{
                    egui::ScrollArea::vertical().max_height(500.0).show(ui,|ui|{
                        ui.label(format!("Lume {}",env!("CARGO_PKG_VERSION")));ui.separator();
                        for(k,v)in [
                            ("← / →","Imagem anterior / próxima; com seleção, mover recorte"),("↑ / ↓","Mover imagem verticalmente; com seleção, mover recorte"),
                            ("Espaço / Backspace","Próxima / anterior"),("Page Down / Page Up","Próxima / anterior"),("Ctrl + navegação","Pular 5 imagens"),
                            ("Ctrl + 0–9","Avançar N (0 = 10); Shift inverte"),("Ctrl+Home / Ctrl+End","Primeira / última imagem"),
                            ("+ ou = / −","Ampliar / reduzir"),("1–7","Modos de ajuste, conforme menu Exibir"),("Enter / F","Tela cheia / janela"),("T","Mostrar / ocultar título da arte (tela cheia)"),
                            ("Esc","Cancelar seleção, sair da tela cheia ou fechar"),("F2 / F6","Renomear, incluindo extensão"),("F5 / U","Recarregar imagem e pasta"),
                            ("F7 / F8","Mover / copiar arquivo"),("Del / Shift+Del","Lixeira / exclusão permanente, com confirmação"),("F11","Ocultar / mostrar cursor em tela cheia"),
                            ("I","Informações da imagem"),("P","Preferências"),("S","Exportar JPEG / WebP"),("Ctrl+O","Abrir imagem"),("Ctrl+S","Salvar imagem"),("Ctrl+Shift+S","Exportar WebP"),("Ctrl+A","Selecionar / desselecionar tudo"),
                            ("Ctrl+Y","Aplicar recorte selecionado"),("Ctrl+Shift+L","Iniciar recorte JPEG sem perdas"),("Ctrl + setas","Redimensionar seleção proporcionalmente"),
                            ("Shift + setas","Mover seleção em passos maiores"),("Alt + setas","Alterar largura / altura da seleção"),
                            ("Ctrl+Z / Ctrl+J","Desfazer / refazer edições"),("Ctrl+C / Ctrl+Insert","Copiar imagem ou seleção"),("Ctrl+V","Colar como nova imagem"),
                            ("Ctrl+H","Tamanho original"),("Ctrl+G","Escala de cinza"),("Ctrl+R","Redimensionar"),("Ctrl+F","Procurar na pasta"),
                            ("Ctrl+M","Imagem aleatória"),("Ctrl+N","Abrir em outra janela"),("Ctrl+W","Iniciar / parar apresentação"),
                            ("Pause (Windows)","Pausar apresentação; outra tecla continua"),
                        ]{ui.horizontal_wrapped(|ui|{icons::shortcut(ui,&t(k));ui.label(t(v));});}
                        ui.separator();ui.label(t("Edições comuns ficam em memória até exportar. Navegar descarta essas edições. Recorte JPEG sem perdas salva uma cópia com sufixo _crop, _crop1, _crop2, preservando o original. JPEG com orientação EXIF diferente de 1 usa recorte comum nesta versão."));
                        ui.label(t("Imagens animadas e multipáginas: apenas o primeiro quadro. Sem OCR, miniaturas, impressão, captura de tela, filtros 8BF ou edição IPTC nesta versão."));
                    });
                },
                Dialog::Search(query)=>{
                    ui.set_width(480.0);ui.add(egui::TextEdit::singleline(query).desired_width(f32::INFINITY).hint_text(t("Parte do nome do arquivo")));
                    let q=query.to_lowercase();let matches:Vec<_>=self.files.iter().enumerate().filter(|(_,p)|p.file_name().unwrap_or_default().to_string_lossy().to_lowercase().contains(&q)).map(|(n,p)|(n,p.file_name().unwrap_or_default().to_string_lossy().into_owned())).collect();
                    egui::ScrollArea::vertical().max_height(360.0).show(ui,|ui|{for(n,name)in matches{if ui.add_sized([ui.available_width(),30.0],egui::Button::new(name)).clicked(){self.navigate(n as isize-self.index as isize);keep=false;}}});
                },
                Dialog::Batch{files,dest,format,quality}=>{
                    ui.label(format!("{} {}",files.len(),t("imagens selecionadas")));
                    if ui.button(t("Selecionar imagens")).clicked(){if let Some(f)=rfd::FileDialog::new().pick_files(){*files=f;}}
                    if ui.button(t("Usar imagens da pasta atual")).clicked(){*files=self.files.clone();}
                    ui.label(format!("{}: {}",t("Destino"),dest.display()));
                    if ui.button(t("Escolher pasta de saída")).clicked(){if let Some(p)=rfd::FileDialog::new().pick_folder(){*dest=p;}}
                    ui.horizontal(|ui|{ui.radio_value(format,Format::Jpeg,"JPEG");ui.radio_value(format,Format::Webp,"WebP");ui.radio_value(format,Format::Png,"PNG");});
                    ui.add(egui::Slider::new(quality,1..=100).text(t("Qualidade")));
                    ui.label(t("Converte os arquivos originais. Nomes existentes recebem um sufixo numérico."));
                    if ui.add_enabled(!files.is_empty()&&dest.is_dir(),egui::Button::new("Converter")).clicked(){self.start_batch(files.clone(),dest.clone(),*format,*quality);keep=false;}
                },
            }
            if !matches!(dialog,Dialog::Delete { .. }|Dialog::FolderEnd { .. }|Dialog::Unsaved { .. }){
                ui.add_space(4.0);ui.separator();
                let label=if matches!(dialog,Dialog::Rename(..)|Dialog::Resize(..)|Dialog::ManualRatio{..}|Dialog::SaveOriginal(_)|Dialog::SaveAs{..}|Dialog::Overwrite(..)|Dialog::Batch{..}){"Cancelar"}else{"Fechar"};
                if ui.button(t(label)).clicked(){keep=false;}
            }
            ui.add_space(4.0);
        });
        if keep {
            self.dialog = Some(dialog);
        }
    }
}

fn dialog_header(ui: &mut egui::Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(t(title)).strong().size(18.0));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (rect, response) = ui.allocate_exact_size(
                Vec2::splat(24.0),
                Sense::click().difference(Sense::focusable_noninteractive()),
            );
            let color = if response.hovered() {
                Color32::WHITE
            } else {
                ui.visuals().text_color()
            };
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, 4.0, Color32::from_rgb(175, 55, 65));
            }
            let d = Vec2::splat(5.0);
            ui.painter().line_segment(
                [rect.center() - d, rect.center() + d],
                Stroke::new(1.5_f32, color),
            );
            ui.painter().line_segment(
                [
                    rect.center() + Vec2::new(-5.0, 5.0),
                    rect.center() + Vec2::new(5.0, -5.0),
                ],
                Stroke::new(1.5_f32, color),
            );
            close = response.clicked_by(egui::PointerButton::Primary);
            response.on_hover_text(t("Fechar"));
        });
    });
    ui.add_space(6.0);
    ui.separator();
    ui.add_space(6.0);
    close
}

fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}
fn tool_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    icons::tool_button(ui, label, enabled)
}

fn dialog_file(ui: &mut egui::Ui, path: &std::path::Path) {
    egui::Frame::group(ui.style())
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.set_width((ui.available_width() - 24.0).max(120.0));
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            ui.add(egui::Label::new(egui::RichText::new(name).strong()).wrap());
            if let Some(parent) = path.parent() {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(parent.display().to_string())
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    )
                    .wrap(),
                );
            }
        });
}

fn key_number(k: Key) -> Option<usize> {
    match k {
        Key::Num0 => Some(0),
        Key::Num1 => Some(1),
        Key::Num2 => Some(2),
        Key::Num3 => Some(3),
        Key::Num4 => Some(4),
        Key::Num5 => Some(5),
        Key::Num6 => Some(6),
        Key::Num7 => Some(7),
        Key::Num8 => Some(8),
        Key::Num9 => Some(9),
        _ => None,
    }
}
fn item(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let clicked = ui
        .add(egui::Button::new(t(label)).shortcut_text(shortcut))
        .clicked();
    if clicked {
        ui.close_menu();
    }
    clicked
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(windows)]
        {
            let down =
                unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(0x13) }
                    < 0;
            if down
                && !self.pause_down
                && self.slide
                && ctx.input(|i| i.viewport().focused.unwrap_or(false))
            {
                self.paused = !self.paused;
                self.last_slide = Instant::now();
            }
            self.pause_down = down;
        }
        i18n::set_language(self.settings.language);
        match self.settings.theme {
            Theme::Dark | Theme::Neon => ctx.set_theme(egui::ThemePreference::Dark),
            Theme::Light => ctx.set_theme(egui::ThemePreference::Light),
            Theme::System => ctx.set_theme(egui::ThemePreference::System),
        }
        let mut visuals = if ctx.theme() == egui::Theme::Light {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        };
        if self.settings.theme == Theme::Neon {
            let [r, g, b] = self.settings.neon_color;
            let neon = Color32::from_rgb(r, g, b);
            visuals.panel_fill = Color32::BLACK;
            visuals.window_fill = Color32::BLACK;
            visuals.extreme_bg_color = Color32::BLACK;
            visuals.window_stroke = Stroke::new(1.0_f32, neon);
            visuals.widgets.hovered.fg_stroke.color = neon;
            visuals.widgets.active.fg_stroke.color = neon;
            visuals.selection.bg_fill = neon.gamma_multiply(0.4);
            visuals.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
            visuals.hyperlink_color = neon;
            visuals.widgets.inactive.bg_fill = Color32::from_rgb(12, 20, 18);
            visuals.widgets.hovered.bg_fill = neon.gamma_multiply(0.25);
            visuals.widgets.active.bg_fill = neon.gamma_multiply(0.4);
        }
        ctx.set_visuals(visuals);
        if ctx.input(|i| i.viewport().close_requested()) && self.dirty {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !matches!(self.dialog, Some(Dialog::Unsaved { .. })) {
                self.confirm_abandon(PendingAction::Close);
            }
        }
        self.pixel_ratio = ctx.pixels_per_point();
        self.messages(ctx);
        self.shortcuts(ctx);
        if !self.fullscreen {
            self.menus(ctx);
            self.toolbar(ctx);
            self.status_bar(ctx);
        }
        self.canvas(ctx);
        self.dialogs(ctx);
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            if !self.busy {
                self.open(p);
            }
        }
        if self.slide
            && !self.paused
            && !self.busy
            && self.loading.is_none()
            && self.dialog.is_none()
            && self.error.is_none()
            && self.last_slide.elapsed().as_secs_f32() >= self.settings.slide_seconds
        {
            if self.index + 1 >= self.files.len() {
                self.index = 0;
                if let Some(p) = self.files.first().cloned() {
                    self.request_load(p);
                }
            } else {
                self.navigate(1);
            }
            self.last_slide = Instant::now();
        }
        ctx.set_cursor_icon(
            if self.fullscreen && self.hide_cursor && self.dialog.is_none() {
                egui::CursorIcon::None
            } else {
                egui::CursorIcon::Default
            },
        );
        let (w, h) = self.dimensions();
        let s = model::display_scale(
            self.fit,
            (w, h),
            (self.canvas.width(), self.canvas.height()),
            self.zoom,
            self.pixel_ratio,
        )
        .0;
        let filename = self.display_name();
        let title = if self.image.is_some() {
            format!(
                "{filename}{} · {w} × {h} · {:.0}%{} — Lume",
                if self.dirty { " *" } else { "" },
                s * self.pixel_ratio * 100.0,
                if self.files.is_empty() {
                    String::new()
                } else {
                    format!(" · {}/{}", self.index + 1, self.files.len())
                }
            )
        } else {
            "Lume".into()
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        if self.busy
            || self.loading.is_some()
            || self.slide
            || self
                .notice
                .as_ref()
                .map(|(_, t)| t.elapsed() < Duration::from_secs(4))
                .unwrap_or(false)
        {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(s) = serde_json::to_string(&self.settings) {
            storage.set_string("settings", s);
        }
    }
}

fn cat_icon() -> egui::IconData {
    let im = image::load_from_memory(include_bytes!("../assets/lume-cat.png"))
        .expect("Lume icon")
        .resize_exact(128, 128, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    egui::IconData {
        rgba: im.into_raw(),
        width: 128,
        height: 128,
    }
}
fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Lume",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(cat_icon())
                .with_inner_size([1100.0, 780.0])
                .with_min_inner_size([480.0, 320.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[cfg(test)]
mod settings_tests {
    use super::*;
    #[test]
    fn old_settings_keep_opaque_menus_and_transparency_persists() {
        let mut settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.menu_transparency, 0);
        settings.menu_transparency = 100;
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: Settings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.menu_transparency, 100);
    }
}
