//! Original-pixel tiles are prepared lazily, off the UI thread. Small fit previews remain cached.
use eframe::egui::{self, Pos2, Rect, Vec2};
use image::RgbaImage;
use std::{
    collections::{HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
};
const SIDE: u32 = 512;
const CACHE: usize = 64;
#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct Key {
    generation: u64,
    x: u32,
    y: u32,
    lod: u32,
}
struct Job {
    key: Key,
    image: Arc<RgbaImage>,
    ctx: egui::Context,
}
struct Ready {
    key: Key,
    image: egui::ColorImage,
}
#[derive(Default)]
pub struct Renderer {
    generation: Arc<AtomicU64>,
    worker: Option<mpsc::Sender<Job>>,
    ready: Option<mpsc::Receiver<Ready>>,
    pending: HashSet<Key>,
    cache: VecDeque<(Key, egui::TextureHandle)>,
}
fn crop_tile(im: &RgbaImage, x: u32, y: u32, lod: u32) -> egui::ColorImage {
    let side = SIDE * lod;
    let width = side.min(im.width() - x);
    let height = side.min(im.height() - y);
    let tile = image::imageops::crop_imm(im, x, y, width, height).to_image();
    let tile = if lod > 1 {
        image::imageops::resize(
            &tile,
            width.div_ceil(lod),
            height.div_ceil(lod),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        tile
    };
    egui::ColorImage::from_rgba_unmultiplied(
        [tile.width() as usize, tile.height() as usize],
        tile.as_raw(),
    )
}
impl Renderer {
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.pending.clear();
        self.cache.clear();
    }
    fn start(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel::<Job>();
        let (out, ready) = mpsc::channel();
        let generation = self.generation.clone();
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                if generation.load(Ordering::Relaxed) != job.key.generation {
                    continue;
                }
                let image = crop_tile(&job.image, job.key.x, job.key.y, job.key.lod);
                if generation.load(Ordering::Relaxed) == job.key.generation {
                    if out
                        .send(Ready {
                            key: job.key,
                            image,
                        })
                        .is_err()
                    {
                        break;
                    }
                    job.ctx.request_repaint();
                }
            }
        });
        self.worker = Some(tx);
        self.ready = Some(ready);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ctx: &egui::Context,
        painter: &egui::Painter,
        image: Arc<RgbaImage>,
        shown: Rect,
        view: Rect,
        preview: egui::TextureId,
        rotation: crate::view_rotation::Rotation,
    ) {
        let view=rotation.inverse().rect(view,shown.center());
        self.start();
        let generation = self.generation.load(Ordering::Relaxed);
        if let Some(rx) = &self.ready {
            while let Ok(ready) = rx.try_recv() {
                if ready.key.generation != generation {
                    continue;
                }
                self.pending.remove(&ready.key);
                let handle = ctx.load_texture(
                    "original_tile",
                    ready.image,
                    if ready.key.lod == 1
                        && shown.width() / image.width() as f32 * ctx.pixels_per_point() >= 1.
                    {
                        egui::TextureOptions::NEAREST
                    } else {
                        egui::TextureOptions::LINEAR
                    },
                );
                self.cache.push_back((ready.key, handle));
                while self.cache.len() > CACHE {
                    self.cache.pop_front();
                }
            }
        }
        let sx = shown.width() / image.width() as f32;
        let sy = shown.height() / image.height() as f32;
        let physical = sx.max(sy) * ctx.pixels_per_point();
        let lod = if physical >= 1. {
            1
        } else {
            (1.0 / physical.max(0.001)).log2().floor().clamp(0., 7.) as u32
        };
        let lod = if physical >= 1. { 1 } else { 1 << lod };
        let side = SIDE * lod;
        let visible = shown.intersect(view);
        if !visible.is_positive() {
            return;
        }
        let left = (((visible.min.x - shown.min.x) / sx).max(0.) as u32 / side) * side;
        let top = (((visible.min.y - shown.min.y) / sy).max(0.) as u32 / side) * side;
        let right = (((visible.max.x - shown.min.x) / sx).ceil() as u32).min(image.width());
        let bottom = (((visible.max.y - shown.min.y) / sy).ceil() as u32).min(image.height());
        for y in (top..bottom).step_by(side as usize) {
            for x in (left..right).step_by(side as usize) {
                let key = Key {
                    generation,
                    x,
                    y,
                    lod,
                };
                let size = Vec2::new(
                    side.min(image.width() - x) as f32 * sx,
                    side.min(image.height() - y) as f32 * sy,
                );
                let dest =
                    Rect::from_min_size(shown.min + Vec2::new(x as f32 * sx, y as f32 * sy), size);
                if let Some(index) = self.cache.iter().position(|(k, _)| *k == key) {
                    let item = self.cache.remove(index).unwrap();
                    let size = Vec2::new(
                        side.min(image.width() - x) as f32 * sx,
                        side.min(image.height() - y) as f32 * sy,
                    );
                    let dest = Rect::from_min_size(
                        shown.min + Vec2::new(x as f32 * sx, y as f32 * sy),
                        size,
                    );
                    rotation.paint(painter,
                        item.1.id(),
                        dest,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        shown.center(),
                    );
                    self.cache.push_back(item);
                } else {
                    let uv = Rect::from_min_max(
                        Pos2::new(
                            x as f32 / image.width() as f32,
                            y as f32 / image.height() as f32,
                        ),
                        Pos2::new(
                            (x + side).min(image.width()) as f32 / image.width() as f32,
                            (y + side).min(image.height()) as f32 / image.height() as f32,
                        ),
                    );
                    rotation.paint(painter,preview,dest,uv,shown.center());
                    if self.pending.len() < CACHE && self.pending.insert(key) {
                        let _ = self.worker.as_ref().unwrap().send(Job {
                            key,
                            image: image.clone(),
                            ctx: ctx.clone(),
                        });
                    }
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tall_image_original_tile_preserves_every_pixel() {
        let mut image = RgbaImage::new(97, 20000);
        for (y, row) in image.rows_mut().enumerate() {
            for (x, p) in row.enumerate() {
                *p = image::Rgba([x as u8, y as u8, 123, 255]);
            }
        }
        let tile = crop_tile(&image, 0, 16384, 1);
        assert_eq!(tile.size, [97, 512]);
        for y in 0..512 {
            for x in 0..97 {
                assert_eq!(
                    tile.pixels[y * 97 + x].to_array(),
                    image.get_pixel(x as u32, 16384 + y as u32).0
                );
            }
        }
        let last = crop_tile(&image, 0, 19968, 1);
        assert_eq!(last.size, [97, 32]);
    }
    #[test]
    fn lod_tiles_stay_within_texture_limits() {
        let image = RgbaImage::new(1600, 2000);
        let tile = crop_tile(&image, 0, 0, 4);
        assert_eq!(tile.size, [400, 500]);
    }
}
