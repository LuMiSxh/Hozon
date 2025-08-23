//! Generator module provides traits and implementations for various file format generators.
//!
//! This module contains the common interface for document generators and specific
//! implementations for different file formats.

use crate::error::Result;
use crate::types::EbookMetadata;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

pub mod cbz;
pub mod epub;

/// Common interface for all file generators.
///
/// The `Generator` trait defines a consistent API for document generators
/// that can create different file formats (like CBZ, EPUB) from source images.
/// Implementations handle the specifics of each file format.
#[async_trait]
pub trait Generator {
    /// Creates a new generator instance.
    ///
    /// # Parameters
    /// * `output_dir` - Directory where the generated file will be saved
    /// * `base_filename` - Base name of the output file (without extension, e.g., "My Series | Volume 1")
    ///
    /// # Returns
    /// * `Result<Self>` - A new generator instance or an error if creation fails
    fn new(output_dir: &Path, base_filename: &str) -> Result<Self>
    where
        Self: Sized;

    /// Adds a page to the generated document.
    ///
    /// # Parameters
    /// * `image_path` - Path to the image file to add as a page
    ///
    /// # Returns
    /// * `Result<&mut Self>` - Self reference for method chaining, or an error if failed
    async fn add_page(&mut self, image_path: &PathBuf) -> Result<&mut Self>
    where
        Self: Sized;

    /// Sets comprehensive metadata for the generated document.
    ///
    /// # Parameters
    /// * `file_name_base` - The base name for *this specific output file* (e.g., "My Series | Volume 1")
    /// * `file_volume_number` - The volume number of *this specific output file* (e.g., 1, 2, 3)
    /// * `series_metadata` - The complete series-level metadata
    /// * `total_pages_in_file` - The total number of pages being added to *this specific output file*
    /// * `collected_chapter_titles` - Titles of chapters included in this specific volume, for TOC/notes.
    ///
    /// # Returns
    /// * `Result<&mut Self>` - Self reference for method chaining, or an error if failed
    async fn set_metadata(
        &mut self,
        file_name_base: &str,
        file_volume_number: Option<usize>,
        series_metadata: &EbookMetadata,
        total_pages_in_file: usize,
        collected_chapter_titles: &[String],
    ) -> Result<&mut Self>
    where
        Self: Sized;

    /// Saves the generated document to disk.
    ///
    /// Finalizes the document and writes it to the specified output location.
    ///
    /// # Returns
    /// * `Result<()>` - Success indicator or an error if saving fails
    async fn save(self) -> Result<()>;
}
