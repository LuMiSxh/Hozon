//! # Hozon - High-Performance Image to Ebook Conversion Library
//!
//! Hozon is a Rust library that provides a fast, asynchronous, and feature-rich API
//! for converting image-based content (manga, comics, photo collections) into
//! standardized ebook formats (CBZ and EPUB). It offers intelligent content analysis,
//! flexible volume grouping strategies, and comprehensive metadata support.
//!
//! ## Features
//!
//! - **Multiple Input Methods**: Convert from directory structures, pre-collected data, or structured volumes
//! - **Smart Analysis**: Automatic content analysis with configurable sensitivity for optimal grouping
//! - **Flexible Volume Strategies**: Name-based, image analysis, manual, or flat grouping options
//! - **Rich Metadata Support**: Complete ebook metadata including custom fields and multilingual support
//! - **High Performance**: Async/parallel processing with configurable concurrency limits
//! - **Robust Error Handling**: Comprehensive error reporting and validation
//! - **Cross-Platform**: Works on Windows, macOS, and Linux
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use hozon::prelude::*;
//! use std::path::PathBuf;
//!
//! #[tokio::main]
//! async fn main() -> hozon::error::Result<()> {
//!     // Simple conversion with automatic settings
//!     let config = HozonConfig::builder()
//!         .metadata(EbookMetadata::default_with_title("My Comic".to_string()))
//!         .source_path(PathBuf::from("./manga_chapters"))
//!         .target_path(PathBuf::from("./output"))
//!         .output_format(FileFormat::Cbz)
//!         .create_output_directory(true)
//!         .build()?;
//!
//!     config.convert_from_source().await?;
//!     println!("Conversion complete!");
//!     Ok(())
//! }
//! ```
//!
//! ## Advanced Usage
//!
//! ### Content Analysis
//!
//! Analyze your source content before conversion to understand its structure:
//!
//! ```rust,no_run
//! use hozon::prelude::*;
//! # use std::path::PathBuf;
//! # #[tokio::main]
//! # async fn main() -> hozon::error::Result<()> {
//!
//! let config = HozonConfig::builder()
//!     .metadata(EbookMetadata::default_with_title("Analysis Example".to_string()))
//!     .source_path(PathBuf::from("./source"))
//!     .target_path(PathBuf::from("./output"))
//!     .build()?;
//!
//! // Analyze without converting
//! let collected = config.analyze_source().await?;
//! println!("Found {} chapters", collected.chapters_with_pages.len());
//! println!("Analysis findings: {:?}", collected.report.findings);
//! println!("Recommended strategy: {:?}", collected.report.recommended_strategy);
//! # Ok(())
//! # }
//! ```
//!
//! ### Custom Metadata and Multiple Volumes
//!
//! ```rust,no_run
//! use hozon::prelude::*;
//! use std::collections::HashMap;
//! # use std::path::PathBuf;
//! # use chrono::Utc;
//! # #[tokio::main]
//! # async fn main() -> hozon::error::Result<()> {
//!
//! let mut custom_fields = HashMap::new();
//! custom_fields.insert("Source".to_string(), "Official Release".to_string());
//! custom_fields.insert("Translator".to_string(), "Translation Team".to_string());
//!
//! let metadata = EbookMetadata {
//!     title: "Advanced Example Series".to_string(),
//!     series: Some("Example Manga".to_string()),
//!     authors: vec!["Manga Author".to_string()],
//!     publisher: Some("Example Publisher".to_string()),
//!     description: Some("An example manga series for demonstration.".to_string()),
//!     language: "ja".to_string(),
//!     tags: vec!["manga".to_string(), "action".to_string()],
//!     release_date: Some(Utc::now()),
//!     custom_fields,
//!     ..Default::default()
//! };
//!
//! let config = HozonConfig::builder()
//!     .metadata(metadata)
//!     .source_path(PathBuf::from("./manga_source"))
//!     .target_path(PathBuf::from("./output"))
//!     .output_format(FileFormat::Epub)
//!     .reading_direction(Direction::Rtl) // Right-to-left for manga
//!     .volume_grouping_strategy(VolumeGroupingStrategy::Name)
//!     .image_analysis_sensibility(90) // High sensitivity for precise grouping
//!     .build()?;
//!
//! config.convert_from_source().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Directory Structure
//!
//! Hozon expects one of these directory structures:
//!
//! ```text
//! # Deep structure (default)
//! source/
//! ├── Chapter_01/
//! │   ├── page_001.jpg
//! │   ├── page_002.jpg
//! │   └── ...
//! ├── Chapter_02/
//! │   ├── page_001.jpg
//! │   └── ...
//! └── ...
//!
//! # Shallow structure
//! source/
//! ├── page_001.jpg
//! ├── page_002.jpg
//! └── ...
//! ```
//!
//! ## Volume Grouping Strategies
//!
//! - **`VolumeGroupingStrategy::Name`**: Groups chapters by name patterns (e.g., "Vol1-Ch01", "Vol1-Ch02", "Vol2-Ch01")
//! - **`VolumeGroupingStrategy::ImageAnalysis`**: Detects volume breaks by analyzing cover pages (grayscale detection)
//! - **`VolumeGroupingStrategy::Manual`**: Uses explicit volume sizes or treats all content as one volume
//! - **`VolumeGroupingStrategy::Flat`**: Combines all pages into a single chapter in one volume
//!
//! For detailed examples and API documentation, see the individual module documentation.

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
/// This module re-exports the most commonly used types, traits, and functions,
/// allowing you to import everything you need with a single `use hozon::prelude::*;` statement.
///
/// ## Included Types
///
/// - **Core Config**: `HozonConfig`, `HozonConfigBuilder`
/// - **Metadata**: `EbookMetadata`
/// - **Data Structures**: `CollectedContent`, `StructuredContent`
/// - **Enums**: `FileFormat`, `Direction`, `VolumeGroupingStrategy`, `CollectionDepth`
/// - **Analysis**: `AnalyzeReport`, `AnalyzeFinding`, `VolumeStructureReport`
/// - **Utilities**: `Collector`, `Regex`, `PathBuf`, `Path`, `Arc`
/// - **Error Handling**: `error` module
/// - **Execution Modes**: `HozonExecutionMode`
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
