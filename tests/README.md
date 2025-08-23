# Hozon Tests

This directory contains comprehensive tests for the Hozon image-to-ebook conversion library.

## Test Structure

### Unit Tests (`unit.rs`)

- Focuses on individual components in isolation.
- Tests `HozonBuilder` validation and initial state setup.
- Verifies `Collector` functions: `regex_parser`, `is_grayscale`, `sort_name_by_number_default`, `sort_by_name_volume_chapter_default`.
- Checks internal data structures and error handling in isolation.
- **Run with**: `cargo test --test unit`

### Integration Tests (`integration.rs`)

- Performs end-to-end tests of the `Hozon` conversion pipeline.
- Creates temporary dummy file structures to simulate various input scenarios.
- Tests different `VolumeGroupingStrategy` options (`Manual`, `Name`, `ImageAnalysis`, `Flat`).
- Verifies custom regex for sorting and `CollectionDepth` behavior.
- Asserts correct metadata propagation into generated CBZ/EPUB files.
- Includes tests for error paths within the pipeline (e.g., non-existent source, missing data).
- **Run with**: `cargo test --test integration`

## Running Tests

### All Tests

```bash
cargo test
```
