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
    AnalyzeReport, CollectedContent, CollectionDepth, Direction, EbookMetadata, FileFormat,
    HozonExecutionMode, StructuredContent, VolumeGroupingStrategy, VolumeStructureReport,
};

/// The main Hozon conversion configuration, built declaratively.
/// Once configured, it can execute the conversion process using various
/// entry points (`convert_from_source`, `convert_from_collected_data`, etc.).
#[derive(Clone, derive_builder::Builder)]
#[builder(setter(into, strip_option), build_fn(validate = "Self::validate"))]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct HozonConfig {
    // --- Core Conversion Settings ---
    #[builder(default = "EbookMetadata::default_with_title(\"Untitled Conversion\".to_string())")]
    pub metadata: EbookMetadata,
    #[builder(default)]
    pub source_path: PathBuf, // Base path for collection, even if data is injected
    pub target_path: PathBuf, // Output directory
    #[builder(default = "FileFormat::Cbz")]
    pub output_format: FileFormat,
    #[builder(default = "Direction::Ltr")]
    pub reading_direction: Direction,
    #[builder(default = "true")]
    pub create_output_directory: bool,
    #[builder(default = "CollectionDepth::Deep")]
    pub collection_depth: CollectionDepth,
    #[builder(default = "75")] // 0-100%
    pub image_analysis_sensibility: u8,

    // --- Customization for Collection & Structuring Logic ---
    #[builder(default = "VolumeGroupingStrategy::Manual")]
    pub volume_grouping_strategy: VolumeGroupingStrategy,
    #[builder(default)]
    pub chapter_name_regex_str: Option<String>, // User-provided string, compiled in build()
    #[builder(default)]
    pub page_name_regex_str: Option<String>, // User-provided string, compiled in build()
    #[builder(default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub custom_chapter_path_sorter:
        Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    #[builder(default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    pub custom_page_path_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Send + Sync>>,
    #[builder(default)]
    pub volume_sizes_override: Option<Vec<usize>>, // For Manual grouping strategy

    // --- Internal fields populated by build() after string regexes are compiled ---
    #[builder(setter(skip), default)]
    #[cfg_attr(feature = "serde", serde(skip))]
    #[cfg_attr(feature = "specta", specta(skip))]
    pub(crate) compiled_chapter_name_regex: Option<Regex>,
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
    /// Returns a new builder for configuring `HozonConfig`.
    pub fn builder() -> HozonConfigBuilder {
        HozonConfigBuilder::default()
    }

    /// Performs non-content-loading, pre-flight checks on the configuration
    /// based on the intended execution mode. This method is optional to call
    /// for the user, as comprehensive checks are run internally by `convert_from_*` methods.
    ///
    /// This allows for early validation of the configuration setup.
    ///
    /// # Arguments
    /// * `mode` - Specifies the expected starting point of the conversion, influencing checks.
    ///
    /// # Returns
    /// A `Result` indicating success or an error if the configuration is invalid
    /// for the given execution mode. On success, it returns a reference to `Self` for chaining.
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
    /// This method performs the full pipeline: collection -> structuring -> generation.
    pub async fn convert_from_source(self) -> Result<()> {
        self.preflight_check(HozonExecutionMode::FromSource)?;

        // 1. Collect Content
        let collector = Collector::new(
            &self.source_path,
            self.collection_depth,
            self.compiled_chapter_name_regex.as_ref(),
            self.compiled_page_name_regex.as_ref(),
            self.image_analysis_sensibility,
        );

        let chapters = collector
            .collect_chapters(self.custom_chapter_path_sorter.as_deref())
            .await?;
        if chapters.is_empty() {
            return Err(Error::NotFound(format!(
                "No chapters found in source path: {:?}",
                self.source_path
            )));
        }
        let page_sorter = self.custom_page_path_sorter.clone();
        let collected_chapters_pages_data = collector.collect_pages(chapters, page_sorter).await?;
        if collected_chapters_pages_data.iter().all(|c| c.is_empty()) {
            return Err(Error::NotFound(format!(
                "No pages found in collected chapters from source path: {:?}",
                self.source_path
            )));
        }

        let collected_content = CollectedContent {
            chapters_with_pages: collected_chapters_pages_data,
            report: AnalyzeReport::default(), // No analysis report if analyze() is removed, or a minimal one
            grouping_strategy_recommended: self.volume_grouping_strategy, // Use configured strategy
        };

        // 2. Structure Volumes
        let structured_content =
            Self::perform_structuring(&self, collected_content.chapters_with_pages).await?;

        // 3. Generate Ebooks
        Self::perform_generation(
            &self,
            structured_content.volumes_with_chapters_and_pages,
            None,
        )
        .await
    }

    /// Starts the conversion with user-provided collected chapters and pages.
    /// This method performs: structuring -> generation.
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

    /// Starts the conversion with user-provided fully structured volumes.
    /// This method performs only: generation.
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

                if let Some(user_volume_sizes) = &config.volume_sizes_override {
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
