use std::path::{Path, PathBuf};

use crate::extractor::extractor::{EPUBS_DIR, HTML_DIR};

#[derive(PartialEq, Eq, Clone, Hash)]
pub(crate) struct DirHelper {
    common_path: PathBuf,
}

impl DirHelper {
    pub fn new(path: PathBuf) -> Self {
        return DirHelper { common_path: path };
    }

    pub fn epub_file_path(&self) -> PathBuf {
        let path = Path::new(EPUBS_DIR);
        let path = path.join(&self.common_path);
        let path = path.with_extension("epub");
        return path;
    }

    pub fn html_dir(&self) -> PathBuf {
        let path = Path::new(HTML_DIR);
        let path = path.join(&self.common_path);
        return path;
    }
}
