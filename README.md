# Hozon - Images to CBZ/Epub Conversion Library

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Hozon** is a high-performance, asynchronous Rust library designed for converting image-based content (like manga or comics) into standardized ebook formats such as CBZ and EPUB. Extracted from the Palaxy Tauri application, Hozon provides a robust, fluent API for collecting image files, intelligently bundling them into volumes, analyzing content, and generating high-quality ebook outputs.

> **Note**: This project is currently in development.

## Features

- **Image Collection**: Efficiently gathers image files from nested directory structures.
- **Intelligent Bundling**: Supports multiple strategies for grouping chapters into volumes (manual, name-based, image analysis via grayscale detection).
- **Format Generation**: Converts image sets into CBZ and EPUB files.
- **Asynchronous Processing**: Leverages `tokio` for concurrent I/O operations and `rayon` for parallel CPU-bound tasks (image analysis).
- **Configurable Output**: Customize output format (CBZ/EPUB), reading direction, and directory creation.
- **Comprehensive Analysis**: Provides detailed feedback on directory structure, file formats, naming conventions, and potential issues.
- **Fluent Pipeline API**: A clean, chainable interface that guides the user through configuration, analysis, bundling, and conversion.
- **Robust Error Handling**: Detailed error types for easier debugging.

## Quick Start

Add Hozon to your `Cargo.toml`:

```toml
[dependencies]
hozon = "0.1" # (Version to be determined)
tokio = { version = "1.x", features = ["rt-multi-thread", "macros", "fs", "io-util", "sync", "time"] }
image = "0.25"
zip = { version = "0.6", features = ["deflate"] }
epub_builder = "0.6"
rayon = "1.11"
regex = "1.11"
lazy_static = "1.x"
memmap2 = "0.9"
log = "0.4"
thiserror = "1.x"
derive_builder = "0.20"
serde = { version = "1.0", features = ["derive"] }
```

### Basic Usage

```rust
use hozon::prelude::*;
use std::path::{PathBuf, Path};

#[tokio::main]
async fn main() -> HozonResult<()> {
    let source_dir = PathBuf::from("./my_manga_collection/one_piece_vol_1");
    let target_dir = PathBuf::from("./converted_ebooks");
    let series_name = "One Piece".to_string();

    // 1. Initialize the Hozon pipeline with basic configuration
    let mut pipeline = Hozon::builder()
        .source(source_dir)
        .target(target_dir)
        .name(series_name.clone())
        .format(FileFormat::Epub) // Or FileFormat::Cbz
        .direction(Direction::Ltr)
        .create_output_directory(true)
        .build()?;

    // 2. Run analysis, inspect results, and potentially override bundle flag
    let pipeline = pipeline.analyze().await?;
    let analysis_results = pipeline.get_analysis_results().unwrap();
    println!("Analysis complete. Recommended bundle flag: {:?}", analysis_results.payload.as_ref().map(|p| p.flag));

    // You can now access analysis_results.payload to get details.
    // If you want to override the recommended flag:
    let pipeline = pipeline.with_bundle_flag(BundleFlag::Name); // Example override

    // 3. Run bundling, inspect results (e.g., total volumes, chapter sizes)
    let pipeline = pipeline.bundle(None).await?; // Use None for default image analysis sensibility, or Some(value)
    let bundle_results = pipeline.get_bundle_results().unwrap();
    println!("Bundling complete. Total Chapters: {}, Total Volumes: {:?}",
             bundle_results.payload.as_ref().map_or(0, |p| p.total_chapters),
             bundle_results.payload.as_ref().and_then(|p| p.total_volumes));

    // 4. Convert and save the ebook(s)
    println!("Starting conversion for: {}", series_name);
    pipeline.convert().await?;
    println!("Conversion of '{}' complete!", series_name);

    Ok(())
}
```

### Advanced Usage with Manual Edits

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> HozonResult<()> {
    // ... initial setup same as above ...
    let source_dir = PathBuf::from("./my_manga_collection/one_piece_vol_1");
    let target_dir = PathBuf::from("./converted_ebooks");
    let series_name = "One Piece".to_string();

    let mut pipeline = Hozon::builder()
        .source(source_dir)
        .target(target_dir)
        .name(series_name.clone())
        .format(FileFormat::Epub)
        .build()?;

    // Analyze first to get initial data structure
    let pipeline = pipeline.analyze().await?;

    // Get the collected data for editing
    let mut edited_chapters_pages = pipeline.get_config().data.clone();

    // Example: Remove the first page of the first chapter
    if let Some(first_chapter) = edited_chapters_pages.first_mut() {
        if !first_chapter.is_empty() {
            println!("Removing first page of first chapter for manual edit demonstration.");
            first_chapter.remove(0);
        }
    }

    // Now, apply the edited data to the pipeline
    let pipeline = pipeline.with_edited_data(edited_chapters_pages);

    // Continue with bundling and conversion using the modified data
    let pipeline = pipeline.bundle(None).await?;
    pipeline.convert().await?;

    println!("Conversion with edited data for '{}' complete!", series_name);

    Ok(())
}
```

## API Overview

### `Hozon` (The Conversion Pipeline)

The `Hozon` struct acts as the central orchestrator for the conversion process. It holds the current configuration and results of each stage.

```rust
pub struct Hozon {
    // pub config: ConversionConfig, // Internal configuration state
    pub analysis_results: Option<CommAnalyzeMeta>,
    pub bundle_results: Option<CommBundle>,
    // ... (other internal fields) ...
}

impl Hozon {
    /// Entry point to start building a new conversion pipeline.
    pub fn builder() -> HozonBuilder { /* ... */ }

    /// Performs source directory analysis, updates internal `config.data`, and stores analysis metadata.
    /// Returns `Self` for chaining.
    pub async fn analyze(mut self) -> HozonResult<Self> { /* ... */ }

    /// Explicitly sets the bundle flag, overriding any recommendation from `analyze()`.
    /// Returns `Self` for chaining.
    pub fn with_bundle_flag(mut self, flag: BundleFlag) -> Self { /* ... */ }

    /// Optional: Allows overriding the collected data (chapters and pages) after `analyze()`.
    /// Returns `Self` for chaining.
    pub fn with_edited_data(mut self, data: Vec<Vec<PathBuf>>) -> Self { /* ... */ }

    /// Bundles chapters into volumes based on configured strategy, updates `config.volume_sizes`,
    /// and stores bundle metadata. Requires `analyze()` to have been called.
    /// Returns `Self` for chaining.
    pub async fn bundle(mut self, sensibility: Option<usize>) -> HozonResult<Self> { /* ... */ }

    /// Final step: Converts bundled volumes into the specified output format.
    /// Consumes `self` as it's the terminal operation.
    pub async fn convert(self) -> HozonResult<()> { /* ... */ }

    /// Access the current internal configuration.
    pub fn get_config(&self) -> &ConversionConfig { /* ... */ }

    /// Access the results from the last `analyze` call.
    pub fn get_analysis_results(&self) -> Option<&CommAnalyzeMeta> { /* ... */ }

    /// Access the results from the last `bundle` call.
    pub fn get_bundle_results(&self) -> Option<&CommBundle> { /* ... */ }
}
```

### `HozonBuilder` (The Initial Configurator)

The `HozonBuilder` struct is used solely for the initial setup of the `Hozon` pipeline.

```rust
#[derive(Debug, Default, Builder)]
#[builder(setter(into))]
pub struct HozonBuilder {
    #[builder(default)] name: String,
    #[builder(default)] source: PathBuf,
    #[builder(default)] target: PathBuf,
    #[builder(default = "BundleFlag::Manual")] bundle_flag: BundleFlag,
    #[builder(default = "Direction::Ltr")] direction: Direction,
    #[builder(default = "FileFormat::Cbz")] format: FileFormat,
    #[builder(default = "true")] create_directory: bool,
    #[builder(default)] volume_sizes: Vec<usize>, // Can be pre-set for manual
    #[builder(default)] edited_data: Option<Vec<Vec<PathBuf>>>, // Can be pre-set
}

impl HozonBuilder {
    pub fn new() -> Self { /* ... */ }

    /// Creates a new `Hozon` pipeline instance with the configured initial parameters.
    pub fn build(self) -> HozonResult<Hozon> { /* ... */ }
}
```

## Architecture

Hozon is structured into the following key modules:

- [`src/error.rs`]: Defines custom error types (`HozonError`) for consistent error handling.
- [`src/types.rs`]: Contains data structures like `BundleFlag`, `FileFormat`, `Direction`, `ConversionConfig` (internal state), and response types (`AnalyzeResponse`, `BundleResponse`, `BaseResponse`).
- [`src/collector.rs`]: Responsible for filesystem scanning, image collection, directory structure analysis, and grayscale-based volume detection.
- [`src/generator/`]: Provides traits and implementations for generating specific ebook formats (CBZ, EPUB).
    - `generator/mod.rs`: Defines the `Generator` trait.
    - `generator/cbz.rs`: CBZ file generation logic.
    - `generator/epub.rs`: EPUB file generation logic.
- [`src/pipeline.rs`]: **(New)** This module will contain the `Hozon` (public alias for `HozonConversionPipeline`) struct and its associated `HozonBuilder`. It orchestrates the entire conversion process, chaining `analyze`, `bundle`, and `convert`.
- [`src/lib.rs`]: Serves as the crate root, defining modules and re-exporting key types.

## Core Types

```rust
// In src/types.rs
pub enum BundleFlag { /* ... */ }
pub enum FileFormat { /* ... */ }
pub enum Direction { /* ... */ }

/// Internal configuration and mutable state for the conversion pipeline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConversionConfig {
    pub name: String,
    pub source: PathBuf,
    pub target: PathBuf,
    pub bundle_flag: BundleFlag,
    pub direction: Direction,
    pub format: FileFormat,
    pub create_directory: bool,
    pub volume_sizes: Vec<usize>,       // Populated by `bundle`
    pub data: Vec<Vec<PathBuf>>,        // Collected pages per chapter, populated by `analyze`
    pub edited_data: Option<Vec<Vec<PathBuf>>>, // Optional: For manual edits of chapters/pages
}

// Response types (from Palaxy's types.rs, now simplified for Hozon)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BaseResponse<T = ()> {
    pub duration: f64,
    pub comment: Option<String>,
    pub payload: Option<T>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BundleResponse {
    pub total_chapters: usize,
    pub total_volumes: Option<usize>,
    pub chapter_sizes: Option<Vec<usize>>,
}

pub type CommBundle = BaseResponse<BundleResponse>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnalyzeResponse {
    pub negative: Vec<String>,
    pub positive: Vec<String>,
    pub warning: Vec<String>,
    pub flag: BundleFlag, // Recommended bundle flag
}

pub type CommAnalyzeMeta = BaseResponse<AnalyzeResponse>;

// Utility function (from Palaxy's prelude.rs)
pub fn get_file_info(image_path: &PathBuf) -> HozonResult<(&'static str, &'static str)> { /* ... */ }
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
