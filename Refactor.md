# Refactoring Plan: Extracting Conversion Logic to 'Hozon' Crate

This document outlines the plan to extract the core image-to-ebook conversion logic from the `palaxy` Tauri application backend into a new, standalone Rust crate named `Hozon`. The new crate will follow a similar structure (testing, documentation, README) to the existing `Tosho` crate, with an enhanced builder-pattern API.

## I. New Crate Structure

The `Hozon` crate will have the following directory structure:

```
└── hozon/
    ├── src
    │   ├── lib.rs
    │   ├── pipeline.rs        # New: Hozon (Conversion Pipeline) struct and HozonBuilder
    │   ├── collector.rs       # Moved from palaxy/src/collector.rs
    │   ├── generator/         # Moved from palaxy/src/generator/
    │   │   ├── mod.rs
    │   │   ├── cbz.rs
    │   │   └── epub.rs
    │   ├── error.rs           # New: Custom error definitions for Hozon
    │   └── types.rs           # New: Core data types (enums, config structs)
    ├── tests/
    │   ├── unit.rs
    │   ├── integration.rs
    │   └── README.md
    ├── .gitignore
    ├── Cargo.toml
    └── README.md              # New: Crate documentation, similar to Tosho
```

## II. Module-by-Module Refactoring Plan

### 1. `Cargo.toml` Setup

- **Action**: Create `hozon/Cargo.toml`.
- **Content**:
    - Set `name = "hozon"`, `version = "0.1.0"`, `edition = "2024"`.
    - Add `description = "A library for converting image-based content to ebook formats (CBZ/EPUB)"`, `license = "MIT"`, `repository = "https://github.com/yourusername/hozon"`, `keywords = ["ebook", "conversion", "cbz", "epub", "manga"]`, `categories = ["multimedia", "filesystem"]`.
    - **Dependencies**:
        - `tokio = { version = "1.x", features = ["rt-multi-thread", "macros", "fs", "io-util", "sync", "time"] }` (essential for async operations)
        - `async-trait = "0.1"` (for the `Generator` trait)
        - `rayon = "1.11"` (for parallel processing in `Collector`)
        - `image = "0.25"` (for image processing in `Collector`, specifically `DynamicImage`)
        - `lazy_static = "1.x"` (for `REGEX_ANALYZE` in `Collector`)
        - `regex = "1.11"` (for `REGEX_ANALYZE` in `Collector`)
        - `zip = { version = "0.6", features = ["deflate"] }` (for CBZ generation)
        - `epub_builder = "0.6"` (for EPUB generation)
        - `memmap2 = "0.9"` (for efficient file mapping in `Generator`)
        - `log = "0.4"` (for logging)
        - `thiserror = "1.x"` (for structured error handling)
        - `serde = { version = "1.0", features = ["derive"] }` (for types exposed in API responses)
        - `derive_builder = "0.20"` (for `HozonBuilder`)
        - `num_cpus = "1.18"` (to get CPU count for concurrency limits)

### 2. `hozon/src/error.rs`

- **Action**: Create this file.
- **Content**: Extract and adapt relevant error variants from `palaxy::src::prelude::Error`.
    - `#[derive(Debug, thiserror::Error)]`
    - `pub enum HozonError {`
    - `#[error(transparent)] Io(#[from] std::io::Error),`
    - `#[error(transparent)] Regex(#[from] regex::Error),`
    - `#[error(transparent)] Image(#[from] image::ImageError),`
    - `#[error(transparent)] Epub(#[from] epub_builder::Error),`
    - `#[error(transparent)] Zip(#[from] zip::result::ZipError),`
    - `#[error("The given path '{0}' is invalid: {1}")] InvalidPath(PathBuf, String),`
    - `#[error("Asynchronous task failed: {0}")] AsyncTaskError(String),`
    - `#[error("Unsupported: {0}")] Unsupported(String),`
    - `#[error("Not found: {0}")] NotFound(String),`
    - `#[error("Other error: {0}")] Other(String),`
    - `}`
- **Type Alias**: Define `pub type HozonResult<T> = Result<T, HozonError>;`
- **`From<String>` impl**: Add `impl From<String> for HozonError { fn from(error: String) -> Self { HozonError::Other(error) } }`
- **No custom `serde::Serialize`**: As discussed, for simplicity, no custom `serde::Serialize` impl for `HozonError` is needed if not directly exposed to a JS frontend.

### 3. `hozon/src/types.rs`

- **Action**: Create this file.
- **Content**: Extract and adapt data structures from `palaxy::src/types.rs` and `palaxy::src/prelude.rs`.
    - `#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)] pub enum BundleFlag { ... }`
    - `#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)] pub enum FileFormat { ... }`
    - `#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy, Default)] pub enum Direction { ... }`
    - `#[derive(Debug, Clone, Default, Serialize, Deserialize)] pub struct ConversionConfig { ... }` (as defined in the `Core Types` section of the `README`).
    - `#[derive(Debug, Clone, Default, Serialize, Deserialize)] pub struct BaseResponse<T = ()> { ... }`
    - `#[derive(Debug, Clone, Default, Serialize, Deserialize)] pub struct BundleResponse { ... }`
    - `pub type CommBundle = BaseResponse<BundleResponse>;`
    - `#[derive(Debug, Clone, Default, Serialize, Deserialize)] pub struct AnalyzeResponse { ... }`
    - `pub type CommAnalyzeMeta = BaseResponse<AnalyzeResponse>;`
- **Utility Functions**: Move `pub fn get_file_info(image_path: &PathBuf) -> HozonResult<(&'static str, &'static str)>` into this module. Adjust `Error` to `HozonError`.

### 4. `hozon/src/collector.rs`

- **Action**: Copy the entire content of `palaxy::src/collector.rs`.
- **Adjustments**:
    - Replace `use crate::prelude::*;` with explicit imports: `use std::path::PathBuf; use std::cmp::Ordering; use std::ffi::OsStr; use image::{DynamicImage, GenericImageView, Pixel}; use lazy_static::lazy_static; use rayon::prelude::*; use regex::Regex; use tokio::fs::{read_dir, ReadDir}; use tokio::sync::Semaphore; use tauri::async_runtime::{spawn, spawn_blocking, JoinHandle}; use log::{debug, error, info, trace, warn};`
    - Import `HozonError` and `HozonResult` from `crate::error`.
    - Import `BundleFlag` from `crate::types`.
    - Ensure all `EResult` are changed to `HozonResult`.
    - Ensure all `Error::` variants are changed to `HozonError::`.
    - `Collector::new` will take `base_directory: &PathBuf`.

### 5. `hozon/src/generator/`

- **Action**: Copy the entire contents of `palaxy::src/generator/mod.rs`, `cbz.rs`, `epub.rs`.
- **Adjustments in `mod.rs`**:
    - Replace `use crate::prelude::*;` with `use crate::error::HozonResult; use std::path::PathBuf; use async_trait::async_trait;`
    - Update `EResult` to `HozonResult`.
- **Adjustments in `cbz.rs` and `epub.rs`**:
    - Replace `use crate::prelude::*;` with `use crate::error::{HozonResult, HozonError}; use crate::types::{Direction, FileFormat, get_file_info}; use std::fs::File; use std::io::Write; use std::path::{Path, PathBuf}; use tokio::task::spawn_blocking; use tokio::fs; use zip::write::SimpleFileOptions; use zip::{CompressionMethod, ZipWriter}; use async_trait::async_trait; use log::{debug, error, info, trace, warn};` (and `epub_builder::{EpubBuilder, EpubContent, EpubVersion, ZipLibrary}; use memmap2::MmapOptions; use std::io::Cursor;` for `epub.rs`). Note `tauri::async_runtime::spawn_blocking` is changed to `tokio::task::spawn_blocking` as `tauri` is not a dependency of `hozon`.
    - Update `EResult` to `HozonResult`.
    - Update `Error::from(e)` or other `Error::` variants to `HozonError::from(e)` or `HozonError::` variants.
    - The `Generator::new` functions in `cbz.rs` and `epub.rs` should accept `output_path: &Path, filename: &str` and return `HozonResult<Self>`.

### 6. `hozon/src/pipeline.rs` (New Core Pipeline Logic)

- **Action**: Create this file. This will hold the `Hozon` (publicly aliased for `HozonConversionPipeline`) struct and `HozonBuilder` struct, containing the enhanced builder-pattern logic as described above.
- **Content**: (See `Revised Powerful Builder/Pipeline Structure` in my thinking process above for the full code. It's too long to repeat here, but it defines `HozonConversionPipelineBuilder` and `HozonConversionPipeline` with the `analyze`, `with_bundle_flag`, `bundle`, `with_edited_data`, `convert` methods.)
    - Ensure all necessary `use` statements are added:

        ```rust
        use derive_builder::Builder;
        use std::path::{Path, PathBuf};
        use tokio::fs::create_dir;
        use std::sync::Arc;
        use tokio::sync::Semaphore;
        use num_cpus;
        use log::{debug, info, error, trace, warn};

        use crate::error::{HozonError, HozonResult};
        use crate::types::{
            ConversionConfig, BundleFlag, FileFormat, Direction,
            AnalyzeResponse, BundleResponse, BaseResponse, CommAnalyzeMeta, CommBundle,
        };
        use crate::collector::Collector;
        use crate::generator::{Generator, cbz::Cbz, epub::EPub};
        ```

    - **Key changes from previous plan**: `HozonConversionPipeline` (aliased as `Hozon`) methods now return `Self` for chaining, store `analysis_results` and `bundle_results` internally, and `convert` consumes `self`.

### 7. `hozon/src/lib.rs`

- **Action**: Create this file.
- **Content**:
    - Define `mod pipeline;`, `mod collector;`, `mod generator;`, `mod error;`, `mod types;`.
    - Re-export core types and the `Hozon` builder for convenience:

        ```rust
        //! Hozon - Image-to-Ebook Conversion Library
        //!
        //! This crate provides functionality to convert image-based content
        //! into ebook formats like CBZ and EPUB.

        pub mod pipeline;
        pub mod collector;
        pub mod generator;
        pub mod error;
        pub mod types;

        // Publicly expose the main pipeline as `Hozon`
        pub use pipeline::HozonConversionPipeline as Hozon;
        pub use pipeline::HozonConversionPipelineBuilder as HozonBuilder; // Expose the builder too

        pub use error::{HozonError, HozonResult};
        pub use types::{
            ConversionConfig, BundleFlag, FileFormat, Direction,
            AnalyzeResponse, BundleResponse, BaseResponse, CommAnalyzeMeta, CommBundle,
        };

        /// Prelude module for convenient imports.
        ///
        /// This module re-exports the most commonly used types and traits, allowing you to
        /// import everything you need with a single `use hozon::prelude::*;` statement.
        pub mod prelude {
            pub use super::{
                Hozon, HozonBuilder, HozonError, HozonResult,
                ConversionConfig, BundleFlag, FileFormat, Direction,
                AnalyzeResponse, BundleResponse, BaseResponse, CommAnalyzeMeta, CommBundle,
            };
            pub use std::path::{PathBuf, Path}; // Common path types
        }
        ```

### 8. `hozon/tests/`

- **Action**: Create `tests/unit.rs` and `tests/integration.rs`, and a `tests/README.md`.
- **Content**:
    - `tests/unit.rs`: Write tests for `Collector` functions (e.g., `collect_parallel`, `check_path`, `sort_by_stem_number`), `Generator` trait implementation (mock file system or small in-memory tests), `HozonBuilder` validation, `HozonError` handling.
    - `tests/integration.rs`: Write end-to-end tests for the `Hozon` pipeline's fluent API:
        - `Hozon::builder().source().target().name().build().unwrap().analyze().await?.bundle().await?.convert().await?;`
        - Test `with_bundle_flag` and `with_edited_data` insertions.
        - These will require creating dummy image directories.
    - `tests/README.md`: Document the test structure and how to run tests, mirroring `Tosho`'s `tests/README.md`.
