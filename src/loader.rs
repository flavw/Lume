//! Foreground decoding and speculative decoding use separate workers.
//! Navigation supersedes pending requests; stale results never replace the selected file.
use crate::backend::{self, Loaded};
use eframe::egui;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    time::SystemTime,
};

const BUDGET: usize = 256 * 1024 * 1024;
static SERIAL: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
    revision: u64,
    limit: u32,
}
impl Key {
    fn new(path: PathBuf, revision: u64, limit: u32) -> Self {
        let meta = crate::comic::metadata(&path).or_else(|| std::fs::metadata(&path).ok());
        Self {
            path,
            len: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            modified: meta.and_then(|m| m.modified().ok()),
            revision,
            limit,
        }
    }
}

pub struct Prepared {
    pub serial: u64,
    pub file_bytes: Option<u64>,
    pub created: Option<SystemTime>,
    pub has_alpha: bool,
    pub loaded: Loaded,
    pub display: Arc<egui::ColorImage>,
}
impl Prepared {
    pub fn new(loaded: Loaded, limit: u32) -> Self {
        Self::prepare_if(loaded, limit, || true).unwrap()
    }
    fn prepare_if(loaded: Loaded, limit: u32, mut current: impl FnMut() -> bool) -> Option<Self> {
        if !current() {
            return None;
        }
        let pixels = &loaded.pixels;
        let small = if pixels.width() > limit || pixels.height() > limit {
            Some(image::imageops::thumbnail(&**pixels, limit, limit))
        } else {
            None
        };
        if !current() {
            return None;
        }
        let display = small.as_ref().unwrap_or(pixels);
        let display = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
            [display.width() as usize, display.height() as usize],
            display.as_raw(),
        ));
        if !current() {
            return None;
        }
        let has_alpha = pixels.pixels().any(|p| p[3] < 255);
        if !current() {
            return None;
        }
        Some(Self {
            serial: SERIAL.fetch_add(1, Ordering::Relaxed),
            file_bytes: None,
            created: None,
            has_alpha,
            loaded,
            display,
        })
    }
    fn bytes(&self) -> usize {
        self.loaded.pixels.as_raw().len() + self.display.pixels.len() * 4
    }
}
pub struct Ready {
    pub id: u64,
    pub path: PathBuf,
    pub result: backend::Result<Arc<Prepared>>,
}
#[derive(Clone)]
struct Job {
    id: u64,
    key: Key,
}
#[derive(Default)]
struct State {
    revision: u64,
    next_id: u64,
    target: Option<Job>,
    foreground: Option<Job>,
    prefetch: VecDeque<Job>,
    speculative_keys: Vec<Key>,
    cache: VecDeque<(Key, Arc<Prepared>)>,
    stopped: bool,
}
impl State {
    fn needs(&self, key: &Key, foreground: bool) -> bool {
        !self.stopped
            && self.revision == key.revision
            && (self.target.as_ref().is_some_and(|j| &j.key == key)
                || (!foreground && self.speculative_keys.contains(key)))
    }
    fn cached(&mut self, key: &Key) -> Option<Arc<Prepared>> {
        let n = self.cache.iter().position(|(k, _)| k == key)?;
        let entry = self.cache.remove(n)?;
        let value = entry.1.clone();
        self.cache.push_back(entry);
        Some(value)
    }
    fn insert(&mut self, key: Key, value: Arc<Prepared>) {
        self.cache.retain(|(k, _)| k != &key);
        self.cache.push_back((key, value));
        while self.cache.len() > 1
            && (self.cache.len() > 8
                || self.cache.iter().map(|(_, v)| v.bytes()).sum::<usize>() > BUDGET)
        {
            let remove = self
                .cache
                .iter()
                .position(|(k, _)| self.target.as_ref().map(|j| &j.key != k).unwrap_or(true))
                .unwrap_or(0);
            self.cache.remove(remove);
        }
    }
}
pub struct Loader {
    shared: Arc<(Mutex<State>, Condvar)>,
    pub ready: mpsc::Receiver<Ready>,
    ctx: egui::Context,
    limit: u32,
}
impl Loader {
    pub fn new(ctx: egui::Context) -> Self {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let (tx, ready) = mpsc::channel();
        for foreground in [true, false] {
            let shared = shared.clone();
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let (mut state, cv) = (shared.0.lock().unwrap(), &shared.1);
                    loop {
                        if state.stopped {
                            return;
                        }
                        let next = if foreground {
                            state.foreground.take()
                        } else {
                            state.prefetch.pop_front()
                        };
                        if let Some(job) = next {
                            if let Some(value) = state.cached(&job.key) {
                                if let Some(target) = &state.target {
                                    if target.key == job.key {
                                        let _ = tx.send(Ready {
                                            id: target.id,
                                            path: job.key.path.clone(),
                                            result: Ok(value),
                                        });
                                        ctx.request_repaint();
                                    }
                                }
                                continue;
                            }
                            break job;
                        }
                        state = cv.wait(state).unwrap();
                    }
                };
                let decoded = backend::load(&job.key.path);
                if !shared.0.lock().unwrap().needs(&job.key, foreground) {
                    continue;
                }
                let result = match decoded {
                    Ok(im) => {
                        let Some(mut prepared) = Prepared::prepare_if(im, job.key.limit, || {
                            shared.0.lock().unwrap().needs(&job.key, foreground)
                        }) else {
                            continue;
                        };
                        prepared.file_bytes = Some(job.key.len);
                        prepared.created = crate::comic::metadata(&job.key.path)
                            .or_else(|| std::fs::metadata(&job.key.path).ok())
                            .and_then(|m| m.created().ok());
                        Ok(Arc::new(prepared))
                    }
                    Err(error) => Err(error),
                };
                let mut state = shared.0.lock().unwrap();
                // Refresh/rename/delete invalidate even an in-flight decode.
                if !state.needs(&job.key, foreground) {
                    continue;
                }
                if let Ok(im) = &result {
                    state.insert(job.key.clone(), im.clone());
                }
                if let Some(target) = &state.target {
                    if target.key == job.key {
                        let _ = tx.send(Ready {
                            id: target.id,
                            path: job.key.path,
                            result,
                        });
                        ctx.request_repaint();
                    }
                }
            });
        }
        let limit = ctx.input(|i| i.max_texture_side) as u32;
        Self {
            shared,
            ready,
            ctx,
            limit,
        }
    }
    pub fn request(&mut self, path: PathBuf) -> (u64, Option<Arc<Prepared>>) {
        let mut state = self.shared.0.lock().unwrap();
        state.next_id += 1;
        let id = state.next_id;
        let key = Key::new(path, state.revision, self.limit);
        let job = Job {
            id,
            key: key.clone(),
        };
        state.target = Some(job.clone());
        state.prefetch.clear();
        state.speculative_keys.clear();
        let cached = state.cached(&key);
        state.foreground = if cached.is_none() { Some(job) } else { None };
        self.shared.1.notify_all();
        (id, cached)
    }
    pub fn prefetch(&mut self, paths: Vec<PathBuf>) {
        let mut state = self.shared.0.lock().unwrap();
        state.prefetch.clear();
        state.speculative_keys.clear();
        for path in paths {
            let key = Key::new(path, state.revision, self.limit);
            if state.cache.iter().any(|(k, _)| k == &key)
                || state.target.as_ref().map(|j| j.key == key).unwrap_or(false)
            {
                continue;
            }
            state.speculative_keys.push(key.clone());
            state.prefetch.push_back(Job { id: 0, key });
        }
        self.shared.1.notify_all();
    }
    pub fn invalidate(&mut self) {
        let mut state = self.shared.0.lock().unwrap();
        state.revision += 1;
        state.cache.clear();
        state.prefetch.clear();
        state.foreground = None;
        state.target = None;
    }
    pub fn cancel(&mut self) {
        let mut state = self.shared.0.lock().unwrap();
        state.target = None;
        state.foreground = None;
        state.prefetch.clear();
    }
    pub fn context(&self) -> egui::Context {
        self.ctx.clone()
    }
    pub fn prepare(&self, loaded: Loaded) -> Arc<Prepared> {
        Arc::new(Prepared::new(loaded, self.limit))
    }
}
impl Drop for Loader {
    fn drop(&mut self) {
        self.shared.0.lock().unwrap().stopped = true;
        self.shared.1.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn image() -> Arc<Prepared> {
        Arc::new(Prepared::new(
            Loaded {
                pixels: Arc::new(image::RgbaImage::from_pixel(
                    4,
                    4,
                    image::Rgba([10, 20, 30, 255]),
                )),
                jpeg: None,
            },
            4096,
        ))
    }
    #[test]
    fn obsolete_jobs_skip_preparation() {
        let a = Key::new(Path::new("a.png").into(), 0, 4096);
        let b = Key::new(Path::new("b.png").into(), 0, 4096);
        let mut state = State {
            target: Some(Job {
                id: 2,
                key: b.clone(),
            }),
            ..Default::default()
        };
        assert!(!state.needs(&a, true));
        assert!(state.needs(&b, true));
        state.speculative_keys.push(a.clone());
        assert!(state.needs(&a, false));
        state.speculative_keys.clear();
        assert!(!state.needs(&a, false));
        let loaded = image().loaded.clone();
        assert!(Prepared::prepare_if(loaded, 4096, || false).is_none());
    }
    #[test]
    fn cache_shares_pixels_and_checks_file_version() {
        let mut state = State::default();
        let key = Key::new(Path::new("a.jpg").into(), 0, 4096);
        let value = image();
        state.insert(key.clone(), value.clone());
        assert!(Arc::ptr_eq(&state.cached(&key).unwrap(), &value));
        let mut changed = key.clone();
        changed.len += 1;
        assert!(state.cached(&changed).is_none());
        let mut refreshed = key;
        refreshed.revision += 1;
        assert!(state.cached(&refreshed).is_none());
    }
    #[test]
    fn latest_request_wins_and_refresh_invalidates_cache() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        image::RgbaImage::from_pixel(20, 10, image::Rgba([255, 0, 0, 255]))
            .save(&a)
            .unwrap();
        image::RgbaImage::from_pixel(10, 20, image::Rgba([0, 0, 255, 255]))
            .save(&b)
            .unwrap();
        let mut loader = Loader::new(egui::Context::default());
        let (_, first) = loader.request(a);
        assert!(first.is_none());
        let (latest, cached) = loader.request(b.clone());
        assert!(cached.is_none());
        let result = loop {
            let ready = loader
                .ready
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            if ready.id == latest {
                break ready;
            }
        };
        assert_eq!(result.path, b);
        let prepared = result.result.unwrap();
        assert_eq!(prepared.loaded.pixels.dimensions(), (10, 20));
        let (_, hit) = loader.request(b.clone());
        assert!(Arc::ptr_eq(&prepared, &hit.unwrap()));
        loader.invalidate();
        assert!(loader.request(b).1.is_none());
    }
}
