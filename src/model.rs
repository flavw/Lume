use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Crop {
    pub fn resize_side(self, side: u8, delta: i64, iw: u32, ih: u32, ratio: Option<f32>) -> Self {
        let mut c = self;
        let horizontal = side == 6 || side == 7;
        if horizontal {
            let right = self.x + self.w;
            if side == 6 {
                c.x = (self.x as i64 + delta).clamp(0, right as i64 - 1) as u32;
                c.w = right - c.x;
            } else {
                c.w = (self.w as i64 + delta).clamp(1, (iw - self.x) as i64) as u32;
            }
            if let Some(r) = ratio {
                let cy = self.y as f64 + self.h as f64 / 2.0;
                let maxh = (2.0 * cy.min(ih as f64 - cy)).floor().max(1.0) as u32;
                c.w = c.w.min((maxh as f64 * r as f64).floor().max(1.0) as u32);
                c.h = (c.w as f64 / r as f64).round().max(1.0).min(maxh as f64) as u32;
                c.y = (cy - c.h as f64 / 2.0).round().max(0.0) as u32;
                if side == 6 {
                    c.x = right - c.w;
                }
            }
        } else {
            let bottom = self.y + self.h;
            if side == 8 {
                c.y = (self.y as i64 + delta).clamp(0, bottom as i64 - 1) as u32;
                c.h = bottom - c.y;
            } else {
                c.h = (self.h as i64 + delta).clamp(1, (ih - self.y) as i64) as u32;
            }
            if let Some(r) = ratio {
                let cx = self.x as f64 + self.w as f64 / 2.0;
                let maxw = (2.0 * cx.min(iw as f64 - cx)).floor().max(1.0) as u32;
                c.h = c.h.min((maxw as f64 / r as f64).floor().max(1.0) as u32);
                c.w = (c.h as f64 * r as f64).round().max(1.0).min(maxw as f64) as u32;
                c.x = (cx - c.w as f64 / 2.0).round().max(0.0) as u32;
                if side == 8 {
                    c.y = bottom - c.h;
                }
            }
        }
        c.bounded(iw, ih)
    }
    pub fn bounded(self, w: u32, h: u32) -> Self {
        let x = self.x.min(w.saturating_sub(1));
        let y = self.y.min(h.saturating_sub(1));
        Self {
            x,
            y,
            w: self.w.max(1).min(w - x),
            h: self.h.max(1).min(h - y),
        }
    }
    pub fn aligned(self, iw: u32, ih: u32, mcu: (u32, u32)) -> Self {
        let r = self.bounded(iw, ih);
        let x = r.x / mcu.0 * mcu.0;
        let y = r.y / mcu.1 * mcu.1;
        // Keep the exact right/bottom edges. Only the origin must align.
        Self {
            x,
            y,
            w: r.x + r.w - x,
            h: r.y + r.h - y,
        }
    }
    pub fn centered(w: u32, h: u32, ratio: Option<f32>) -> Self {
        let (cw, ch) = if let Some(r) = ratio {
            if w as f32 / h as f32 > r {
                ((h as f32 * r).round() as u32, h)
            } else {
                (w, (w as f32 / r).round() as u32)
            }
        } else {
            (w * 3 / 4, h * 3 / 4)
        };
        Self {
            x: (w - cw.min(w)) / 2,
            y: (h - ch.min(h)) / 2,
            w: cw,
            h: ch,
        }
        .bounded(w, h)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fit {
    Original,
    Large,
    All,
    Stretch,
    Width,
    Height,
    SmallSide,
    Custom,
}

#[derive(Debug, PartialEq)]
pub enum WheelAction {
    Zoom,
    Pan,
    Navigate,
}
/// Modifiers are evaluated for each wheel event; manual zoom does not change the base policy.
pub fn wheel_action(base: Fit, ctrl: bool, alt: bool) -> WheelAction {
    if ctrl || alt {
        WheelAction::Zoom
    } else if matches!(base, Fit::Original | Fit::Width) {
        WheelAction::Pan
    } else {
        WheelAction::Navigate
    }
}

pub fn scale(mode: Fit, image: (u32, u32), view: (f32, f32), zoom: f32) -> (f32, f32) {
    let sx = view.0 / image.0.max(1) as f32;
    let sy = view.1 / image.1.max(1) as f32;
    let fit = sx.min(sy);
    let s = match mode {
        Fit::Original => 1.0,
        Fit::Large => fit.min(1.0),
        Fit::All => fit,
        Fit::Width => sx,
        Fit::Height => sy,
        Fit::SmallSide => {
            if image.0 < image.1 {
                sx
            } else {
                sy
            }
        }
        Fit::Custom => zoom,
        Fit::Stretch => return (sx, sy),
    };
    (s, s)
}

pub fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|s| {
            matches!(
                s.to_lowercase().as_str(),
                "jpg"
                    | "jpeg"
                    | "jfif"
                    | "png"
                    | "webp"
                    | "avif"
                    | "gif"
                    | "bmp"
                    | "tif"
                    | "tiff"
                    | "ico"
                    | "heic"
                    | "heif"
            )
        })
        .unwrap_or(false)
}

pub fn shift_delete_cut(shift: bool, control: bool, alt: bool) -> bool {
    shift && !control && !alt
}

pub fn at_folder_end(index: usize, delta: isize, len: usize) -> bool {
    len > 0 && index == len - 1 && delta > 0
}

pub fn next_index(index: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        (index as isize + delta).rem_euclid(len as isize) as usize
    }
}

pub fn folder_files(path: &Path) -> Vec<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(path.parent().unwrap_or(Path::new(".")))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && supported(p))
        .collect();
    // Natural order: photo2 precedes photo10.
    files.sort_by_cached_key(|p| natural_key(&p.file_name().unwrap_or_default().to_string_lossy()));
    if !files.iter().any(|p| p == path) && path.is_file() {
        files.push(path.to_owned());
    }
    files
}

pub(crate) fn natural_key(s: &str) -> String {
    let mut out = String::new();
    let mut digits = String::new();
    for c in s.to_lowercase().chars().chain(std::iter::once('\0')) {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            if !digits.is_empty() {
                let n = digits.trim_start_matches('0');
                out.push_str(&format!("\u{1}{:08}:{}:{}", n.len(), n, digits.len()));
                digits.clear();
            }
            out.push(c);
        }
    }
    out
}

pub fn validate_filename(name: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
    {
        return Err("Use um nome de arquivo válido, sem caminhos ou caracteres reservados.".into());
    }
    let base = name.split('.').next().unwrap_or("").to_uppercase();
    if matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
    {
        return Err("Este nome é reservado pelo Windows.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_shift_delete_cut_is_permanent_delete_only() {
        assert!(shift_delete_cut(true, false, false));
        assert!(!shift_delete_cut(false, true, false));
        assert!(!shift_delete_cut(true, true, false));
        assert!(!shift_delete_cut(true, false, true));
    }
    #[test]
    fn side_handles_resize_free_and_preserve_ratio() {
        let c = Crop {
            x: 100,
            y: 100,
            w: 200,
            h: 100,
        };
        let free = c.resize_side(6, -50, 600, 400, None);
        assert_eq!(
            free,
            Crop {
                x: 50,
                y: 100,
                w: 250,
                h: 100
            }
        );
        for side in 6..=9 {
            for delta in [-500, -40, 40, 500] {
                let out = c.resize_side(side, delta, 600, 400, Some(2.0));
                assert!(out.x + out.w <= 600 && out.y + out.h <= 400);
                assert!((out.w as i64 - 2 * out.h as i64).abs() <= 1);
                if side == 6 {
                    assert_eq!(out.x + out.w, c.x + c.w);
                }
                if side == 8 {
                    assert_eq!(out.y + out.h, c.y + c.h);
                }
            }
        }
    }
    #[test]
    fn wheel_zoom_requires_current_modifier_and_release_restores_base_policy() {
        for mode in [
            Fit::Original,
            Fit::Large,
            Fit::All,
            Fit::Stretch,
            Fit::Width,
            Fit::Height,
            Fit::SmallSide,
        ] {
            let plain = if matches!(mode, Fit::Original | Fit::Width) {
                WheelAction::Pan
            } else {
                WheelAction::Navigate
            };
            assert_eq!(wheel_action(mode, false, false), plain);
            assert_eq!(wheel_action(mode, true, false), WheelAction::Zoom);
            assert_eq!(wheel_action(mode, false, false), plain);
            assert_eq!(wheel_action(mode, false, true), WheelAction::Zoom);
            assert_eq!(wheel_action(mode, false, false), plain);
        }
    }
    #[test]
    fn navigation_wraps_both_ends_and_large_jumps() {
        assert!(at_folder_end(2, 1, 3));
        assert!(at_folder_end(0, 1, 1));
        assert!(!at_folder_end(0, 1, 3));
        assert!(!at_folder_end(2, -1, 3));
        assert!(!at_folder_end(0, 1, 0));
        assert_eq!(next_index(2, 1, 3), 0);
        assert_eq!(next_index(0, -1, 3), 2);
        assert_eq!(next_index(1, 5, 3), 0);
        assert_eq!(next_index(0, 1, 1), 0);
        assert_eq!(next_index(0, 1, 0), 0);
    }
    #[test]
    fn crop_keeps_bounds_and_lossless_edges() {
        let c = Crop {
            x: 19,
            y: 23,
            w: 90,
            h: 80,
        }
        .aligned(100, 100, (16, 16));
        assert_eq!(
            c,
            Crop {
                x: 16,
                y: 16,
                w: 84,
                h: 84
            }
        );
        assert_eq!(
            Crop::centered(1920, 1080, Some(9.0 / 16.0)),
            Crop {
                x: 656,
                y: 0,
                w: 608,
                h: 1080
            }
        );
    }
    #[test]
    fn fit_modes_and_filename_safety() {
        assert_eq!(
            scale(Fit::Large, (100, 50), (500.0, 500.0), 1.0),
            (1.0, 1.0)
        );
        assert_eq!(scale(Fit::All, (100, 50), (500.0, 500.0), 1.0), (5.0, 5.0));
        for n in ["../x.jpg", "CON.jpg", "LPT1.png", "bad."] {
            assert!(validate_filename(n).is_err());
        }
        assert!(validate_filename("foto nova.avif").is_ok());
        assert!(natural_key("foto2.jpg") < natural_key("foto10.jpg"));
    }
}

/// Display coordinates are logical points; zoom percentages refer to physical pixels.
pub fn display_scale(
    mode: Fit,
    image: (u32, u32),
    view: (f32, f32),
    zoom: f32,
    pixels_per_point: f32,
) -> (f32, f32) {
    let density = pixels_per_point.max(0.1);
    let (sx, sy) = scale(mode, image, (view.0 * density, view.1 * density), zoom);
    (sx / density, sy / density)
}
#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn original_size_is_one_physical_pixel_at_any_dpi() {
        for density in [1., 1.25, 1.5, 2.] {
            let (sx, sy) = display_scale(Fit::Original, (1000, 30000), (900., 700.), 1., density);
            assert!((sx * density - 1.).abs() < 0.0001);
            assert!((sy * density - 1.).abs() < 0.0001);
        }
    }
}
