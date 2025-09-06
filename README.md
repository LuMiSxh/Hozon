# Hozon - Image to Ebook Conversion Library

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Build Status](https://github.com/lumisxh/hozon/workflows/Release%20and%20Documentation/badge.svg)](https://github.com/lumisxh/hozon/actions)
[![Documentation](https://img.shields.io/badge/docs-latest-blue.svg)](https://lumisxh.github.io/hozon/)

**Hozon** is a high-performance, asynchronous Rust library designed for converting image-based content (like manga or comics) into standardized ebook formats such as CBZ and EPUB. Hozon provides a robust, declarative API for flexible image collection, intelligent content structuring, and high-quality ebook generation, focusing on configuration upfront and execution on demand.

> **Note**: This project is currently in development.

## Features

- **Declarative Configuration**: Define your entire conversion task upfront using a rich builder pattern.
- **Robust Path Handling**: Comprehensive support for long paths, special characters, and non-ASCII filenames across all platforms.
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

## Installation

Add Hozon to your `Cargo.toml`:

```toml
[dependencies]
hozon = { git = "https://github.com/lumisxh/hozon", tag = "vX.X.X" }  # Replace `vX.X.X` with the version you want to use
```

## Quick Example

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let metadata = EbookMetadata {
        title: "My Awesome Series".to_string(),
        authors: vec!["Jane Doe".to_string()],
        genre: Some("Action, Adventure".to_string()),
        ..Default::default()
    };

    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(PathBuf::from("./source"))
        .target_path(PathBuf::from("./output"))
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .build()?;

    // Convert without custom cover
    config.convert_from_source(CoverOptions::None).await?;

    // Alternative: Convert with custom cover for all volumes
    // config.convert_from_source(CoverOptions::Single(PathBuf::from("./cover.jpg"))).await?;

    Ok(())
}
```

## Advanced Usage Examples

### Custom Cover Images

You can specify custom cover images using the `CoverOptions` parameter in all conversion methods:

```rust
use hozon::prelude::*;
use std::path::PathBuf;
use std::collections::HashMap;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("My Series".to_string()))
        .source_path(PathBuf::from("./source"))
        .target_path(PathBuf::from("./output"))
        .build()?;

    // Option 1: No custom cover (default behavior)
    config.convert_from_source(CoverOptions::None).await?;

    // Option 2: Single cover for all volumes
    config.convert_from_source(CoverOptions::Single(PathBuf::from("./cover.jpg"))).await?;

    // Option 3: Different covers per volume
    let mut covers = HashMap::new();
    covers.insert(0, PathBuf::from("./vol1_cover.jpg"));
    covers.insert(1, PathBuf::from("./vol2_cover.jpg"));
    config.convert_from_source(CoverOptions::PerVolume(covers)).await?;

    Ok(())
}
```

### Working with Pre-collected Data

Skip the analysis phase if you already have organized data:

```rust
use hozon::prelude::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> hozon::error::Result<()> {
    let chapters = vec![
        vec![PathBuf::from("ch1/page1.jpg"), PathBuf::from("ch1/page2.jpg")],
        vec![PathBuf::from("ch2/page1.jpg"), PathBuf::from("ch2/page2.jpg")],
    ];

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Pre-collected Series".to_string()))
        .target_path(PathBuf::from("./output"))
        .build()?;

    config.convert_from_collected_data(chapters, CoverOptions::None).await?;
    Ok(())
}
```

## Conversion Workflow Methods

Hozon provides three main conversion methods depending on your starting point:

### 1. `convert_from_source(cover_options)`

**Full Pipeline**: Analysis → Structuring → Generation

Use this when starting with a directory of images. Hozon will:

- Scan and analyze the directory structure
- Group chapters into logical volumes using your chosen strategy
- Generate the final ebook files

```rust
config.convert_from_source(CoverOptions::None).await?;
```

### 2. `convert_from_collected_data(chapters, cover_options)`

**Partial Pipeline**: Structuring → Generation

Use this when you've already collected and organized your images into chapters. Hozon will:

- Group your chapters into logical volumes
- Generate the final ebook files

```rust
let chapters = vec![
    vec![PathBuf::from("ch1/page1.jpg"), PathBuf::from("ch1/page2.jpg")],
    vec![PathBuf::from("ch2/page1.jpg"), PathBuf::from("ch2/page2.jpg")],
];
config.convert_from_collected_data(chapters, CoverOptions::None).await?;
```

### 3. `convert_from_structured_data(volumes, cover_options)`

**Generation Only**: Generation

Use this when you have complete control over volume organization. Hozon will only:

- Generate the final ebook files from your pre-structured data

```rust
let volumes = vec![
    // Volume 1
    vec![
        vec![PathBuf::from("vol1/ch1/page1.jpg"), PathBuf::from("vol1/ch1/page2.jpg")],
        vec![PathBuf::from("vol1/ch2/page1.jpg"), PathBuf::from("vol1/ch2/page2.jpg")],
    ],
    // Volume 2
    vec![
        vec![PathBuf::from("vol2/ch1/page1.jpg"), PathBuf::from("vol2/ch1/page2.jpg")],
    ],
];
config.convert_from_structured_data(volumes, CoverOptions::None).await?;
```

## Documentation

### API Documentation

Comprehensive API documentation is automatically generated and available at:
**[https://lumisxh.github.io/hozon/](https://lumisxh.github.io/Hozon/)**

The documentation includes:

- Complete API reference with examples
- Detailed usage patterns and workflows
- Configuration options and best practices
- Error handling guides

## Development Status

This library is actively developed with automated testing and security auditing. Check the [Actions page](https://github.com/lumisxh/hozon/actions) for current build status and security audit results.

## Contributing

Contributions are welcome! Please see the API documentation for development guidelines and architecture details.

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
