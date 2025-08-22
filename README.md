# Hozon - Image to Ebook Conversion Library

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Hozon** is a high-performance, asynchronous Rust library designed for converting image-based content (like manga or comics) into standardized ebook formats such as CBZ and EPUB. Hozon provides a robust, fluent API for flexible image collection, intelligent content structuring, detailed analysis, and high-quality ebook generation.

> **Note**: This project is currently in development.

## Features

- **Flexible Image Collection**: Adapt to various source directory structures (flat list of pages, chapters in subfolders) using `CollectionDepth`.
- **Intelligent Content Structuring**: Group collected chapters into logical volumes using advanced strategies:
    - `Name`: Based on numerical patterns in chapter folder names (e.g., "001-001", "001-002").
    - `ImageAnalysis`: Automatically detects volume breaks using grayscale image detection (e.g., cover pages).
    - `Manual`: Provides full control over volume sizes.
    - `Flat`: Treats all content as a single output book, bypassing complex bundling.
- **Configurable Generation**: Convert structured image sets into CBZ and EPUB files.
- **Comprehensive Analysis**: Get detailed feedback on source structure, potential issues, and a recommended structuring strategy via granular `AnalyzeFinding`s.
- **Rich Metadata Support**: Embed comprehensive ebook metadata (title, author, publisher, description, tags, custom fields) in output files.
- **Customizable Sorting**: Provide custom regex patterns or even full closure-based sorters for precise control over chapter and page ordering.
- **Dynamic Workflows**: Initialize with raw paths, pre-collected pages, or pre-bundled volumes, skipping unnecessary steps.
- **Asynchronous & Parallel**: Leverages `tokio` for concurrent I/O and `rayon` for CPU-bound tasks (image analysis).
- **Robust Error Handling**: Detailed `HozonError` types for clearer debugging.
- **Fluent Builder API**: A clean, chainable interface guiding you through the conversion pipeline.

## Quick Start

Add Hozon to your `Cargo.toml`:

```toml
[dependencies]
hozon = "0.1" # (Version to be determined)
```

### Basic Workflow: Analyze, Bundle, Convert

This is the most common full-pipeline usage.

```rust
use hozon::prelude::*;
use std::path::{PathBuf, Path};

#[tokio::main]
async fn main() -> HozonResult<()> {
    let source_dir = PathBuf::from("./my_manga_collection/series_a");
    let target_dir = PathBuf::from("./converted_ebooks");

    // 1. Initialize Hozon with essential configuration and basic metadata
    let mut hozon = Hozon::builder()
        .name("My Awesome Series".to_string())
        .source_path(source_dir.clone())
        .target_path(target_dir.clone())
        .output_format(FileFormat::Epub) // Or FileFormat::Cbz
        .reading_direction(Direction::Ltr)
        .create_output_directory(true)
        .with_author("Jane Doe".to_string())
        .with_genre("Action, Adventure".to_string())
        .build()?;

    // 2. Run analysis, inspect results, and apply recommended strategy
    println!("Starting analysis for: {}", hozon.get_state().config.name);
    hozon = hozon.analyze().await?;
    if let Some(report) = hozon.get_analysis_report() {
        println!("Analysis Report (Findings: {}):", report.findings.len());
        for finding in &report.findings {
            println!("  - {:?}", finding);
        }
        println!("Recommended grouping strategy: {:?}", report.recommended_strategy);
        // Hozon automatically applies the recommended strategy, but you can override:
        // hozon = hozon.with_grouping_strategy(VolumeGroupingStrategy::Manual);
    }

    // 3. Bundle chapters into volumes
    println!("Starting bundling using strategy: {:?}", hozon.get_state().current_grouping_strategy);
    hozon = hozon.bundle(None).await?; // Pass sensibility for ImageAnalysis if needed: Some(80)
    if let Some(report) = hozon.get_volume_structure_report() {
        println!("Bundling complete. Volumes: {}, Chapters per volume: {:?}",
                 report.total_volumes_created, report.chapter_counts_per_volume);
    }

    // 4. Convert and save the ebook(s)
    println!("Starting conversion...");
    hozon.convert().await?;
    println!("Conversion of '{}' complete!", hozon.get_state().config.name);

    Ok(())
}
```

### Converting Flat Pages (e.g., single folder of images into one book)

```rust
use hozon::prelude::*;
use std::path::{PathBuf, Path};

#[tokio::main]
async fn main() -> HozonResult<()> {
    let flat_source_dir = PathBuf::from("./my_scans/single_issue");
    let target_dir = PathBuf::from("./converted_comics");

    // Assume flat_source_dir contains image.jpg, image2.png, etc.
    // We need to list these pages ourselves or use Collector::collect_pages
    let collector = Collector::new(&flat_source_dir, CollectionDepth::Shallow, None, None);
    let all_pages: Vec<PathBuf> = collector.collect_pages(vec![flat_source_dir.clone()], None).await?
                                           .into_iter().flatten().collect();

    let hozon = Hozon::builder()
        .name("Single Issue Comic".to_string())
        .source_path(flat_source_dir) // Still good practice to link to original source
        .target_path(target_dir)
        .output_format(FileFormat::Cbz)
        .with_flat_pages(all_pages) // Provide flat list of pages
        .with_author("Comic Creator".to_string())
        .with_description("A quick one-shot comic.".to_string())
        .build()?;

    // No need to call analyze() or bundle(), Hozon is ready to convert!
    hozon.convert().await?;
    println!("Flat conversion of '{}' complete!", hozon.get_state().config.name);

    Ok(())
}
```

### Customizing Sorting with Regex

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> HozonResult<()> {
    let source_dir = PathBuf::from("./custom_named_manga/volume-chapter-pages");
    let target_dir = PathBuf::from("./output");

    // Assume chapters are named like "V01_C001_Title" and pages like "IMG_001.jpg"
    let hozon = Hozon::builder()
        .name("Custom Sorted Manga".to_string())
        .source_path(source_dir)
        .target_path(target_dir)
        .output_format(FileFormat::Epub)
        .with_chapter_name_regex(r"V(\d+)_C(\d+)".to_string()) // Extract numbers from Volume/Chapter pattern
        .with_page_name_regex(r"IMG_(\d+)".to_string())       // Extract page number after IMG_
        .with_grouping_strategy(VolumeGroupingStrategy::Name) // Use name-based sorting with our regex
        .build()?;

    hozon.analyze().await?.bundle(None).await?.convert().await?;
    println!("Custom regex conversion complete!");

    Ok(())
}
```

## API Overview

### `Hozon` (The Conversion Pipeline)

The `Hozon` struct is the central orchestrator, holding the evolving state and orchestrating the conversion process.

```rust
pub struct Hozon {
    // pub state: HozonState, // Internal configuration & results
}

impl Hozon {
    /// Entry point to start building a new conversion pipeline.
    pub fn builder() -> HozonBuilder { /* ... */ }

    /// Access the current internal state (read-only).
    pub fn get_state(&self) -> &HozonState { /* ... */ }

    /// Access the results from the last `analyze()` call.
    pub fn get_analysis_report(&self) -> Option<&AnalyzeReport> { /* ... */ }

    /// Access the results from the last `bundle()` call.
    pub fn get_volume_structure_report(&self) -> Option<&VolumeStructureReport> { /* ... */ }

    /// Explicitly sets the volume grouping strategy, overriding `analyze()`'s recommendation.
    /// Returns `Self` for chaining.
    pub fn with_grouping_strategy(mut self, strategy: VolumeGroupingStrategy) -> Self { /* ... */ }

    /// Provides an override for the collected chapter and page paths (Vec<Vec<PathBuf>>).
    /// This is useful for manual editing of the structure after initial collection/analysis.
    /// Returns `Self` for chaining.
    pub fn with_edited_data(mut self, edited_data: Vec<Vec<PathBuf>>) -> Self { /* ... */ }

    /// Performs source directory analysis, populating `collected_chapters_pages` and `analysis_report`.
    /// This step is optional if `with_flat_pages`, `with_chapters_and_pages`, or `with_pre_bundled_data` were used.
    /// Returns `Self` for chaining.
    pub async fn analyze(mut self) -> HozonResult<Self> { /* ... */ }

    /// Bundles collected chapters into volumes based on the currently set `VolumeGroupingStrategy`.
    /// Requires collected page data. Populates `volume_structures` and `volume_structure_report`.
    /// `sensibility` is only used for `ImageAnalysis` strategy (0-100%).
    /// This step is optional if `with_pre_bundled_data` or `VolumeGroupingStrategy::Flat` is active.
    /// Returns `Self` for chaining.
    pub async fn bundle(mut self, sensibility: Option<usize>) -> HozonResult<Self> { /* ... */ }

    /// Final step: Converts the structured volumes into the specified output format.
    /// Consumes `self` as it's the terminal operation.
    pub async fn convert(self) -> HozonResult<()> { /* ... */ }
}
```

### `HozonBuilder` (Initial Configuration)

The `HozonBuilder` handles the initial setup of the `Hozon` pipeline.

```rust
#[derive(Debug, Default, Builder)]
#[builder(setter(into))]
pub struct HozonBuilder {
    // Core Conversion Configuration
    #[builder(default)] name: String,
    #[builder(default)] source_path: PathBuf,
    #[builder(default)] target_path: PathBuf,
    #[builder(default = "FileFormat::Cbz")] output_format: FileFormat,
    #[builder(default = "Direction::Ltr")] reading_direction: Direction,
    #[builder(default = "true")] create_output_directory: bool,
    #[builder(default = "CollectionDepth::Deep")] collection_depth: CollectionDepth,
    #[builder(default = "75")] image_analysis_sensibility: u8, // 0-100%

    // Ebook Metadata (optional, but highly recommended)
    #[builder(default)] metadata: EbookMetadata,

    // Advanced Data Input (bypasses earlier pipeline stages)
    #[builder(default)] collected_chapters_pages_input: Option<Vec<Vec<PathBuf>>>, // For with_chapters_and_pages
    #[builder(default)] flat_pages_input: Option<Vec<PathBuf>>, // For with_flat_pages
    #[builder(default)] pre_bundled_volumes_input: Option<Vec<Vec<Vec<PathBuf>>>>, // For with_pre_bundled_data

    // Custom Sorting & Grouping
    #[builder(default = "VolumeGroupingStrategy::Manual")] initial_grouping_strategy: VolumeGroupingStrategy,
    #[builder(default)] chapter_name_regex_str: Option<String>,
    #[builder(default)] page_name_regex_str: Option<String>,
    #[builder(default)] custom_chapter_path_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    #[builder(default)] custom_page_path_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    #[builder(default)] volume_sizes_override: Option<Vec<usize>>, // For Manual grouping strategy
}

impl HozonBuilder {
    pub fn new() -> Self { /* ... */ }

    // Convenience metadata setters (e.g., with_author, with_description, etc.)
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.metadata.authors.push(author.into()); self
    }
    // ... other metadata setters ...

    /// Provides a direct list of image paths, bypassing internal collection & bundling,
    /// and sets grouping strategy to `Flat`.
    pub fn with_flat_pages(mut self, pages: Vec<PathBuf>) -> Self { /* ... */ }

    /// Provides pre-collected chapters and their pages, bypassing internal collection.
    /// User can still choose to `bundle()` these.
    pub fn with_chapters_and_pages(mut self, data: Vec<Vec<PathBuf>>) -> Self { /* ... */ }

    /// Provides fully pre-bundled volumes (volumes of chapters of pages),
    /// ready for direct conversion.
    pub fn with_pre_bundled_data(mut self, data: Vec<Vec<Vec<PathBuf>>>) -> Self { /* ... */ }

    /// Builds the `Hozon` pipeline instance. Performs validation and compiles regexes.
    pub fn build(self) -> HozonResult<Hozon> { /* ... */ }
}
```

### Core Types

```rust
// In src/types.rs

/// Strategy for grouping collected chapters into logical volumes.
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum VolumeGroupingStrategy {
    Name, // Group by patterns in folder names (e.g., "001-001" vs "002-001")
    ImageAnalysis, // Group by detecting cover-like pages (e.g., grayscale analysis)
    #[default]
    Manual, // User provides explicit volume breaks or assumes 1 volume
    Flat, // Treats all collected pages as a single chapter in a single volume
}

/// How deeply to scan the source directory for chapters and pages.
#[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
pub enum CollectionDepth {
    #[default]
    Deep,    // Expects `source_path/chapter_folder/page.jpg` structure.
    Shallow, // Expects `source_path/page.jpg` (all pages in root, treated as one virtual chapter).
}

/// A specific finding from the analysis stage (positive, warning, or negative).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "details")]
pub enum AnalyzeFinding {
    Positive(String),
    Warning(String),
    Negative(String),
    MissingNumericIdentifier(PathBuf),
    UnsupportedImageFormat(PathBuf, String),
    UnusualFileSize(PathBuf, u64, u64),
    LongPath(PathBuf, usize),
    SpecialCharactersInPath(PathBuf),
    PermissionDenied(PathBuf),
    InconsistentPageCount(PathBuf, usize, usize),
    InconsistentImageFormat(Vec<String>),
    // ... more detailed findings can be added
}

/// Comprehensive metadata for an ebook.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EbookMetadata {
    pub title: String,
    pub series: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub language: String, // e.g., "en", "ja"
    pub rights: Option<String>,
    pub identifier: Option<String>,
    pub release_date: Option<chrono::DateTime<chrono::Utc>>,
    pub genre: Option<String>, // Specific for ComicInfo.xml
    pub page_count: Option<usize>, // Total pages in the *entire series* (or just the book if single volume)
    pub custom_fields: std::collections::HashMap<String, String>, // For arbitrary key-value pairs
}

// ... other enums like FileFormat, Direction ...

/// Internal configuration and mutable state for the conversion pipeline.
/// This struct evolves as the Hozon pipeline progresses.
#[derive(Debug, Clone, Default)]
pub struct HozonState {
    pub config: ConversionConfig, // Initial user configuration

    pub collected_chapters_pages: Option<Vec<Vec<PathBuf>>>,
    pub volume_structures: Option<Vec<Vec<Vec<PathBuf>>>>,
    pub edited_chapters_pages_override: Option<Vec<Vec<PathBuf>>>,

    pub analysis_report: Option<AnalyzeReport>,
    pub volume_structure_report: Option<VolumeStructureReport>, // Renamed

    pub current_grouping_strategy: VolumeGroupingStrategy,

    pub custom_chapter_name_regex: Option<regex::Regex>,
    pub custom_page_name_regex: Option<regex::Regex>,
    pub custom_chapter_path_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    pub custom_page_path_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,

    pub volume_sizes_override: Option<Vec<usize>>, // Used for Manual grouping
}
```

## Architecture

Hozon is organized into several key modules:

- [`src/hozon.rs`]: Implements the `Hozon` struct and its `HozonBuilder`, orchestrating the entire conversion workflow. This is your main entry point.
- [`src/collector.rs`]: Handles low-level filesystem interactions, image collection, directory structure analysis, and image property checks (e.g., grayscale detection). Highly configurable with `CollectionDepth` and custom regex/sorters.
- [`src/generator/`]: Provides traits and concrete implementations for generating specific ebook formats (`cbz.rs`, `epub.rs`). These generators leverage the rich `EbookMetadata`.
- [`src/types.rs`]: Defines all public and internal data structures, enums, and reports, including the comprehensive `EbookMetadata` and the evolving `HozonState`.
- [`src/error.rs`]: Contains the `HozonError` enum and `HozonResult` type alias for consistent and detailed error handling throughout the library.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
