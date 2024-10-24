pub mod contact;

use std::path::Path;

use thiserror::Error;

/// Generic persistence result type
pub type Result<T> = std::result::Result<T, Error>;

/// Generic persistence error type
#[derive(Debug, Error)]
pub enum Error {
    #[error("unable to write file {0}")]
    IoError(#[from] std::io::Error),

    #[error("no such {0} entity {1}")]
    NoSuchEntity(String, String),
}

pub use contact::{ContactStoreApi, FileBasedContactStore};

/// Given a base path and a directory path, ensures that the directory
/// exists and returns the full path.
pub fn file_storage_path(data_dir: &str, path: &str) -> Result<String> {
    let directory = format!("{}/{}", data_dir, path);
    if !Path::new(&directory).exists() {
        std::fs::create_dir(&directory)?;
    }
    Ok(directory)
}
