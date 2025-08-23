//! Hozon - Image to Ebook Conversion Library
//!
//! This crate provides a high-performance, asynchronous, and declarative API
//! for converting image-based content into standardized ebook formats (CBZ and EPUB).
//!
//! # Getting Started
//!
//! To use Hozon, first define your conversion task by creating an `EbookMetadata`
//! struct and configuring the `HozonConfig` via its builder. Then, execute the
//! task with one of the `convert_from_*` methods.
//!
//! ```rust,no_run
//! use hozon::prelude::*;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> hozon::error::Result<()> {
//!     let source_dir = PathBuf::from("./my_manga_collection/series_a");
//!     let target_dir = PathBuf::from("./converted_ebooks");
//!
//!     // 1. Define the metadata for your ebook
//!     let metadata = EbookMetadata {
//!         title: "My Awesome Series".to_string(),
//!         authors: vec!["Jane Doe".to_string()],
//!         genre: Some("Action, Adventure".to_string()),
//!         ..Default::default()
//!     };
//!
//!     // 2. Configure your conversion task using the builder
//!     let config = HozonConfig::builder()
//!         .metadata(metadata) // Pass the metadata struct
//!         .source_path(source_dir.clone())
//!         .target_path(target_dir.clone())
//!         .output_format(FileFormat::Epub)
//!         .reading_direction(Direction::Ltr)
//!         .create_output_directory(true)
//!         .volume_grouping_strategy(VolumeGroupingStrategy::Name)
//!         .image_analysis_sensibility(85)
//!         .build()?;
//!
//!     // Optional: Run a pre-flight check for the intended mode
//!     config.preflight_check(HozonExecutionMode::FromSource)?;
//!
//!     // 3. Execute the full conversion pipeline from the source path
//!     println!("Starting full conversion for: '{}' from source: {:?}",
//!              config.metadata.title, config.source_path);
//!     config.convert_from_source().await?;
//!     println!("Conversion complete!");
//!
//!     Ok(())
//! }
//! ```
//!
//! For more advanced usage, including custom sorting, providing pre-collected data,
//! or converting flat image lists, refer to the module-level documentation.

pub mod collector;
pub mod error;
pub mod generator;
pub mod hozon;
pub mod types;

// Publicly expose the main `HozonConfig` struct and its builder
pub use hozon::HozonConfig;
pub use hozon::HozonConfigBuilder;

// Re-export error and core types for direct access
pub use types::{
    AnalyzeFinding, AnalyzeReport, CollectedContent, CollectionDepth, Direction, EbookMetadata,
    FileFormat, HozonExecutionMode, StructuredContent, VolumeGroupingStrategy,
    VolumeStructureReport,
};

/// Prelude module for convenient imports.
///
/// This module re-exports the most commonly used types and traits, allowing you to
/// import everything you need with a single `use hozon::prelude::*;` statement.
pub mod prelude {
    pub use super::{
        AnalyzeFinding, AnalyzeReport, CollectedContent, CollectionDepth, Direction, EbookMetadata,
        FileFormat, HozonConfig, HozonConfigBuilder, HozonExecutionMode, StructuredContent,
        VolumeGroupingStrategy, VolumeStructureReport, error, generator, types,
    };
    pub use crate::collector::Collector;
    pub use regex::Regex;
    pub use std::cmp::Ordering;
    pub use std::path::{Path, PathBuf};
    pub use std::sync::Arc;
}
