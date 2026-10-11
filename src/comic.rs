use crate::{backend, model};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock, Weak},
    time::SystemTime,
};
static BOOKS: OnceLock<Mutex<Vec<Weak<Book>>>> = OnceLock::new();
const MAX_PAGE: u64 = 256 * 1024 * 1024;
pub struct Book {
    pub source: PathBuf,
    pub pages: Vec<PathBuf>,
    pub names: Vec<String>,
    temp: tempfile::TempDir,
    locks: Vec<Mutex<()>>,
    indices: Vec<usize>,
    stamp: (u64, Option<SystemTime>),
}
pub fn supported(p: &Path) -> bool {
    p.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("cbz"))
}
fn stamp(p: &Path) -> (u64, Option<SystemTime>) {
    std::fs::metadata(p)
        .map(|m| (m.len(), m.modified().ok()))
        .unwrap_or_default()
}
fn find(p: &Path) -> Option<(Arc<Book>, usize)> {
    let mut books = BOOKS.get_or_init(Default::default).lock().ok()?;
    books.retain(|b| b.strong_count() > 0);
    books
        .iter()
        .filter_map(Weak::upgrade)
        .find_map(|b| b.pages.iter().position(|x| x == p).map(|i| (b, i)))
}
pub fn metadata(p: &Path) -> Option<std::fs::Metadata> {
    let (b, _) = find(p)?;
    std::fs::metadata(&b.source).ok()
}
impl Book {
    pub fn open(source: PathBuf) -> backend::Result<Arc<Self>> {
        let file = std::fs::File::open(&source).map_err(|e| e.to_string())?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|e| format!("Arquivo ZIP inválido: {e}"))?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|e| format!("Não foi possível ler o quadrinho: {e}"))?;
            if !entry.is_dir()
                && entry.size() > 0
                && entry.size() <= MAX_PAGE
                && model::supported(Path::new(entry.name()))
            {
                entries.push((entry.name().to_owned(), index));
            }
        }
        entries.sort_by_cached_key(|(name, _)| model::natural_key(name));
        let (names, indices): (Vec<String>, Vec<usize>) = entries.into_iter().unzip();
        if names.is_empty() {
            return Err("O quadrinho não contém imagens compatíveis.".into());
        }
        let temp = tempfile::Builder::new()
            .prefix("lume-comic-")
            .tempdir()
            .map_err(|e| e.to_string())?;
        let pages = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                temp.path().join(format!(
                    "{i:08}.{}",
                    Path::new(n)
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                ))
            })
            .collect();
        let locks = names.iter().map(|_| Mutex::new(())).collect();
        let stamp = stamp(&source);
        let book = Arc::new(Self {
            source,
            pages,
            names,
            temp,
            locks,
            indices,
            stamp,
        });
        BOOKS
            .get_or_init(Default::default)
            .lock()
            .map_err(|e| e.to_string())?
            .push(Arc::downgrade(&book));
        Ok(book)
    }
    fn materialize(&self, i: usize) -> backend::Result<()> {
        let _guard = self.locks[i].lock().map_err(|e| e.to_string())?;
        if stamp(&self.source) != self.stamp {
            return Err("O quadrinho mudou no disco. Use F5 para reabrir.".into());
        }
        if self.pages[i].is_file() {
            return Ok(());
        }
        let file = std::fs::File::open(&self.source).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let entry = archive
            .by_index(self.indices[i])
            .map_err(|e| e.to_string())?;
        let mut temp =
            tempfile::NamedTempFile::new_in(self.temp.path()).map_err(|e| e.to_string())?;
        let count =
            std::io::copy(&mut entry.take(MAX_PAGE + 1), &mut temp).map_err(|e| e.to_string())?;
        if count > MAX_PAGE {
            return Err("Página grande demais.".into());
        }
        temp.flush().map_err(|e| e.to_string())?;
        temp.persist(&self.pages[i]).map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn load(p: &Path) -> Option<backend::Result<backend::Loaded>> {
    let (b, i) = find(p)?;
    Some(b.materialize(i).and_then(|_| backend::load_disk(p)))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zip_pages_are_lazy_cached_and_naturally_sorted() {
        for name in ["sample.cbz"] {
            let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures")
                .join(name);
            let book = Book::open(source).unwrap();
            assert_eq!(
                book.names
                    .iter()
                    .map(|n| n.replace('\\', "/"))
                    .collect::<Vec<_>>(),
                ["nested/page2.png", "nested/page10.png"]
            );
            assert!(!book.pages[0].exists());
            let before = metadata(&book.pages[0]).unwrap().len();
            let im = load(&book.pages[0]).unwrap().unwrap();
            assert_eq!(im.pixels.dimensions(), (40, 60));
            assert_eq!(im.pixels.get_pixel(0, 0).0, [255, 0, 0, 255]);
            assert_eq!(metadata(&book.pages[0]).unwrap().len(), before);
            assert!(book.pages[0].exists());
            assert!(!book.pages[1].exists());
            assert_eq!(
                load(&book.pages[1])
                    .unwrap()
                    .unwrap()
                    .pixels
                    .get_pixel(0, 0)
                    .0,
                [0, 0, 255, 255]
            );
            let temp = book.temp.path().to_owned();
            drop(book);
            assert!(!temp.exists());
        }
    }
    #[test]
    fn non_zip_content_is_rejected() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"not a ZIP archive").unwrap();
        assert!(Book::open(file.path().to_owned()).is_err());
    }
}
