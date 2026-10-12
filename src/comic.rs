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
    ico_entries: Option<Vec<(usize,usize,[u8;16])>>,
}
pub fn supported(p: &Path) -> bool {
    p.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("cbz") || s.eq_ignore_ascii_case("ico"))
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
        if source.extension().is_some_and(|e|e.eq_ignore_ascii_case("ico")) {return Self::open_ico(source);}
        let file = std::fs::File::open(&source).map_err(|e| e.to_string())?;
        let mut archive =
            zip::ZipArchive::new(file).map_err(|_| "Não foi possível abrir o CBZ. O arquivo está corrompido ou não é um arquivo ZIP válido.".to_owned())?;
        let mut entries = Vec::new();
        for index in 0..archive.len() {
            let entry = archive
                .by_index(index)
                .map_err(|_| "Não foi possível ler as páginas do CBZ. O arquivo pode estar corrompido ou protegido por senha.".to_owned())?;
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
            .prefix("lumeview-comic-")
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
            ico_entries: None,
        });
        BOOKS
            .get_or_init(Default::default)
            .lock()
            .map_err(|e| e.to_string())?
            .push(Arc::downgrade(&book));
        Ok(book)
    }
    fn open_ico(source: PathBuf) -> backend::Result<Arc<Self>> {
        let data=std::fs::read(&source).map_err(|e|e.to_string())?;
        let invalid=||"Não foi possível abrir o ICO. O arquivo está inválido ou corrompido.".to_owned();
        if data.get(..4)!=Some(&[0,0,1,0]) {return Err(invalid());}
        let count=data.get(4..6).map(|v|u16::from_le_bytes([v[0],v[1]]) as usize).ok_or_else(invalid)?;
        if count==0 || count>1024 || data.len()<6+16*count {return Err(invalid());}
        let mut entries=Vec::new();let mut names=Vec::new();
        for index in 0..count {
            let entry:[u8;16]=data[6+index*16..22+index*16].try_into().unwrap();
            let length=u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset=u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            if length==0 || length as u64>MAX_PAGE || offset<6+16*count || offset.checked_add(length).is_none_or(|end|end>data.len()) {return Err(invalid());}
            let w=if entry[0]==0 {256}else{u32::from(entry[0])};
            let h=if entry[1]==0 {256}else{u32::from(entry[1])};
            let bits=u16::from_le_bytes([entry[6],entry[7]]);
            names.push(format!("{} · {} × {} · {} bits",index+1,w,h,bits));
            entries.push((offset,length,entry));
        }
        let temp=tempfile::Builder::new().prefix("lumeview-ico-").tempdir().map_err(|e|e.to_string())?;
        let pages=(0..count).map(|i|temp.path().join(format!("{i:08}.ico"))).collect();
        let stamp=stamp(&source);
        let book=Arc::new(Self{source,pages,names,temp,locks:(0..count).map(|_|Mutex::new(())).collect(),indices:Vec::new(),stamp,ico_entries:Some(entries)});
        BOOKS.get_or_init(Default::default).lock().map_err(|e|e.to_string())?.push(Arc::downgrade(&book));
        Ok(book)
    }
    fn materialize(&self, i: usize) -> backend::Result<()> {
        let _guard = self.locks[i].lock().map_err(|e| e.to_string())?;
        if stamp(&self.source) != self.stamp {
            if self.ico_entries.is_some() {return Err("O arquivo ICO mudou no disco. Use F5 para reabrir.".into());}
            return Err("O quadrinho mudou no disco. Use F5 para reabrir.".into());
        }
        if self.pages[i].is_file() {
            return Ok(());
        }
        if let Some(entries)=&self.ico_entries {
            let (offset,length,mut entry)=entries[i];
            let data=std::fs::read(&self.source).map_err(|e|e.to_string())?;
            let payload=data.get(offset..offset+length).ok_or("ICO inválido.")?;
            entry[12..16].copy_from_slice(&22u32.to_le_bytes());
            let mut temp=tempfile::NamedTempFile::new_in(self.temp.path()).map_err(|e|e.to_string())?;
            temp.write_all(&[0,0,1,0,1,0]).and_then(|_|temp.write_all(&entry)).and_then(|_|temp.write_all(payload)).map_err(|e|e.to_string())?;
            temp.flush().map_err(|e|e.to_string())?;
            temp.persist(&self.pages[i]).map_err(|e|e.to_string())?;
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
            std::io::copy(&mut entry.take(MAX_PAGE + 1), &mut temp).map_err(|e| {
                if e.to_string().eq_ignore_ascii_case("Invalid checksum") {
                    "Falha na verificação de integridade da página (CRC). Provavelmente há um erro ou corrupção no arquivo ZIP/CBZ. Teste o arquivo com o 7-Zip ou obtenha uma nova cópia.".to_owned()
                } else { e.to_string() }
            })?;
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
    fn ico_entries_are_independent_and_source_is_preserved() {
        let source=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/lumeview.ico");
        let bytes=std::fs::read(&source).unwrap();
        let book=Book::open(source.clone()).unwrap();
        assert!(book.pages.len()>1);
        let entries=book.ico_entries.as_ref().unwrap();
        for (i,(_,_,entry)) in entries.iter().enumerate() {
            let image=load(&book.pages[i]).unwrap().unwrap();
            let w=if entry[0]==0 {256}else{u32::from(entry[0])};
            let h=if entry[1]==0 {256}else{u32::from(entry[1])};
            assert_eq!(image.pixels.dimensions(),(w,h));
        }
        assert_eq!(std::fs::read(source).unwrap(),bytes);
        let dir=tempfile::tempdir().unwrap();let bad=dir.path().join("bad.ico");
        std::fs::write(&bad,b"\0\0\x01\0\x02\0").unwrap();
        assert!(Book::open(bad).is_err());
    }
    #[test]
    fn corrupt_page_crc_has_localizable_error_and_is_not_cached() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.cbz");
        let mut data = std::fs::read(fixture).unwrap();
        let headers: Vec<usize> = data.windows(4).enumerate()
            .filter_map(|(i, bytes)| (bytes == b"PK\x01\x02").then_some(i)).collect();
        assert!(!headers.is_empty());
        for i in headers { data[i + 16] ^= 1; }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("bad.cbz");
        std::fs::write(&source, data).unwrap();
        let book = Book::open(source).unwrap();
        let error = book.materialize(0).unwrap_err();
        assert!(error.contains("(CRC)"));
        assert!(error.contains("ZIP/CBZ"));
        assert!(!book.pages[0].exists());
    }
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
