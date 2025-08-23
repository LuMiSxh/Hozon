//! Custom error types and result handling for Hozon operations.
//!
//! This module defines the comprehensive error handling system used throughout Hozon.
//! All operations return a [`Result<T>`] which is a type alias for `std::result::Result<T, Error>`.
//!
use std::path::PathBuf;

/// Type alias for Results with Hozon errors.
pub type Result<T> = std::result::Result<T, Error>;

/// Comprehensive error type for all Hozon operations.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    /// I/O errors from the standard library
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Regular expression parsing errors
    #[error(transparent)]
    Regex(#[from] regex::Error),
    /// Image processing errors
    #[error(transparent)]
    Image(#[from] image::ImageError),
    /// EPUB generation errors
    #[error(transparent)]
    Epub(#[from] epub_builder::Error),
    /// ZIP file operation errors
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    /// Async task join errors
    #[error(transparent)]
    Join(#[from] tokio::task::JoinError),
    #[error(transparent)]
    Sephamore(#[from] tokio::sync::AcquireError),
    #[error(transparent)]
    HozonBuider(#[from] crate::hozon::HozonConfigBuilderError),
    /// Error for invalid file or directory paths
    #[error("The given path '{0:?}' is invalid: {1}")]
    InvalidPath(PathBuf, String),
    /// Error for failed asynchronous tasks (e.g., Tokio JoinError)
    #[error("Asynchronous task failed: {0}")]
    AsyncTaskError(String),
    /// Error for unsupported operations or formats (e.g., unknown image extension)
    #[error("Unsupported: {0}")]
    Unsupported(String),
    /// Error for resources that couldn't be found (e.g., source directory, image file)
    #[error("Not found: {0}")]
    NotFound(String),
    /// Other errors that don't fit into specific categories
    #[error("Other error: {0}")]
    Other(String),
}

// Basic From<String> conversion for convenience
impl From<String> for Error {
    fn from(error: String) -> Self {
        Error::Other(error)
    }
}

// Convert eyre::Report to String for our error type
impl From<&str> for Error {
    fn from(error: &str) -> Self {
        Error::Other(error.to_string())
    }
}
