//! Comprehensive error types and result handling for Hozon operations.
//!
//! This module defines the error handling system used throughout Hozon, providing
//! detailed error information for debugging and user feedback. All operations return
//! a [`Result<T>`] which is a type alias for `std::result::Result<T, Error>`.
//!
//! # Error Categories
//!
//! The [`Error`] enum covers several categories of errors:
//!
//! - **I/O Errors**: File system operations, permission issues
//! - **Format Errors**: Unsupported file formats, parsing issues
//! - **Configuration Errors**: Invalid regex patterns, missing required fields
//! - **Resource Errors**: Missing files/directories, memory allocation
//! - **Processing Errors**: Image processing, compression, async task failures
//!
//! # Examples
//!
//! ```rust,no_run
//! use hozon::prelude::*;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = HozonConfig::builder()
//!         .metadata(EbookMetadata::default_with_title("Test".to_string()))
//!         .source_path(PathBuf::from("./nonexistent"))
//!         .target_path(PathBuf::from("./output"))
//!         .build();
//!
//!     match config {
//!         Ok(cfg) => {
//!             if let Err(e) = cfg.convert_from_source(CoverOptions::None).await {
//!                 match e {
//!                     hozon::error::Error::NotFound(msg) => {
//!                         eprintln!("Source not found: {}", msg);
//!                     }
//!                     hozon::error::Error::InvalidPath(path, reason) => {
//!                         eprintln!("Invalid path {:?}: {}", path, reason);
//!                     }
//!                     _ => eprintln!("Other error: {}", e),
//!                 }
//!             }
//!         }
//!         Err(e) => eprintln!("Configuration error: {}", e),
//!     }
//! }
//! ```
//!
use std::path::PathBuf;

/// Type alias for Results with Hozon errors.
pub type Result<T> = std::result::Result<T, Error>;

/// Comprehensive error type for all Hozon operations.
///
/// This enum represents all possible errors that can occur during Hozon operations,
/// from configuration validation to file processing and ebook generation. Each variant
/// provides specific context about the error condition.
#[derive(thiserror::Error, Debug)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
pub enum Error {
    /// I/O errors from file system operations.
    ///
    /// These include file reading/writing errors, permission denied errors,
    /// and other operating system-level I/O failures.
    #[error(transparent)]
    Io(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        std::io::Error,
    ),
    /// Regular expression compilation errors.
    ///
    /// Occurs when user-provided regex patterns for chapter or page name
    /// parsing are invalid or cannot be compiled.
    #[error(transparent)]
    Regex(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        regex::Error,
    ),
    /// Image processing and format errors.
    ///
    /// These include unsupported image formats, corrupted image files,
    /// or failures during image analysis operations.
    #[error(transparent)]
    Image(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        image::ImageError,
    ),
    /// EPUB generation and formatting errors.
    ///
    /// Occurs during EPUB file creation, metadata embedding,
    /// or when the epub-builder library encounters issues.
    #[error(transparent)]
    Epub(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        epub_builder::Error,
    ),
    /// ZIP file operation errors for CBZ generation.
    ///
    /// These include compression failures, archive corruption,
    /// or issues during CBZ file creation.
    #[error(transparent)]
    Zip(
        #[from]
        #[cfg_attr(feature = "serde", serde(skip))]
        zip::result::ZipError,
    ),
    /// Async task execution failures.
    ///
    /// Occurs when parallel processing tasks fail to complete
    /// or when joining async operations encounters errors.
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
    /// Error for invalid or problematic file paths.
    ///
    /// Indicates issues with path validation, accessibility,
    /// or when paths don't meet expected criteria.
    #[error("The given path '{0:?}' is invalid: {1}")]
    InvalidPath(PathBuf, String),
    /// Error for paths that exceed system limitations.
    ///
    /// Indicates that a file path is too long for the current system
    /// or contains characters that cannot be properly processed.
    #[error("Path too long or contains invalid characters: {0:?}")]
    PathTooLong(PathBuf),
    /// Error for UTF-8 conversion failures in file paths.
    ///
    /// Occurs when file paths contain non-UTF-8 sequences that cannot
    /// be converted to strings for processing.
    #[error("Path contains invalid UTF-8 sequences: {0:?}")]
    PathUtf8Error(PathBuf),
    /// Error for failed asynchronous task execution.
    ///
    /// More specific than the general `Join` error, this covers
    /// failures in custom async operations and task coordination.
    #[error("Asynchronous task failed: {0}")]
    AsyncTaskError(String),
    /// Error for unsupported operations, formats, or features.
    ///
    /// Examples include unknown image file extensions, unsupported
    /// metadata fields, or operations not implemented for certain configurations.
    #[error("Unsupported: {0}")]
    Unsupported(String),
    /// Error for missing or inaccessible resources.
    ///
    /// Indicates that required files, directories, or other resources
    /// could not be located or accessed during operations.
    #[error("Not found: {0}")]
    NotFound(String),
    /// Generic error for cases that don't fit other categories.
    ///
    /// Used for unexpected errors, custom error messages,
    /// or when wrapping errors from external libraries.
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
