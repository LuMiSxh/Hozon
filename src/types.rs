//! Core data types, enums, and reports for the Hozon conversion library.
//!
//! This module defines the fundamental data structures used throughout Hozon:
//! - Configuration options (`ConversionConfig`)
//! - Intermediate data structures (`CollectedContent`, `StructuredContent`)
//! - Reporting types (`AnalyzeReport`, `VolumeStructureReport`)
//! - Enumerations for various settings (`VolumeGroupingStrategy`, `CollectionDepth`, `FileFormat`, `Direction`)
//! - Comprehensive metadata (`EbookMetadata`)
//! - Error detail types (`AnalyzeFinding`)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Strategy for grouping collected chapters into logical volumes.
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum VolumeGroupingStrategy {
    Name,          // Group by patterns in chapter folder names (e.g., "001-001" vs "002-001")
    ImageAnalysis, // Group by detecting cover-like pages (e.g., grayscale analysis)
    #[default]
    Manual, // User provides explicit volume breaks or assumes 1 volume for collected content
    Flat,          // Treats all collected pages as a single chapter in a single output book
}

/// How deeply to scan the source directory for chapters and pages during collection.
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum CollectionDepth {
    #[default]
    Deep, // Expects structure: `source_path/chapter_folder/page.jpg`
    Shallow, // Expects structure: `source_path/page.jpg` (all pages in root, treated as one virtual chapter)
}

/// A specific finding from the analysis stage, categorized as positive, warning, or negative.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalyzeFinding {
    Positive(String),
    Warning(String),
    Negative(String),
    MissingNumericIdentifier(PathBuf),
    UnsupportedImageFormat(PathBuf, String), // Path, extension
    UnusualFileSize(PathBuf, u64, u64),      // Path, actual size, average size
    LongPath(PathBuf, usize),                // Path, length
    SpecialCharactersInPath(PathBuf),
    PermissionDenied(PathBuf),
    InconsistentPageCount(PathBuf, usize, usize), // Chapter path, actual count, average count
    InconsistentImageFormat(Vec<String>),         // List of distinct formats found
    EmptySourcePath,
    SourcePathNotDirectory,
    NoSubdirectoriesFound,
    NoPagesFoundInSubdirectories,
}

/// Defines the output file format for the generated ebook(s).
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
pub enum FileFormat {
    #[serde(rename = "EPUB")]
    Epub,
    #[default]
    #[serde(rename = "CBZ")]
    Cbz,
}

/// Defines the reading direction for content within an EPUB file.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl ToString for Direction {
    fn to_string(&self) -> String {
        match self {
            Direction::Ltr => "ltr".to_string(),
            Direction::Rtl => "rtl".to_string(),
        }
    }
}

/// Comprehensive metadata for an ebook, used for generation.
/// This struct holds all information that can be embedded into the output file(s).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EbookMetadata {
    pub title: String,
    pub series: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>, // General tags/subjects
    pub language: String,  // e.g., "en", "ja"
    pub rights: Option<String>,
    pub identifier: Option<String>, // e.g., ISBN, UUID, mangaupdates ID
    pub release_date: Option<DateTime<Utc>>,
    pub genre: Option<String>, // Specific genre (often for ComicInfo.xml)
    pub web: Option<String>,   // Website link (often for ComicInfo.xml)
    #[serde(default)]
    pub custom_fields: HashMap<String, String>, // For arbitrary key-value pairs
}

impl EbookMetadata {
    /// Creates a default `EbookMetadata` instance with a specified title and default language "en".
    pub fn default_with_title(title: String) -> Self {
        Self {
            title,
            language: "en".to_string(),
            ..Default::default()
        }
    }
}

/// Immutable configuration for a Hozon conversion task, established during `HozonConfigBuilder::build()`.
/// This holds all the user-defined settings for how the conversion should proceed.
#[derive(Debug, Clone, Default)]
pub struct ConversionConfig {
    pub metadata: EbookMetadata,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    pub output_format: FileFormat,
    pub reading_direction: Direction,
    pub create_output_directory: bool,
    pub collection_depth: CollectionDepth,
    pub image_analysis_sensibility: u8, // 0-100%
}

/// Represents the outcome of the content collection and initial analysis phase.
/// This data structure holds the organized image paths and an `AnalyzeReport`.
#[derive(Debug, Clone)]
pub struct CollectedContent {
    pub chapters_with_pages: Vec<Vec<PathBuf>>, // Vec<Chapter: Vec<PagePath>>
    pub report: AnalyzeReport,                  // Report from the collection/analysis phase
    pub grouping_strategy_recommended: VolumeGroupingStrategy, // What strategy `collect_content` recommended
}

/// Represents the outcome of the volume structuring (grouping) phase.
/// This data structure holds the image paths organized into logical volumes
/// and a `VolumeStructureReport`.
#[derive(Debug, Clone)]
pub struct StructuredContent {
    pub volumes_with_chapters_and_pages: Vec<Vec<Vec<PathBuf>>>, // Vec<Volume: Vec<Chapter: Vec<PagePath>>
    pub report: VolumeStructureReport, // Report from the structuring phase
    pub grouping_strategy_applied: VolumeGroupingStrategy, // What strategy was actually used
}

/// Report from the initial content collection and analysis stage.
/// This summarizes findings about the source material.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalyzeReport {
    pub findings: Vec<AnalyzeFinding>,
    pub recommended_strategy: VolumeGroupingStrategy,
}

/// Report from the volume structuring (grouping) stage.
/// This summarizes how content was organized into volumes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeStructureReport {
    pub total_chapters_processed: usize,
    pub total_volumes_created: usize,
    pub chapter_counts_per_volume: Vec<usize>, // e.g., `[10, 12, 8]` for 3 volumes
}

/// Specifies the intended starting point for a Hozon conversion.
/// Used by `HozonConfig::preflight_check` to tailor validation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HozonExecutionMode {
    /// The conversion will start by collecting chapters and pages from the configured `source_path`.
    FromSource,
    /// The conversion will start with user-provided collected chapters and pages.
    FromCollectedData,
    /// The conversion will start with user-provided fully structured volumes.
    FromStructuredData,
}

/// Utility function: Determines file type and MIME type from a file path
///
/// # Arguments
///
/// * `image_path` - Path to the file to analyze
///
/// # Returns
///
/// * `Ok((&str, &str))` - A tuple containing (file extension, MIME type)
/// * `Err(Error)` - An error if the file format is unsupported
///
/// # Supported formats
///
/// - JPEG/JPG: image/jpeg
/// - PNG: image/png
/// - WebP: image/webp
pub fn get_file_info(image_path: &PathBuf) -> Result<(&'static str, &'static str)> {
    let path = image_path.extension().and_then(|e| e.to_str());

    match path {
        Some("jpg") | Some("jpeg") => Ok(("jpg", "image/jpeg")),
        Some("png") => Ok(("png", "image/png")),
        Some("webp") => Ok(("webp", "image/webp")),
        _ => Err(Error::Unsupported(format!("Image format {:#?}", path))),
    }
}
