//! The cache of the encrypted files of the newest release (`Feed._cache_*`): best effort, a
//! failure only means a download. Layout `<cache_dir>/<seq>/<name with "/" -> "__">`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use clawd_contracts::FeedFile;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub(crate) struct Cache {
    root: Option<PathBuf>,
}

impl Cache {
    pub fn new(root: Option<PathBuf>) -> Self {
        Self { root }
    }

    fn path(&self, seq: u64, f: &FeedFile) -> Option<PathBuf> {
        self.root
            .as_ref()
            .map(|r| r.join(seq.to_string()).join(f.name.replace('/', "__")))
    }

    pub fn read(&self, seq: u64, f: &FeedFile) -> Option<Vec<u8>> {
        std::fs::read(self.path(seq, f)?).ok()
    }

    pub fn drop_file(&self, seq: u64, f: &FeedFile) {
        if let Some(p) = self.path(seq, f) {
            let _ = std::fs::remove_file(p);
        }
    }

    /// Write one file, then delete the other releases' folders ("only the newest release is
    /// worth keeping").
    pub fn write(&self, seq: u64, f: &FeedFile, blob: &[u8]) {
        let (Some(root), Some(p)) = (&self.root, self.path(seq, f)) else {
            return;
        };
        let _ = (|| -> std::io::Result<()> {
            let dir = p.parent().expect("cache file has a parent");
            std::fs::create_dir_all(dir)?;
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            let tmp = dir.join(format!(
                ".{}.{}.{}.tmp",
                name.unwrap_or_default(),
                std::process::id(),
                TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&tmp, blob)?;
            std::fs::rename(&tmp, &p)?;
            for old in std::fs::read_dir(root)? {
                let old = old?.path();
                if old.is_dir()
                    && old
                        .file_name()
                        .is_some_and(|n| n != seq.to_string().as_str())
                {
                    let _ = std::fs::remove_dir_all(old);
                }
            }
            Ok(())
        })();
    }
}
