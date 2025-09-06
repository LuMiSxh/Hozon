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
use crate::path_utils::sanitize_filename;
use crate::types::{
    CollectedContent, CollectionDepth, CoverOptions, Direction, EbookMetadata, FileFormat,
    HozonExecutionMode, StructuredContent, VolumeGroupingStrategy, VolumeStructureReport,
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
    #[builder(default)]
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

    /// Separator character(s) used between series title and volume number.
    ///
    /// When multiple volumes are generated, the filename format will be:
    /// `{title}{separator}Volume {number}.{extension}`
    ///
    /// Examples:
    /// - `" - "` → "My Series - Volume 1.cbz"
    /// - `" | "` → "My Series | Volume 1.cbz"
    /// - `"_"` → "My Series_Volume 1.cbz"
    #[builder(default = "\" - \".to_string()")]
    pub volume_separator: String,

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
            .field("volume_separator", &self.volume_separator)
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

    /// Validates only the source-related parts of the configuration.
    fn validate_source(&self) -> Result<()> {
        if self.source_path.as_os_str().is_empty() {
            return Err(Error::Other(
                "`source_path` must be set for analysis.".to_string(),
            ));
        }

        // Validate path format and characters
        crate::path_utils::validate_path(&self.source_path)?;

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

        // Try to normalize the path to catch potential long path issues early
        match crate::path_utils::normalize_path(&self.source_path) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::InvalidPath(
                self.source_path.clone(),
                format!("Path validation failed: {}", e),
            )),
        }
    }

    /// Performs only the structuring step on pre-collected chapter data.
    ///
    /// This method takes organized chapter and page data and groups them into logical
    /// volumes based on the configured [`VolumeGroupingStrategy`]. Use this when you
    /// want to structure data without performing the full conversion pipeline.
    ///
    /// # Arguments
    ///
    /// * `collected_data` - A vector of chapters, where each chapter is a vector of image file paths.
    ///   The structure should be: `Vec<Chapter: Vec<PagePath>>`
    ///
    /// # Returns
    ///
    /// * `Ok(StructuredContent)` - Contains:
    ///   - `volumes_with_chapters_and_pages`: Organized volume data ready for generation
    ///   - `report`: Volume structuring report with statistics
    ///   - `grouping_strategy_applied`: The strategy that was actually used
    /// * `Err(Error)` - Structuring failed due to configuration or processing errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> hozon::error::Result<()> {
    /// let chapters = vec![
    ///     vec![PathBuf::from("ch1/page1.jpg"), PathBuf::from("ch1/page2.jpg")],
    ///     vec![PathBuf::from("ch2/page1.jpg"), PathBuf::from("ch2/page2.jpg")],
    ///     vec![PathBuf::from("ch3/page1.jpg"), PathBuf::from("ch3/page2.jpg")],
    /// ];
    ///
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("Structure Example".to_string()))
    ///     .target_path(PathBuf::from("./output"))
    ///     .volume_grouping_strategy(VolumeGroupingStrategy::Manual)
    ///     .volume_sizes_override(vec![2, 1]) // 2 chapters in vol 1, 1 chapter in vol 2
    ///     .build()?;
    ///
    /// let structured = config.structure_from_collected_data(chapters).await?;
    ///
    /// println!("Created {} volumes", structured.report.total_volumes_created);
    /// println!("Volume chapter counts: {:?}", structured.report.chapter_counts_per_volume);
    /// println!("Strategy used: {:?}", structured.grouping_strategy_applied);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn structure_from_collected_data(
        &self,
        collected_data: Vec<Vec<PathBuf>>,
    ) -> Result<StructuredContent> {
        Self::perform_structuring(self, collected_data).await
    }

    /// Analyzes the source directory structure and content without performing conversion.
    ///
    /// This method performs the initial analysis phase of the conversion pipeline,
    /// scanning the source directory to collect chapters and pages while generating
    /// a comprehensive report about the content structure, potential issues, and
    /// recommended volume grouping strategies.
    ///
    /// # Returns
    ///
    /// * `Ok(CollectedContent)` - Contains:
    ///   - `chapters_with_pages`: Organized chapter and page data ready for structuring
    ///   - `report`: Detailed analysis findings and recommendations
    /// * `Err(Error)` - Analysis failed due to source validation or I/O errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> hozon::error::Result<()> {
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("Analysis Example".to_string()))
    ///     .source_path(PathBuf::from("./manga_source"))
    ///     .target_path(PathBuf::from("./output"))
    ///     .volume_grouping_strategy(VolumeGroupingStrategy::Name)
    ///     .build()?;
    ///
    /// let analysis = config.analyze_source().await?;
    ///
    /// println!("Found {} chapters", analysis.chapters_with_pages.len());
    /// println!("Recommended strategy: {:?}", analysis.report.recommended_strategy);
    ///
    /// // Check for any issues found during analysis
    /// for finding in &analysis.report.findings {
    ///     match finding {
    ///         AnalyzeFinding::ConsistentNamingFound { pattern, .. } => {
    ///             println!("Good: Consistent naming pattern: {}", pattern);
    ///         }
    ///         AnalyzeFinding::InconsistentPageCount { chapter_path, expected, found } => {
    ///             println!("Warning: Chapter {:?} has {} pages, expected {}",
    ///                      chapter_path, found, expected);
    ///         }
    ///         _ => {} // Handle other findings as needed
    ///     }
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn analyze_source(&self) -> Result<CollectedContent> {
        self.validate_source()?;

        let collector = Collector::new(
            &self.source_path,
            self.collection_depth,
            self.compiled_chapter_name_regex.as_ref(),
            self.compiled_page_name_regex.as_ref(),
            self.image_analysis_sensibility,
        );

        collector.analyze_source_content().await
    }

    // --- Core conversion entry points ---

    /// Starts the full conversion pipeline from a source directory.
    ///
    /// This method performs the complete conversion workflow:
    /// 1. **Analysis**: Scans and analyzes the source directory structure
    /// 2. **Structuring**: Groups chapters into logical volumes based on the configured strategy
    /// 3. **Generation**: Creates the final ebook files in the specified format
    ///
    /// # Arguments
    ///
    /// * `cover_options` - Specifies how to handle cover images:
    ///   - [`CoverOptions::None`]: Uses default behavior (first page for EPUB, no cover for CBZ)
    ///   - [`CoverOptions::Single(path)`]: Uses the same cover image for all volumes
    ///   - [`CoverOptions::PerVolume(map)`]: Uses different cover images per volume
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
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("My Comic".to_string()))
    ///     .source_path(PathBuf::from("./source"))
    ///     .target_path(PathBuf::from("./output"))
    ///     .build()?;
    ///
    /// // Convert without custom cover
    /// config.convert_from_source(CoverOptions::None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_from_source(self, cover_options: CoverOptions) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromSource)?;
        let collected_content = self.analyze_source().await?;

        self.convert_from_collected_data(collected_content.chapters_with_pages, cover_options)
            .await
    }

    /// Starts the conversion pipeline from pre-collected chapter/page data.
    ///
    /// This method performs the structuring and generation steps of the conversion workflow:
    /// 1. **Structuring**: Groups the provided chapters into logical volumes
    /// 2. **Generation**: Creates the final ebook files in the specified format
    ///
    /// Use this method when you have already collected and organized your image files
    /// and want to skip the initial analysis phase.
    ///
    /// # Arguments
    ///
    /// * `collected_data` - A vector of chapters, where each chapter is a vector of image file paths.
    ///   The structure should be: `Vec<Chapter: Vec<PagePath>>`
    /// * `cover_options` - Specifies how to handle cover images (see [`CoverOptions`] for details)
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
    /// let chapters = vec![
    ///     vec![PathBuf::from("ch1/page1.jpg"), PathBuf::from("ch1/page2.jpg")],
    ///     vec![PathBuf::from("ch2/page1.jpg"), PathBuf::from("ch2/page2.jpg")],
    /// ];
    ///
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("My Comic".to_string()))
    ///     .target_path(PathBuf::from("./output"))
    ///     .build()?;
    ///
    /// config.convert_from_collected_data(chapters, CoverOptions::None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_from_collected_data(
        self,
        collected_data: Vec<Vec<PathBuf>>,
        cover_options: CoverOptions,
    ) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromCollectedData)?;
        let structured_content = Self::perform_structuring(&self, collected_data).await?;

        Self::perform_generation(
            &self,
            structured_content.volumes_with_chapters_and_pages,
            &cover_options, // Pass CoverOptions by reference
        )
        .await
    }

    /// Executes only the generation step from pre-structured volume data.
    ///
    /// This method performs only the final generation step of the conversion workflow,
    /// creating ebook files from fully structured volume data. Use this when you have
    /// already performed analysis and structuring yourself and want maximum control
    /// over the volume organization.
    ///
    /// # Arguments
    ///
    /// * `structured_data` - A vector of volumes, where each volume contains chapters,
    ///   and each chapter contains page paths. The structure should be:
    ///   `Vec<Volume: Vec<Chapter: Vec<PagePath>>>`
    /// * `cover_options` - Specifies how to handle cover images (see [`CoverOptions`] for details)
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Generation completed successfully
    /// * `Err(Error)` - Generation failed due to validation, I/O, or processing errors
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use hozon::prelude::*;
    /// # use std::path::PathBuf;
    /// # #[tokio::main]
    /// # async fn main() -> hozon::error::Result<()> {
    /// let volumes = vec![
    ///     // Volume 1
    ///     vec![
    ///         vec![PathBuf::from("vol1/ch1/page1.jpg"), PathBuf::from("vol1/ch1/page2.jpg")],
    ///         vec![PathBuf::from("vol1/ch2/page1.jpg"), PathBuf::from("vol1/ch2/page2.jpg")],
    ///     ],
    ///     // Volume 2
    ///     vec![
    ///         vec![PathBuf::from("vol2/ch1/page1.jpg"), PathBuf::from("vol2/ch1/page2.jpg")],
    ///     ],
    /// ];
    ///
    /// let config = HozonConfig::builder()
    ///     .metadata(EbookMetadata::default_with_title("My Series".to_string()))
    ///     .target_path(PathBuf::from("./output"))
    ///     .build()?;
    ///
    /// config.convert_from_structured_data(volumes, CoverOptions::None).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn convert_from_structured_data(
        self,
        structured_data: Vec<Vec<Vec<PathBuf>>>,
        cover_options: CoverOptions,
    ) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromStructuredData)?;
        Self::perform_generation(&self, structured_data, &cover_options).await
    }

    // --- Private helper methods for pipeline steps ---

    /// Internal method to perform the volume structuring logic.
    ///
    /// This method takes collected chapters and groups them into logical volumes
    /// based on the configured [`VolumeGroupingStrategy`]. It handles all the
    /// complex logic for different grouping strategies including name-based grouping,
    /// image analysis, manual grouping, and flat organization.
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration containing grouping strategy and other settings
    /// * `collected_chapters_pages` - Vector of chapters with their page paths
    ///
    /// # Returns
    ///
    /// * `Ok(StructuredContent)` - Successfully structured volumes with detailed report
    /// * `Err(Error)` - Structuring failed due to configuration or processing errors
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
    ///
    /// This method handles the final step of creating ebook files from structured volume data.
    /// It manages concurrent generation of multiple volumes, applies custom covers based on
    /// the provided options, and delegates to format-specific generators (CBZ or EPUB).
    ///
    /// # Arguments
    ///
    /// * `config` - The configuration containing metadata, target paths, and format settings
    /// * `volumes_to_generate` - The structured volume data ready for generation
    /// * `cover_options` - Cover image options for the generated volumes
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All volumes generated successfully
    /// * `Err(Error)` - Generation failed due to I/O, format, or processing errors
    async fn perform_generation(
        config: &HozonConfig,
        volumes_to_generate: Vec<Vec<Vec<PathBuf>>>,
        cover_options: &CoverOptions,
    ) -> Result<()> {
        let target_directory_path = if config.create_output_directory {
            let path =
                PathBuf::from(&config.target_path).join(&sanitize_filename(&config.metadata.title));
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
                sanitize_filename(&format!(
                    "{}{}Volume {}",
                    config.metadata.title, config.volume_separator, current_volume_number
                ))
            } else {
                sanitize_filename(&config.metadata.title)
            };
            let target_dir_clone = target_directory_path.clone();
            let format_clone = config.output_format;
            let semaphore_clone = Arc::clone(&semaphore);
            let series_metadata_clone = config.metadata.clone();
            let cover_path_for_this_volume = match cover_options {
                CoverOptions::None => None,
                CoverOptions::Single(path) => Some(path.clone()),
                CoverOptions::PerVolume(map) => map.get(&i).cloned(),
            };

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

                        // Add custom cover if provided
                        if let Some(cover_path) = &cover_path_for_this_volume {
                            generator.add_cover_page(cover_path).await?;
                        }

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
                        let mut generator = EPub::new(&target_dir_clone, &file_name_base)?;

                        // Use custom cover if provided, otherwise use first page of first chapter
                        if let Some(cover_path) = &cover_path_for_this_volume {
                            generator.set_cover(cover_path)?;
                        } else {
                            if volume_chapters_and_pages.is_empty()
                                || volume_chapters_and_pages
                                    .first()
                                    .map_or(true, |c| c.is_empty())
                            {
                                return Err(Error::Unsupported(
                                    "Cannot create EPUB without a cover image (first page of first chapter)".to_string(),
                                ));
                            }
                            // EPUB generator takes the first page of the first chapter as cover
                            generator.set_cover(
                                volume_chapters_and_pages.first().unwrap().first().unwrap(),
                            )?;
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
