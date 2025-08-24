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
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Error {
    /// I/O errors from the standard library
    #[error(transparent)]
    Io(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        std::io::Error,
    ),
    /// Regular expression parsing errors
    #[error(transparent)]
    Regex(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        regex::Error,
    ),
    /// Image processing errors
    #[error(transparent)]
    Image(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        image::ImageError,
    ),
    /// EPUB generation errors
    #[error(transparent)]
    Epub(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        epub_builder::Error,
    ),
    /// ZIP file operation errors
    #[error(transparent)]
    Zip(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        zip::result::ZipError,
    ),
    /// Async task join errors
    #[error(transparent)]
    Join(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        tokio::task::JoinError,
    ),
    #[error(transparent)]
    Sephamore(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        tokio::sync::AcquireError,
    ),
    #[error(transparent)]
    HozonBuider(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        crate::hozon::HozonConfigBuilderError,
    ),
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

#[cfg(feature = "serde")]
impl serde::Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
