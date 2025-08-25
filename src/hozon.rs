use num_cpus;
use rayon::prelude::*;
use regex::Regex;
use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Semaphore;

use crate::collector::{Collector, DEFAULT_NAME_GROUPING_REGEX};
use crate::error::{Error, Result};
use crate::generator::{Generator, cbz::Cbz, epub::EPub};
use crate::types::{
    CollectedContent, CollectionDepth, Direction, EbookMetadata, FileFormat, HozonExecutionMode,
    StructuredContent, VolumeGroupingStrategy, VolumeStructureReport,
};

/// The main Hozon conversion configuration, built declaratively using the builder pattern.
///
/// This struct encapsulates all settings needed for image-to-ebook conversion, including
/// source and target paths, metadata, formatting options, and advanced analysis settings.
/// Once configured, it can execute the conversion process using various entry points:
///
/// - [`convert_from_source`](HozonConfig::convert_from_source): Full pipeline from directory scanning
/// - [`convert_from_collected_data`](HozonConfig::convert_from_collected_data): From pre-collected chapter/page data
/// - [`convert_from_structured_data`](HozonConfig::convert_from_structured_data): From pre-structured volume data
/// - [`analyze_source`](HozonConfig::analyze_source): Analysis only, no conversion
///
/// ## Builder Pattern
///
/// Use [`HozonConfig::builder()`](HozonConfig::builder) to create a new configuration:
///
/// ```rust,no_run
/// # use hozon::prelude::*;
/// # use std::path::PathBuf;
/// let config = HozonConfig::builder()
///     .metadata(EbookMetadata::default_with_title("My Book".to_string()))
///     .source_path(PathBuf::from("./source"))
///     .target_path(PathBuf::from("./output"))
///     .output_format(FileFormat::Cbz)
///     .build()
///     .expect("Invalid configuration");
/// ```
#[derive(Clone, derive_builder::Builder)]
#[builder(setter(into, strip_option), build_fn(validate = "Self::validate"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HozonConfig {
    // --- Core Conversion Settings ---
    /// Complete ebook metadata including title, authors, description, and custom fields.
    ///
    /// This metadata will be embedded in the generated ebook files and used for
    /// ComicInfo.xml (CBZ) or EPUB metadata. Use [`EbookMetadata::default_with_title`]
    /// for quick setup with just a title.
    #[builder(default = "EbookMetadata::default_with_title(\"Untitled Conversion\".to_string())")]
    pub metadata: EbookMetadata,

    /// Source directory path containing image files to convert.
    ///
    /// Required for [`convert_from_source`](HozonConfig::convert_from_source) and
    /// [`analyze_source`](HozonConfig::analyze_source) methods. The directory structure
    /// depends on the [`collection_depth`](HozonConfig::collection_depth) setting.
    #[builder(default)]
    pub source_path: PathBuf,

    /// Target directory where generated ebook files will be saved.
    ///
    /// If [`create_output_directory`](HozonConfig::create_output_directory) is true,
    /// a subdirectory named after the ebook title will be created here.
    pub target_path: PathBuf,

    /// Output file format for generated ebooks.
    ///
    /// - [`FileFormat::Cbz`]: Comic Book Archive (ZIP-based) with ComicInfo.xml metadata
    /// - [`FileFormat::Epub`]: EPUB format with full metadata and reading direction support
    #[builder(default = "FileFormat::Cbz")]
    pub output_format: FileFormat,

    /// Reading direction for EPUB files.
    ///
    /// - [`Direction::Ltr`]: Left-to-right reading (Western style)
    /// - [`Direction::Rtl`]: Right-to-left reading (manga/Arabic style)
    ///
    /// This setting only affects EPUB output and is ignored for CBZ files.
    #[builder(default = "Direction::Ltr")]
    pub reading_direction: Direction,

    /// Whether to create a subdirectory in the target path named after the ebook title.
    ///
    /// If `true`, output files will be saved to `target_path/ebook_title/`.
    /// If `false`, output files will be saved directly to `target_path/`.
    #[builder(default = "true")]
    pub create_output_directory: bool,

    /// Directory scanning depth for collecting chapters and pages.
    ///
    /// - [`CollectionDepth::Deep`]: Expects `source/chapter/page.jpg` structure
    /// - [`CollectionDepth::Shallow`]: Expects `source/page.jpg` structure (single chapter)
    #[builder(default = "CollectionDepth::Deep")]
    pub collection_depth: CollectionDepth,

    /// Sensitivity for image-based analysis (0-100%).
    ///
    /// Higher values mean stricter requirements for detecting grayscale "cover" pages
    /// when using [`VolumeGroupingStrategy::ImageAnalysis`]. A value of 90 means 90%
    /// of pixels must be grayscale for a page to be considered a volume break.
    #[builder(default = "75")]
    pub image_analysis_sensibility: u8,

    // --- Customization for Collection & Structuring Logic ---
    /// Strategy for grouping chapters into logical volumes.
    ///
    /// - [`VolumeGroupingStrategy::Name`]: Groups by chapter name patterns (e.g., "Vol1-Ch01")
    /// - [`VolumeGroupingStrategy::ImageAnalysis`]: Detects volume breaks using cover page analysis
    /// - [`VolumeGroupingStrategy::Manual`]: Uses explicit sizes or single volume
    /// - [`VolumeGroupingStrategy::Flat`]: All pages in one chapter, one volume
    #[builder(default = "VolumeGroupingStrategy::Manual")]
    pub volume_grouping_strategy: VolumeGroupingStrategy,

    /// Custom regex pattern for extracting chapter numbers from directory names.
    ///
    /// If not provided, uses the default pattern that matches common numbering schemes
    /// like "Chapter 01", "Ch_001", "01-Chapter Title", etc.
    ///
    /// Example: `r"Chapter\s*(\d+(?:\.\d+)?)"` to match "Chapter 1", "Chapter 2.5"
    #[builder(default)]
    pub chapter_name_regex_str: Option<String>,

    /// Custom regex pattern for extracting page numbers from file names.
    ///
    /// If not provided, uses the default pattern that matches common numbering schemes
    /// like "001.jpg", "page_001.png", "p01.webp", etc.
    ///
    /// Example: `r"page[\s_-]*(\d+(?:\.\d+)?)"` to match "page_001", "page-01"
    #[builder(default)]
    pub page_name_regex_str: Option<String>,

    /// Custom sorting function for chapter directories.
    ///
    /// Provides full control over chapter ordering. If not provided, uses the default
    /// numeric sorting based on extracted numbers from directory names.
    ///
    /// The function receives two `PathBuf` references and returns a [`std::cmp::Ordering`].
    #[builder(default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub custom_chapter_path_sorter:
        Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Sync + Send + 'static>>,

    /// Custom sorting function for page files within chapters.
    ///
    /// Provides full control over page ordering within each chapter. If not provided,
    /// uses the default numeric sorting based on extracted numbers from file names.
    ///
    /// The function receives two `PathBuf` references and returns a [`std::cmp::Ordering`].
    #[builder(default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub custom_page_path_sorter:
        Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Sync + Send + 'static>>,

    /// Explicit volume sizes for [`VolumeGroupingStrategy::Manual`].
    ///
    /// Specifies how many chapters should be in each volume. For example, `vec![10, 8, 5]`
    /// creates 3 volumes with 10, 8, and 5 chapters respectively. If empty or the total
    /// is less than available chapters, remaining chapters go into a single volume.
    ///
    /// Only used when `volume_grouping_strategy` is [`VolumeGroupingStrategy::Manual`].
    #[builder(default)]
    pub volume_sizes_override: Vec<usize>,

    // --- Internal Fields (Auto-Generated, Hidden from Builder) ---
    // Note: These are compiled from the above regex strings in the builder's validate() method.
    /// Compiled regex from `chapter_name_regex_str`. Internal use only.
    #[builder(setter(skip), default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub(crate) compiled_chapter_name_regex: Option<Regex>,

    /// Compiled regex from `page_name_regex_str`. Internal use only.
    #[builder(setter(skip), default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub(crate) compiled_page_name_regex: Option<Regex>,
}
impl std::fmt::Debug for HozonConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HozonConfig")
            .field("metadata", &self.metadata)
            .field("source_path", &self.source_path)
            .field("target_path", &self.target_path)
            .field("output_format", &self.output_format)
            .field("reading_direction", &self.reading_direction)
            .field("create_output_directory", &self.create_output_directory)
            .field("collection_depth", &self.collection_depth)
            .field(
                "image_analysis_sensibility",
                &self.image_analysis_sensibility,
            )
            .field("volume_grouping_strategy", &self.volume_grouping_strategy)
            .field("chapter_name_regex_str", &self.chapter_name_regex_str)
            .field("page_name_regex_str", &self.page_name_regex_str)
            .field(
                "custom_chapter_path_sorter",
                if self.custom_chapter_path_sorter.is_some() {
                    &"Some(Function)"
                } else {
                    &"None"
                },
            )
            .field(
                "custom_page_path_sorter",
                if self.custom_page_path_sorter.is_some() {
                    &"Some(Function)"
                } else {
                    &"None"
                },
            )
            .field("volume_sizes_override", &self.volume_sizes_override)
            // Skip compiled regexes in debug output
            .finish()
    }
}

impl HozonConfig {
    /// Creates a new builder for configuring `HozonConfig`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("My Book".to_string()))
    ///     .source_path(PathBuf::from("./source"))
    ///     .target_path(PathBuf::from("./output"))
    ///     .build()
    ///     .expect("Invalid configuration");
    /// ```
    pub fn builder() -> HozonConfigBuilder {
        HozonConfigBuilder::default()
    }

    /// Performs validation checks on the configuration for a specific execution mode.
    ///
    /// This method validates the configuration without performing any file operations or content loading.
    /// It's useful for early validation before starting conversion operations. All `convert_from_*` methods
    /// call this automatically, so manual invocation is optional but recommended for early error detection.
    ///
    /// # Arguments
    ///
    /// * `mode` - The intended execution mode, which determines which validation checks are performed:
    ///   - [`HozonExecutionMode::FromSource`]: Validates `source_path` existence and accessibility
    ///   - [`HozonExecutionMode::FromCollectedData`]: Validates target path settings
    ///   - [`HozonExecutionMode::FromStructuredData`]: Validates target path and metadata
    ///
    /// # Returns
    ///
    /// * `Ok(&self)` - Configuration is valid for the specified mode
    /// * `Err(Error)` - Configuration has validation errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # fn main() -> hozon::error::Result<()> {
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("Test".to_string()))
    ///     .source_path(PathBuf::from("./source"))
    ///     .target_path(PathBuf::from("./output"))
    ///     .build()?;
    ///
    /// // Validate before conversion
    /// config.preflight_check(HozonExecutionMode::FromSource)?;
    /// println!("Configuration is valid!");
    /// # Ok(())
    /// # }
    /// ```
    pub fn preflight_check(&self, mode: HozonExecutionMode) -> Result<&Self> {
        // --- Basic config validation (redundant with Builder::build, but good as a sanity check) ---
        if self.metadata.title.is_empty() {
            return Err(Error::Other("Ebook title is required".to_string()));
        }
        if self.target_path.as_os_str().is_empty() {
            return Err(Error::Other("Target path is required".to_string()));
        }
        if self.image_analysis_sensibility > 100 {
            return Err(Error::Other(
                "Image analysis sensibility must be between 0 and 100.".to_string(),
            ));
        }
        // Compiled regexes are already validated during build.

        // --- Mode-specific checks ---
        match mode {
            HozonExecutionMode::FromSource => {
                if self.source_path.as_os_str().is_empty() {
                    return Err(Error::Other(
                        "`source_path` must be set for `FromSource` execution mode.".to_string(),
                    ));
                }
                if !self.source_path.exists() {
                    return Err(Error::NotFound(format!(
                        "Source path does not exist: {:?}",
                        self.source_path
                    )));
                }
                if !self.source_path.is_dir() {
                    return Err(Error::InvalidPath(
                        self.source_path.clone(),
                        "Source path is not a directory.".to_string(),
                    ));
                }
            }
            HozonExecutionMode::FromCollectedData => {
                // No specific config checks here related to data itself, as data is passed to `convert_from_collected_data`
                // and its emptiness would be checked there.
            }
            HozonExecutionMode::FromStructuredData => {
                // Similarly, no specific config checks related to data itself, as data is passed to `convert_from_structured_data`.
            }
        }

        Ok(self)
    }

    // --- Core conversion entry points ---

    /// Starts the conversion by collecting chapters and pages from `source_path`.
    /// This method performs the full pipeline: analysis -> structuring -> generation.
    pub async fn convert_from_source(self) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromSource)?;

        // 1. Create a collector and run the full analysis.
        let collector = Collector::new(
            &self.source_path,
            self.collection_depth,
            self.compiled_chapter_name_regex.as_ref(),
            self.compiled_page_name_regex.as_ref(),
            self.image_analysis_sensibility,
        );

        let collected_content = collector.analyze_source_content().await?;

        // Extract the collected page data from the analysis result.
        let pages_to_convert = collected_content.chapters_with_pages;

        if pages_to_convert.par_iter().all(Vec::is_empty) {
            return Err(Error::NotFound(format!(
                "No pages found in source path: {:?}",
                self.source_path
            )));
        }

        // 2. Delegate the rest of the process (structuring and generation)
        //    to the `convert_from_collected_data` method
        self.convert_from_collected_data(pages_to_convert).await
    }

    /// Analyzes the source directory based on the current configuration.
    /// This method collects all content and runs a series of checks,
    /// returning a detailed report without performing a conversion.
    pub async fn analyze_source(self) -> Result<CollectedContent> {
        self.preflight_check(HozonExecutionMode::FromSource)?;

        let collector = Collector::new(
            &self.source_path,
            self.collection_depth,
            self.compiled_chapter_name_regex.as_ref(),
            self.compiled_page_name_regex.as_ref(),
            self.image_analysis_sensibility,
        );

        collector.analyze_source_content().await
    }

    /// Executes conversion using pre-collected chapter and page data.
    ///
    /// This method bypasses the collection and analysis phases, starting directly with
    /// user-provided chapter and page paths. It performs:
    /// 1. **Validation**: Ensures the provided data is valid and accessible
    /// 2. **Structuring**: Groups chapters into logical volumes based on the configured strategy
    /// 3. **Generation**: Creates the final ebook files
    ///
    /// This is useful when you want to:
    /// - Convert a subset of chapters from a larger collection
    /// - Apply custom chapter selection or filtering logic
    /// - Work with non-standard directory structures
    /// - Integrate with external content management systems
    ///
    /// # Arguments
    ///
    /// * `collected_data` - Vector of chapters, where each chapter is a vector of page file paths.
    ///   For example: `vec![vec![page1.jpg, page2.jpg], vec![page3.jpg, page4.jpg]]`
    ///   represents 2 chapters with 2 pages each.
    ///
    /// # Requirements
    ///
    /// - `target_path` must be set to a valid output location
    /// - All provided file paths must exist and be readable
    /// - At least one chapter with one page must be provided
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Conversion completed successfully
    /// * `Err(Error)` - Conversion failed due to validation, I/O, or processing errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> hozon::error::Result<()> {
    /// let collected_data = vec![
    ///     vec![
    ///         PathBuf::from("./images/chapter1/page1.jpg"),
    ///         PathBuf::from("./images/chapter1/page2.jpg"),
    ///     ],
    ///     vec![
    ///         PathBuf::from("./images/chapter2/page1.jpg"),
    ///         PathBuf::from("./images/chapter2/page2.jpg"),
    ///     ],
    /// ];
    ///
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("Custom Collection".to_string()))
    ///     .target_path(PathBuf::from("./output"))
    ///     .volume_grouping_strategy(VolumeGroupingStrategy::Manual)
    ///     .build()?;
    ///
    /// config.convert_from_collected_data(collected_data).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_from_collected_data(
        self,
        collected_data: Vec<Vec<PathBuf>>,
    ) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromCollectedData)?;

        if collected_data.is_empty() || collected_data.iter().all(|c| c.is_empty()) {
            return Err(Error::Other(
                "Provided collected data is empty.".to_string(),
            ));
        }

        let config_ref = &self; // Reference to the configuration

        // 1. Structure Volumes
        let structured_content = Self::perform_structuring(config_ref, collected_data).await?;

        // 2. Generate Ebooks
        Self::perform_generation(
            &self,
            structured_content.volumes_with_chapters_and_pages,
            None,
        )
        .await
    }

    /// Executes conversion using pre-structured volume data.
    ///
    /// This method bypasses both the collection/analysis and structuring phases, starting
    /// directly with fully organized volume data. It performs only the generation phase,
    /// creating ebook files from the provided volume structure.
    ///
    /// This is the most direct conversion method, useful when you have:
    /// - Already organized content into logical volumes
    /// - Complex custom volume organization logic
    /// - External volume structuring systems
    /// - Need for maximum control over the final volume organization
    ///
    /// # Arguments
    ///
    /// * `structured_data` - Vector of volumes, where each volume contains chapters,
    ///   and each chapter contains page file paths. For example:
    ///   ```text
    ///   vec![
    ///       // Volume 1
    ///       vec![
    ///           vec![page1.jpg, page2.jpg], // Chapter 1
    ///           vec![page3.jpg, page4.jpg], // Chapter 2
    ///       ],
    ///       // Volume 2
    ///       vec![
    ///           vec![page5.jpg, page6.jpg], // Chapter 3
    ///       ],
    ///   ]
    ///   ```
    ///
    /// # Requirements
    ///
    /// - `target_path` must be set to a valid output location
    /// - All provided file paths must exist and be readable
    /// - At least one volume with one chapter with one page must be provided
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Conversion completed successfully
    /// * `Err(Error)` - Conversion failed due to validation, I/O, or processing errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> hozon::error::Result<()> {
    /// let structured_data = vec![
    ///     // Volume 1: Chapters 1-2
    ///     vec![
    ///         vec![PathBuf::from("./ch1/p1.jpg"), PathBuf::from("./ch1/p2.jpg")],
    ///         vec![PathBuf::from("./ch2/p1.jpg"), PathBuf::from("./ch2/p2.jpg")],
    ///     ],
    ///     // Volume 2: Chapter 3
    ///     vec![
    ///         vec![PathBuf::from("./ch3/p1.jpg"), PathBuf::from("./ch3/p2.jpg")],
    ///     ],
    /// ];
    ///
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("Pre-structured Series".to_string()))
    ///     .target_path(PathBuf::from("./output"))
    ///     .output_format(FileFormat::Epub)
    ///     .build()?;
    ///
    /// config.convert_from_structured_data(structured_data).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_from_structured_data(
        self,
        structured_data: Vec<Vec<Vec<PathBuf>>>,
    ) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromStructuredData)?;

        if structured_data.is_empty()
            || structured_data
                .iter()
                .all(|v| v.is_empty() || v.iter().all(|c| c.is_empty()))
        {
            return Err(Error::Other(
                "Provided structured data is empty.".to_string(),
            ));
        }

        // 1. Generate Ebooks
        Self::perform_generation(&self, structured_data, None).await
    }

    // --- Private helper methods for pipeline steps ---

    /// Internal method to perform the volume structuring logic.
    async fn perform_structuring(
        config: &HozonConfig,
        collected_chapters_pages: Vec<Vec<PathBuf>>,
    ) -> Result<StructuredContent> {
        let collector = Collector::new(
            &config.source_path, // Still need source_path for collector context
            config.collection_depth,
            config.compiled_chapter_name_regex.as_ref(),
            config.compiled_page_name_regex.as_ref(),
            config.image_analysis_sensibility,
        );

        let total_chapters_processed = collected_chapters_pages.len();
        let mut total_volumes_created: usize = 0;
        let mut chapter_counts_per_volume: Vec<usize> = Vec::new();
        let mut final_volume_structures: Vec<Vec<Vec<PathBuf>>> = Vec::new(); // Vec<Volume: Vec<Chapter: Vec<PagePath>>>

        match config.volume_grouping_strategy {
            VolumeGroupingStrategy::Flat => {
                if total_chapters_processed > 0 {
                    let all_pages_flat: Vec<PathBuf> =
                        collected_chapters_pages.into_iter().flatten().collect();
                    final_volume_structures.push(vec![all_pages_flat]); // One volume, one "chapter", all pages
                    total_volumes_created = 1;
                    chapter_counts_per_volume.push(total_chapters_processed); // Represents the count of original chapters if needed
                }
            }
            VolumeGroupingStrategy::Manual => {
                let chapters_for_manual_grouping = collected_chapters_pages; // This is the Vec<Vec<PathBuf>> of chapters with their pages
                let actual_total_chapters = chapters_for_manual_grouping.len();

                if !config.volume_sizes_override.is_empty() {
                    let user_volume_sizes = &config.volume_sizes_override;
                    chapter_counts_per_volume = user_volume_sizes.clone();
                    total_volumes_created = chapter_counts_per_volume.len();

                    let mut current_chapter_idx = 0;
                    for &num_chapters_in_volume in &chapter_counts_per_volume {
                        if current_chapter_idx + num_chapters_in_volume > actual_total_chapters {
                            return Err(Error::Other(format!(
                                "Manual volume sizes ({:?}) exceed available chapters ({})",
                                user_volume_sizes, actual_total_chapters
                            )));
                        }
                        final_volume_structures.push(
                            chapters_for_manual_grouping
                                [current_chapter_idx..current_chapter_idx + num_chapters_in_volume]
                                .to_vec(),
                        );
                        current_chapter_idx += num_chapters_in_volume;
                    }
                } else {
                    // Default manual: one volume containing all chapters
                    if actual_total_chapters > 0 {
                        chapter_counts_per_volume.push(actual_total_chapters);
                        total_volumes_created = 1;
                        final_volume_structures.push(chapters_for_manual_grouping);
                    }
                }
            }
            VolumeGroupingStrategy::Name => {
                // Need to reconstruct temporary chapter PathBufs for sorting by name
                let chapter_paths_for_sorting: Vec<PathBuf> = collected_chapters_pages
                    .iter()
                    .filter_map(|ch_pages| {
                        ch_pages
                            .first()
                            .and_then(|p| p.parent())
                            .map(|p| p.to_path_buf())
                    })
                    .collect();

                let mut sorted_chapter_paths = chapter_paths_for_sorting.clone();
                // Apply custom sorter if provided, otherwise default
                if let Some(sorter) = config.custom_chapter_path_sorter.as_ref() {
                    sorted_chapter_paths.par_sort_by(sorter.as_ref());
                } else {
                    sorted_chapter_paths
                        .par_sort_by(&Collector::sort_by_name_volume_chapter_default);
                }

                // Map sorted chapter paths back to the original collected_chapters_pages structure
                let sorted_collected_chapters_pages = sorted_chapter_paths
                    .into_iter()
                    .filter_map(|sorted_path| {
                        collected_chapters_pages
                            .iter()
                            .find(|ch_pages| {
                                ch_pages
                                    .first()
                                    .and_then(|p| p.parent())
                                    .map_or(false, |parent| parent == sorted_path)
                            })
                            .cloned()
                    })
                    .collect::<Vec<Vec<PathBuf>>>();

                // Now determine volume start indices based on the sorted chapter paths
                let mut volume_start_indices = Vec::new();
                if !sorted_collected_chapters_pages.is_empty() {
                    volume_start_indices.push(0); // The first chapter is always a volume start

                    for i in 1..sorted_collected_chapters_pages.len() {
                        let prev_chapter_path = sorted_collected_chapters_pages[i - 1]
                            .first()
                            .and_then(|p| p.parent());
                        let current_chapter_path = sorted_collected_chapters_pages[i]
                            .first()
                            .and_then(|p| p.parent());

                        if let (Some(prev_path), Some(current_path)) =
                            (prev_chapter_path, current_chapter_path)
                        {
                            let prev_chapter_name =
                                prev_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            let current_chapter_name = current_path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");

                            let prev_vol = DEFAULT_NAME_GROUPING_REGEX
                                .captures(prev_chapter_name)
                                .and_then(|c| c.get(0))
                                .and_then(|m| m.as_str().split('-').next())
                                .and_then(|s| s.trim_start_matches('0').parse::<f64>().ok())
                                .unwrap_or(0.0);
                            let curr_vol = DEFAULT_NAME_GROUPING_REGEX
                                .captures(current_chapter_name)
                                .and_then(|c| c.get(0))
                                .and_then(|m| m.as_str().split('-').next())
                                .and_then(|s| s.trim_start_matches('0').parse::<f64>().ok())
                                .unwrap_or(0.0);

                            if curr_vol > 0.0 && (curr_vol != prev_vol) {
                                volume_start_indices.push(i);
                            }
                        }
                    }
                }

                total_volumes_created = volume_start_indices.len();
                chapter_counts_per_volume = collector
                    .calculate_volume_sizes(volume_start_indices, total_chapters_processed)?;

                let mut current_chapter_offset = 0;
                for &num_chapters_in_vol in &chapter_counts_per_volume {
                    final_volume_structures.push(
                        sorted_collected_chapters_pages
                            [current_chapter_offset..current_chapter_offset + num_chapters_in_vol]
                            .to_vec(),
                    );
                    current_chapter_offset += num_chapters_in_vol;
                }
            }
            VolumeGroupingStrategy::ImageAnalysis => {
                let sensibility_f64 = config.image_analysis_sensibility as f64 / 100.0;

                let volume_start_indices = collector
                    .determine_volume_start_chapters(
                        collected_chapters_pages.clone(),
                        Some(sensibility_f64),
                    )
                    .await?;

                total_volumes_created = volume_start_indices.len();
                chapter_counts_per_volume = collector
                    .calculate_volume_sizes(volume_start_indices, total_chapters_processed)?;

                let mut current_chapter_offset = 0;
                for &num_chapters_in_vol in &chapter_counts_per_volume {
                    final_volume_structures.push(
                        collected_chapters_pages
                            [current_chapter_offset..current_chapter_offset + num_chapters_in_vol]
                            .to_vec(),
                    );
                    current_chapter_offset += num_chapters_in_vol;
                }
            }
        }

        Ok(StructuredContent {
            volumes_with_chapters_and_pages: final_volume_structures,
            report: VolumeStructureReport {
                total_chapters_processed,
                total_volumes_created,
                chapter_counts_per_volume,
            },
            grouping_strategy_applied: config.volume_grouping_strategy,
        })
    }

    /// Internal method to perform the ebook generation logic.
    async fn perform_generation(
        config: &HozonConfig,
        volumes_to_generate: Vec<Vec<Vec<PathBuf>>>,
        _edited_data_override: Option<Vec<Vec<PathBuf>>>,
    ) -> Result<()> {
        let target_directory_path = if config.create_output_directory {
            let path = PathBuf::from(&config.target_path).join(&config.metadata.title);
            if !path.exists() {
                fs::create_dir_all(&path).await?;
            }
            path
        } else {
            let path = PathBuf::from(&config.target_path);
            if !path.exists() {
                return Err(Error::NotFound(
                    "Target directory does not exist".to_string(),
                ));
            }
            path
        };

        if volumes_to_generate.is_empty()
            || volumes_to_generate
                .iter()
                .all(|v| v.is_empty() || v.iter().all(|c| c.is_empty()))
        {
            return Err(Error::Other("No volumes found for generation.".to_string()));
        }

        let max_concurrent = num_cpus::get().min(4); // Cap concurrent conversions to reasonable number
        let semaphore = Arc::new(Semaphore::new(max_concurrent));

        let mut tasks = Vec::new();
        let total_volumes_to_create = volumes_to_generate.len();

        for (i, volume_chapters_and_pages) in volumes_to_generate.into_iter().enumerate() {
            let current_volume_number = i + 1;
            let file_name_base = if total_volumes_to_create > 1 {
                format!(
                    "{} | Volume {}",
                    config.metadata.title, current_volume_number
                )
            } else {
                config.metadata.title.clone()
            };
            let target_dir_clone = target_directory_path.clone();
            let format_clone = config.output_format;
            let semaphore_clone = Arc::clone(&semaphore);
            let series_metadata_clone = config.metadata.clone();

            // Extract chapter titles for metadata (from first page's parent folder name, or dummy name)
            let collected_chapter_titles: Vec<String> = volume_chapters_and_pages
                .iter()
                .filter_map(|chapter_pages| {
                    chapter_pages
                        .first()
                        .and_then(|p| p.parent()) // Get chapter folder path
                        .and_then(|p| p.file_name()) // Get folder name
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string())
                        .or_else(|| Some("Untitled Chapter".to_string()))
                })
                .collect();

            let total_pages_in_volume: usize =
                volume_chapters_and_pages.iter().map(|c| c.len()).sum();

            let task = tokio::spawn(async move {
                let _permit = semaphore_clone.acquire().await?;

                match format_clone {
                    FileFormat::Cbz => {
                        let mut generator = Cbz::new(&target_dir_clone, &file_name_base)?;
                        for chapter_pages in volume_chapters_and_pages.into_iter().flatten() {
                            // Flatten all pages in the volume
                            generator.add_page(&chapter_pages).await?;
                        }
                        generator
                            .set_metadata(
                                &file_name_base,
                                Some(current_volume_number),
                                &series_metadata_clone,
                                total_pages_in_volume,
                                &collected_chapter_titles,
                            )
                            .await?;
                        generator.save().await?;
                    }
                    FileFormat::Epub => {
                        if volume_chapters_and_pages.is_empty()
                            || volume_chapters_and_pages
                                .first()
                                .map_or(true, |c| c.is_empty())
                        {
                            return Err(Error::Unsupported(
                                "Cannot create EPUB without a cover image (first page of first chapter)".to_string(),
                            ));
                        }
                        let mut generator = EPub::new(&target_dir_clone, &file_name_base)?;
                        // EPUB generator takes the first page of the first chapter as cover
                        generator.set_cover(
                            volume_chapters_and_pages.first().unwrap().first().unwrap(),
                        )?;

                        generator
                            .set_metadata(
                                &file_name_base,
                                Some(current_volume_number),
                                &series_metadata_clone,
                                total_pages_in_volume,
                                &collected_chapter_titles,
                            )
                            .await?;

                        for (chapter_idx, chapter_pages) in
                            volume_chapters_and_pages.iter().enumerate()
                        {
                            let chapter_title = collected_chapter_titles
                                .get(chapter_idx)
                                .map_or("Untitled Chapter", |s| s.as_str());
                            generator
                                .add_chapter(chapter_idx + 1, chapter_title, chapter_pages)
                                .await?;
                        }
                        generator.save().await?;
                    }
                }
                Result::Ok(())
            });
            tasks.push(task);
        }

        for task in tasks.into_iter() {
            task.await??;
        }
        Ok(())
    }
}

impl HozonConfigBuilder {
    fn validate(&self) -> std::result::Result<(), String> {
        // Validate custom regexes if they are provided
        if let Some(Some(s)) = &self.chapter_name_regex_str {
            if Regex::new(s).is_err() {
                // Return an Err(String), not your custom Error type
                return Err(format!("Invalid chapter_name_regex: {}", s));
            }
        }
        if let Some(Some(s)) = &self.page_name_regex_str {
            if Regex::new(s).is_err() {
                // Return an Err(String)
                return Err(format!("Invalid page_name_regex: {}", s));
            }
        }

        // Validate image analysis sensibility
        if let Some(sensibility) = self.image_analysis_sensibility {
            if sensibility > 100 {
                // Return an Err(String)
                return Err("Image analysis sensibility must be between 0 and 100.".to_string());
            }
        }

        Ok(())
    }
}
