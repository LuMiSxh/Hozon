## My Arguments & Proposals for `Hozon` - Iteration 3

Your points are spot-on, particularly regarding the need for more granular control over various aspects of the conversion, the metadata, and the structure. Here's my detailed response and the refined plan for `Hozon`.

### 1. Renaming `BundleReport`

**Your Argument**: `BundleReport` no longer fits.
**My Proposal**: I completely agree. The term "Bundle" implies the action, not the structural outcome.

- **Rename `BundleReport` to `VolumeStructureReport`**: This clearly indicates it's a report about how the content was organized into volumes.

### 2. Converting a Single "Chapter" or "Flat Pages"

**Your Argument**: Users often just want to make one book from a flat folder of images, or convert a single pre-defined chapter without complex bundling. The current pipeline forces too many steps.
**My Proposal**: This is crucial. We need to simplify the API for these common cases while retaining power for complex ones.

- **`VolumeGroupingStrategy::Flat`**: As proposed, this explicitly signals to treat all collected pages as a single logical "chapter" within a single "volume" for output. This simplifies the user's mental model significantly.
- **Enhanced `HozonBuilder` for Direct Data Input**:
    - `HozonBuilder::with_flat_pages(Vec<PathBuf>)`: This is the direct answer. User provides a list of image paths. When `build()` is called, `HozonState` will internalize this as `collected_chapters_pages = Some(vec![flat_pages])` and automatically set `current_grouping_strategy = VolumeGroupingStrategy::Flat`. This allows immediate `hozon.convert().await?` if other configs are met.
    - `HozonBuilder::with_chapters_and_pages(Vec<Vec<PathBuf>>)`: For when the user has already pre-organized into chapters. This bypasses the `analyze()` step. The user can then choose to `bundle()` these chapters into volumes or set `VolumeGroupingStrategy::Flat` to throw all these chapters into a single output book.
- **Smarter `Hozon::convert()`**:
    - If `hozon.state.volume_structures` is `None` (meaning `bundle()` was skipped) AND `hozon.state.current_grouping_strategy == VolumeGroupingStrategy::Flat`, then `convert()` will internally construct a single "virtual" volume from `hozon.state.collected_chapters_pages` for the generator. This makes `bundle()` truly optional for flat conversions.
    - Otherwise, `convert()` will use `hozon.state.volume_structures` if it exists.

### 3. Custom Regex for Sorting

**Your Argument**: Hardcoded regexes for naming conventions are too restrictive. Users have diverse file naming.
**My Proposal**: This is an excellent point for flexibility and a must-have for a powerful library.

- **`HozonState` to hold `Option<regex::Regex>`**: Instead of `Option<String>`, `HozonState` will now directly store compiled `Option<regex::Regex>` instances for chapter and page name parsing. This validates the regex at `HozonBuilder::build()` time.
- **`HozonBuilder` methods for regex**:
    - `HozonBuilder::with_chapter_name_regex(impl Into<String>)`: Takes a string, compiles it. Fails `build()` if regex is invalid.
    - `HozonBuilder::with_page_name_regex(impl Into<String>)`: Same for page names.
- **`Collector` to utilize `HozonState`'s Regexes**: The `Collector` will take these compiled `Regex` objects from `HozonState`, using internal defaults only if no custom one is provided.
- **Sorter Hierarchy**: This is crucial for clarity:
    1.  Explicit `custom_chapter_path_sorter` / `custom_page_path_sorter` (closure).
    2.  `VolumeGroupingStrategy::Name` relies on `custom_chapter_name_regex` (or default).
    3.  `VolumeGroupingStrategy::ImageAnalysis` relies on `custom_page_name_regex` for initial ordering, but main logic is image-based.

### 4. Customizing Collection Depth (e.g., 1 folder deep vs. 2 folders deep)

**Your Argument**: `Collector`'s assumed directory structure (`source_path/chapter/page`) is not always accurate.
**My Proposal**: This is a great feature for adapting to varying source material organization.

- **New `CollectionDepth` Enum**:
    ```rust
    #[derive(Debug, PartialEq, Clone, Copy, Default, Serialize, Deserialize)]
    pub enum CollectionDepth {
        #[default]
        Deep,    // source_path/Chapter_X/page.jpg (default)
        Shallow, // source_path/page.jpg (all pages in root, treated as one virtual chapter)
    }
    ```
- **`HozonBuilder::with_collection_depth(CollectionDepth)`**: For user configuration.
- **`Collector` adapts**:
    - `Collector`'s `new` method will take `CollectionDepth`.
    - `collect_chapters()` and `collect_pages()` methods will adjust their behavior based on `self.collection_depth`.
        - If `Shallow`: `collect_chapters` yields `vec![self.base_directory.clone()]` (the root as one conceptual chapter). `collect_pages` lists all images directly within `self.base_directory`.
        - If `Deep`: Behaves as current (scans for subdirectories as chapters).

### 5. Customizing Grayscale Sensibility

**Your Argument**: The `sensibility` for `ImageAnalysis` should be customizable.
**My Proposal**: It's already in the plan!

- `ConversionConfig` holds `image_analysis_sensibility: u8` (0-100%).
- `HozonBuilder::with_image_analysis_sensibility(u8)`.
- `Collector::new` takes this `u8` and converts to `f64` for `is_grayscale`.

### 6. Customizing Metadata (EPUB/CBZ)

**Your Argument**: Hardcoded metadata is insufficient. Need robust control over titles, authors, descriptions, etc., and custom fields.
**My Proposal**: Absolutely essential for quality output. This requires a dedicated, rich `EbookMetadata` struct.

- **`EbookMetadata` Struct**: Will hold comprehensive fields (title, series, author, publisher, description, tags, language, rights, identifier, release date) and specific fields for `ComicInfo.xml` (genre, age rating, scan info, web) as well as a `HashMap<String, String>` for arbitrary custom fields.
- **`ConversionConfig` includes `EbookMetadata`**: This struct will be part of the core `ConversionConfig` initialized by the builder.
- **`HozonBuilder` for Metadata**:
    - `HozonBuilder::with_metadata(EbookMetadata)`: Allows a full struct override.
    - Individual setters for common fields (`with_title`, `with_author`, `with_description`, `with_language`, `with_tags`, `with_genre`, etc.). These will populate the `EbookMetadata` within `ConversionConfig`.
- **`Generator` Trait Update**: `set_metadata` will take:
    - `file_name_base: &str`: The base file name (e.g., "One Piece | Volume 1")
    - `file_volume_number: Option<usize>`: The specific volume number for _this output file_.
    - `series_metadata: &EbookMetadata`: The full series-level metadata.
    - `total_pages_in_file: usize`: Auto-calculated by `Hozon::convert()`.
    - `collected_chapter_titles: &[String]`: A list of titles of chapters included in this specific volume. (For EPUB, these can be chapter navigation titles; for CBZ, they could go into ComicInfo.xml if desired).

This allows `Hozon::convert()` to dynamically create the specific title and volume number for each output file (e.g., "My Series | Vol 3") while drawing all other rich metadata from the `series_metadata` struct provided by the user.

---

## Re-Revised Module Structure for `Hozon` (Finalized)

```
└── hozon/
    ├── src
    │   ├── lib.rs             # Crate root, re-exports
    │   ├── hozon.rs           # The main `Hozon` struct, its builder, and core logic (analyze, bundle, convert)
    │   ├── collector.rs       # Image collection and analysis, now accepting custom regex/depth/sensibility
    │   ├── generator/         # Ebook generation (CBZ, EPUB)
    │   │   ├── mod.rs
    │   │   ├── cbz.rs
    │   │   └── epub.rs
    │   ├── error.rs           # Custom error types (`HozonError`, `HozonResult`)
    │   └── types.rs           # Enums (`VolumeGroupingStrategy`, `FileFormat`, `Direction`, `CollectionDepth`, `AnalyzeFinding`), `EbookMetadata`, `ConversionConfig`, `HozonState`, `AnalyzeReport`, `VolumeStructureReport`
    ├── tests/
    │   ├── unit.rs
    │   ├── integration.rs
    │   └── README.md
    ├── .gitignore
    ├── Cargo.toml
    └── README.md
```

---

## Detailed Implementation Plan (Argumentative Style)

### **1. `Cargo.toml`**

- **Argument**: We need `chrono` for metadata date parsing.
- **Action**: Add `chrono = { version = "0.4", features = ["serde", "std"] }` as a direct dependency.
- **Argument**: Regex compilation needs `regex`.
- **Action**: Ensure `regex = "1.x"` is present.

### **2. `src/error.rs`**

- **Argument**: No changes needed; `HozonError` is already granular enough for a library.

### **3. `src/types.rs`**

- **Argument**: Renaming and adding flexibility.
- **Action**:
    - Rename `BundleFlag` to `VolumeGroupingStrategy`, add `Flat` variant.
    - Rename `BundleReport` to `VolumeStructureReport`.
    - Introduce `CollectionDepth` enum (Deep/Shallow).
    - Define `EbookMetadata` struct with comprehensive fields, `default_with_title` impl.
    - Update `ConversionConfig` to include `CollectionDepth` and `EbookMetadata`, `image_analysis_sensibility` (u8), and a `new_with_name_and_paths` constructor.
    - Update `AnalyzeFinding` enum with more specific findings (e.g., `MissingNumericIdentifier(PathBuf)` instead of just `String`).
    - Update `AnalyzeReport` struct to use `Vec<AnalyzeFinding>` for its `findings` field.
- **Argument**: `HozonState` needs to be the central evolving configuration.
- **Action**:
    - Update `HozonState` to include `custom_chapter_name_regex: Option<regex::Regex>`, `custom_page_name_regex: Option<regex::Regex>`, and `volume_sizes_override: Option<Vec<usize>>`.

### **4. `src/collector.rs`**

- **Argument**: The `Collector` must be configurable at instantiation, not hardcoded.
- **Action**:
    - Modify `Collector::new()` to accept `collection_depth: CollectionDepth`, `chapter_name_regex: Option<&'a regex::Regex>`, `page_name_regex: Option<&'a regex::Regex>`, and `image_analysis_sensibility_u8: u8`. Store these as fields.
    - Implement `lazy_static!` for `DEFAULT_CHAPTER_REGEX` and `DEFAULT_PAGE_REGEX`.
    - Adjust `collect_chapters` and `collect_pages` methods to respect `self.collection_depth`.
    - Update `regex_parser` to use `self.chapter_name_regex` or `self.page_name_regex`, falling back to defaults.
    - `determine_volume_start_chapters` will use `self.image_analysis_sensibility` directly (converted to `f64`).
    - Modify `AnalyzeFinding` conversion in `check_path` and similar places.

### **5. `src/generator/mod.rs`**

- **Argument**: The `Generator` trait needs a robust `set_metadata` signature.
- **Action**: Update `Generator::set_metadata` to accept `file_name_base: &str`, `file_volume_number: Option<usize>`, `series_metadata: &EbookMetadata`, `total_pages_in_file: usize`, and `collected_chapter_titles: &[String]`.

### **6. `src/generator/cbz.rs`**

- **Argument**: `ComicInfo.xml` should be fully populated and customizable.
- **Action**:
    - Implement `set_metadata` using the new `EbookMetadata` fields and `file_name_base`, `file_volume_number`, `total_pages_in_file`.
    - Expand the `ComicInfo.xml` template with placeholders for all `EbookMetadata` fields.
    - Use `chrono` for parsing `release_date` for `Year`, `Month`, `Day` tags.
    - Include `collected_chapter_titles` in ComicInfo.xml under a custom tag if desired (or as comments).

### **7. `src/generator/epub.rs`**

- **Argument**: EPUB metadata should fully leverage `EbookMetadata`.
- **Action**:
    - Implement `set_metadata` using `epub_builder`'s methods to set language, title, creator, description, publisher, date, identifier, rights, and subjects (from tags).
    - Use `file_name_base` as the primary EPUB title.
    - Use `collected_chapter_titles` to create the EPUB's Table of Contents or chapter navigation.

### **8. `src/hozon.rs` (The Orchestrator)**

- **Argument**: The `Hozon` struct should be the fluent pipeline, and the builder should enable all initial configurations.
- **Action**:
    - **`HozonBuilder`**:
        - Remove redundant `name` field; constructor `ConversionConfig::new_with_name_and_paths` handles initial name.
        - Add `with_collection_depth(CollectionDepth)`.
        - Add `with_image_analysis_sensibility(u8)`.
        - Add `with_chapter_name_regex(impl Into<String>)` and `with_page_name_regex(impl Into<String>)`. Handle `regex::Error` during `build()`.
        - Add `with_volume_sizes_override(Vec<usize>)`.
        - **Implement `with_flat_pages(Vec<PathBuf>)`**: Internally sets `collected_chapters_pages` and `current_grouping_strategy = VolumeGroupingStrategy::Flat`.
        - **Implement `with_chapters_and_pages(Vec<Vec<PathBuf>>)`**: Internally sets `collected_chapters_pages`.
        - Implement builder methods for all `EbookMetadata` fields (e.g., `with_author`, `with_description`, `with_language`). These will update the `EbookMetadata` inside `ConversionConfig`.
    - **`HozonBuilder::build()`**:
        - Create `ConversionConfig` using `new_with_name_and_paths`.
        - Apply `with_metadata` override first, then individual metadata setters.
        - Compile custom regex strings into `regex::Regex` objects.
        - Perform comprehensive validation.
        - Initialize `HozonState` fields from builder. Correctly set `collected_chapters_pages` and `volume_structures` based on initial `with_` data inputs, ensuring internal consistency.
    - **`Hozon` methods**:
        - **`analyze()`**:
            - Create `Collector` with all `HozonState`'s configurable fields (depth, regexes, sensibility, sorters).
            - Populate `self.state.collected_chapters_pages` and `self.state.analysis_report`.
            - Extract `recommended_strategy` from `analysis_report` and set `self.state.current_grouping_strategy`.
        - **`bundle()`**:
            - Retrieve `collected_chapters_pages` (or `edited_chapters_pages_override`).
            - Handle `VolumeGroupingStrategy::Flat` separately: create a single virtual volume directly from `collected_chapters_pages`, skipping complex logic.
            - For `VolumeGroupingStrategy::Manual`, use `self.state.volume_sizes_override` if present.
            - Create `Collector` with all `HozonState`'s configurable fields.
            - Populate `self.state.volume_structures` and `self.state.volume_structure_report`.
        - **`convert()`**:
            - Determine final `volume_structures` to use:
                - If `VolumeGroupingStrategy::Flat` is active AND `self.state.volume_structures` is `None`, dynamically construct a single volume from `self.state.collected_chapters_pages`.
                - Otherwise, use `self.state.volume_structures`.
            - Iterate through determined `volume_structures`. For each, calculate:
                - `file_name_base` (e.g., "My Series | Volume X").
                - `file_volume_number` (e.g., X).
                - `total_pages_in_file`.
                - `collected_chapter_titles` (from `PathBuf` names within the volume structure).
            - Create `Generator` with `target_dir` and `file_name_base`.
            - Call `generator.set_metadata(file_name_base, file_volume_number, &config.metadata, total_pages_in_file, &collected_chapter_titles)`.

### **9. `src/lib.rs`**

- **Argument**: The prelude should expose all new types for ease of use.
- **Action**: Update re-exports for `VolumeGroupingStrategy`, `VolumeStructureReport`, `CollectionDepth`, `AnalyzeFinding`, `EbookMetadata`, `ConversionConfig`, `HozonState`.

### **10. `tests/`**

- **Argument**: New features require extensive testing for robustness and correctness.
- **Action**:
    - Add tests for all new `HozonBuilder` methods, especially `with_flat_pages`, `with_chapters_and_pages`.
    - Test different `CollectionDepth` values with dummy folder structures.
    - Test custom regex compilation and their effect on sorting.
    - Test `VolumeGroupingStrategy::Flat` end-to-end.
    - Test rich `EbookMetadata` propagation through `Hozon` and into `Cbz`/`EPub` generators, asserting content of generated files (e.g., `ComicInfo.xml` or EPUB `content.opf`).
    - Test `image_analysis_sensibility`.
    - Ensure `AnalyzeReport` and `VolumeStructureReport` contain expected data for different scenarios.

This comprehensive re-argumentation and plan tackles all the complexities, pushing `Hozon` towards a highly flexible, powerful, and developer-friendly conversion library.
