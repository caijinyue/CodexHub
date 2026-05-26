use anyhow::Result;
use humansize::{format_size, BINARY};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub fn path_size(path: &Path) -> Result<u64> {
    if !path.exists() {
        return Ok(0);
    }
    if path.is_file() {
        return Ok(fs::metadata(path)?.len());
    }
    let mut total = 0;
    for entry in WalkDir::new(path).follow_links(false) {
        let entry = entry?;
        let meta = entry.metadata()?;
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

pub fn human(bytes: u64) -> String {
    format_size(bytes, BINARY)
}
