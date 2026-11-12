//! Uniform read access to ZIP archives and extracted folders.
//!
//! Safety: nothing is ever extracted to disk (zip-slip is impossible), entry names with
//! `..` or absolute paths are ignored, a per-file and a total uncompressed budget guard
//! against zip bombs, and symlinks in folders are not followed.

use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use crate::ImportError;

pub(crate) const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;
pub(crate) const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ENTRIES: usize = 100_000;
const MAX_DEPTH: usize = 12;

pub(crate) trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

enum Backend {
    Zip(zip::ZipArchive<Box<dyn ReadSeek>>),
    Dir(PathBuf),
}

struct Entry {
    /// Normalised name with `/` separators.
    name: String,
    /// Zip index (unused for folders).
    idx: usize,
}

pub(crate) struct Archive {
    backend: Backend,
    entries: Vec<Entry>,
    total_read: u64,
}

fn safe_name(name: &str) -> bool {
    !name.starts_with('/')
        && !name.split('/').any(|seg| seg == ".." || seg.contains(':'))
        && !name.contains('\0')
}

impl Archive {
    pub(crate) fn from_zip<R: Read + Seek + 'static>(reader: R) -> Result<Self, ImportError> {
        let boxed: Box<dyn ReadSeek> = Box::new(reader);
        let mut zip = zip::ZipArchive::new(boxed)
            .map_err(|e| ImportError::Invalid(format!("ZIP-Archiv nicht lesbar: {e}")))?;
        if zip.len() > MAX_ENTRIES {
            return Err(ImportError::Invalid(
                "ZIP-Archiv enthält zu viele Dateien".into(),
            ));
        }
        let mut entries = Vec::new();
        for idx in 0..zip.len() {
            let Ok(f) = zip.by_index_raw(idx) else {
                continue;
            };
            if f.is_dir() {
                continue;
            }
            let Ok(name) = f.name().map(|n| n.replace('\\', "/")) else {
                continue;
            };
            if safe_name(&name) {
                entries.push(Entry { name, idx });
            }
        }
        Ok(Self {
            backend: Backend::Zip(zip),
            entries,
            total_read: 0,
        })
    }

    pub(crate) fn from_dir(root: &Path) -> Result<Self, ImportError> {
        let mut entries = Vec::new();
        walk(root, "", 0, &mut entries)?;
        Ok(Self {
            backend: Backend::Dir(root.to_path_buf()),
            entries,
            total_read: 0,
        })
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    /// Read one entry. `Ok(None)` if it exceeds the per-file limit; `Err` if the total
    /// budget is exhausted.
    pub(crate) fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, ImportError> {
        let Some(entry) = self.entries.iter().find(|e| e.name == name) else {
            return Ok(None);
        };
        let idx = entry.idx;
        let budget = MAX_TOTAL_BYTES
            .saturating_sub(self.total_read)
            .min(MAX_FILE_BYTES);
        let mut buf = Vec::new();
        match &mut self.backend {
            Backend::Zip(zip) => {
                let f = zip
                    .by_index(idx)
                    .map_err(|e| ImportError::Invalid(format!("ZIP-Eintrag nicht lesbar: {e}")))?;
                if f.size() > MAX_FILE_BYTES {
                    return Ok(None);
                }
                f.take(budget + 1).read_to_end(&mut buf)?;
            }
            Backend::Dir(root) => {
                let path = root.join(name);
                if std::fs::metadata(&path)?.len() > MAX_FILE_BYTES {
                    return Ok(None);
                }
                File::open(path)?.take(budget + 1).read_to_end(&mut buf)?;
            }
        }
        self.total_read += buf.len() as u64;
        if buf.len() as u64 > budget {
            if self.total_read > MAX_TOTAL_BYTES {
                return Err(ImportError::Invalid("Archiv ist entpackt zu groß".into()));
            }
            return Ok(None);
        }
        Ok(Some(buf))
    }
}

fn walk(dir: &Path, prefix: &str, depth: usize, out: &mut Vec<Entry>) -> Result<(), ImportError> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let Ok(ft) = item.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let fname = item.file_name().to_string_lossy().into_owned();
        let rel = if prefix.is_empty() {
            fname
        } else {
            format!("{prefix}/{fname}")
        };
        if ft.is_dir() {
            walk(&item.path(), &rel, depth + 1, out)?;
        } else if ft.is_file() {
            if out.len() >= MAX_ENTRIES {
                return Err(ImportError::Invalid(
                    "Ordner enthält zu viele Dateien".into(),
                ));
            }
            out.push(Entry { name: rel, idx: 0 });
        }
    }
    Ok(())
}
