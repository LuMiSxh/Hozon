# Hozon - Image to Ebook Conversion Library

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Hozon** is a high-performance, asynchronous Rust library designed for converting image-based content (like manga or comics) into standardized ebook formats such as CBZ and EPUB. Hozon provides a robust, declarative API for flexible image collection, intelligent content structuring, and high-quality ebook generation, focusing on configuration upfront and execution on demand.

> **Note**: This project is currently in development.

## Features

- **Declarative Configuration**: Define your entire conversion task upfront using a rich builder pattern.
- **Flexible Image Collection**: Adapt to various source directory structures (flat list of pages, chapters in subfolders) using `CollectionDepth`.
- **Intelligent Content Structuring**: Group collected chapters into logical volumes using advanced strategies:
    - `Name`: Based on numerical patterns in chapter folder names (e.g., "01-01", "01-02").
    - `ImageAnalysis`: Automatically detects volume breaks using grayscale image detection (e.g., cover pages).
    - `Manual`: Provides full control over volume sizes via override.
    - `Flat`: Treats all collected content as a single output book.
- **Configurable Generation**: Convert structured image sets into CBZ and EPUB files.
- **Rich Metadata Support**: Embed comprehensive ebook metadata (title, author, publisher, description, tags, custom fields) in output files.
- **Customizable Sorting**: Provide custom regex patterns or even full closure-based sorters for precise control over chapter and page ordering.
- **Dynamic Workflows**: Choose your starting point: convert directly from a source path, from pre-collected pages, or from pre-structured volumes.
- **Asynchronous & Parallel**: Leverages `tokio` for concurrent I/O and `rayon` for CPU-bound tasks.
- **Robust Error Handling**: Detailed `Error` types for clearer debugging, with optional `preflight_check` for early validation.

## Quick Start

Add Hozon to your `Cargo.toml`:

```toml
[dependencies]
hozon = "0.1"
```

### Basic Workflow: Convert from Source

This is the most common full-pipeline usage, where Hozon handles collection and structuring internally.

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let source_dir = PathBuf::from("./my_manga_collection/series_a");
    let target_dir = PathBuf::from("./converted_ebooks");

    // 1. Define the metadata for your ebook
    let metadata = EbookMetadata {
        title: "My Awesome Series".to_string(),
        authors: vec!["Jane Doe".to_string()],
        genre: Some("Action, Adventure".to_string()),
        ..Default::default()
    };

    // 2. Configure your conversion task using the builder
    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(source_dir.clone())
        .target_path(target_dir.clone())
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .build()?;

    // Optional: Run a pre-flight check
    config.preflight_check(HozonExecutionMode::FromSource)?;

    // 3. Execute the full conversion pipeline from the source path
    config.convert_from_source().await?;
    println!("Conversion complete!");

    Ok(())
}
```

### Converting Flat Pages (Single folder of images)

This bypasses internal chapter collection and uses a simple structuring strategy.

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let flat_source_dir = PathBuf::from("./my_scans/single_issue");
    let target_dir = PathBuf::from("./converted_comics");

    // We can let Hozon collect the pages by setting the collection depth.
    let metadata = EbookMetadata {
        title: "Single Issue Comic".to_string(),
        authors: vec!["Comic Creator".to_string()],
        description: Some("A quick one-shot comic.".to_string()),
        ..Default::default()
    };

    // Configure the conversion task
    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(flat_source_dir)
        .target_path(target_dir)
        .output_format(FileFormat::Cbz)
        .collection_depth(CollectionDepth::Shallow) // Treat source as a single chapter
        .build()?;

    // Execute conversion. Hozon will collect the shallow pages.
    config.convert_from_source().await?;
    println!("Flat conversion complete!");

    Ok(())
}
```

### Customizing Sorting with Regex

This demonstrates providing custom regex for `Name` grouping.

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let source_dir = PathBuf::from("./custom_named_manga");
    let target_dir = PathBuf::from("./output");

    // Assume chapters are "V01_C001_Title" and pages are "IMG_001.jpg"
    let metadata = EbookMetadata::default_with_title("Custom Sorted Manga".to_string());

    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(source_dir)
        .target_path(target_dir)
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .chapter_name_regex_str(r"V(\d+)_C(\d+)".to_string())
        .page_name_regex_str(r"IMG_(\d+)".to_string())
        .build()?;

    config.convert_from_source().await?;
    println!("Custom regex conversion complete!");

    Ok(())
}
```
