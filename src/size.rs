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
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) if is_not_found(err.io_error()) => continue,
            Err(err) => return Err(err.into()),
        };
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(err) if is_not_found(err.io_error()) => continue,
            Err(err) => return Err(err.into()),
        };
        if meta.is_file() {
            total += meta.len();
        }
    }
    Ok(total)
}

fn is_not_found(error: Option<&std::io::Error>) -> bool {
    error.is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
}

pub fn human(bytes: u64) -> String {
    format_size(bytes, BINARY)
}
