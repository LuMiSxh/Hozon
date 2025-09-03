//! Comic/manga image collection and organization module.
//!
//! This module provides functionality to collect, organize and analyze image files
//! from a directory structure, typically representing chapters and pages of comics or manga.
//! It includes tools for sorting files numerically and detecting chapter boundaries.

use std::cmp::Ordering;

use std::path::PathBuf;
use std::sync::Arc;

use futures::future::try_join_all;
use image::{DynamicImage, GenericImageView, Pixel};
use lazy_static::lazy_static;
use rayon::prelude::*;
use regex::Regex;
use tokio::fs::{ReadDir, read_dir};
use tokio::spawn;
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, spawn_blocking};

use crate::error::{Error, Result};
use crate::path_utils::{
    compare_paths_by_number_safe, extract_number_from_filename_safe, get_file_name_lossy,
    get_file_name_safe, is_hidden_file, validate_path,
};
use crate::types::CollectionDepth;
use crate::{AnalyzeFinding, AnalyzeReport, CollectedContent, VolumeGroupingStrategy};

/// Limits the number of concurrent directory operations
const MAX_CONCURRENT_DIRS: usize = 64;
/// Controls how many pixels to skip when sampling for grayscale detection
const GRAYSCALE_SAMPLE_RATE: u32 = 10;
/// Maximum dimension for grayscale detection before downsampling
const GRAYSCALE_MAX_DIMENSION: u32 = 500;
/// RGB difference threshold for determining if a pixel is grayscale
const RGB_GRAYSCALE_THRESHOLD: u8 = 10;

lazy_static! {
    /// Default Regex pattern for extracting numeric values from chapter/page filenames.
    /// Matches "001", "1", "1.5" etc.
    pub static ref DEFAULT_NUMBER_REGEX: Regex = Regex::new(r"\d+\.?\d*").unwrap();
    /// Default Regex for analyzing chapter/volume naming patterns for `VolumeGroupingStrategy::Name`.
    /// Matches strings in format "digits-digits[.digits]" (e.g. "01-23" or "01-23.5").
    pub static ref DEFAULT_NAME_GROUPING_REGEX: Regex = Regex::new(r"\d+-\d+(\.\d+)?").unwrap();
}

/// Manages collection and organization of image files in a directory structure
#[derive(Debug)]
pub struct Collector<'a> {
    base_directory: &'a PathBuf,
    collection_depth: CollectionDepth,
    chapter_name_regex: Option<&'a Regex>, // Custom regex for chapter name parsing
    page_name_regex: Option<&'a Regex>,    // Custom regex for page name parsing
    image_analysis_sensibility: u8,        // 0-100%
}

impl<'a> Collector<'a> {
    /// Creates a new Collector instance for the specified directory.
    ///
    /// # Arguments
    ///
    /// * `base_directory` - Path to the root directory containing chapters/volumes
    /// * `collection_depth` - How deep to scan for chapters and pages
    /// * `chapter_name_regex` - Optional custom regex for parsing chapter names
    /// * `page_name_regex` - Optional custom regex for parsing page names
    /// * `image_analysis_sensibility` - Sensitivity (0-100) for grayscale detection
    pub fn new(
        base_directory: &'a PathBuf,
        collection_depth: CollectionDepth,
        chapter_name_regex: Option<&'a Regex>,
        page_name_regex: Option<&'a Regex>,
        image_analysis_sensibility: u8,
    ) -> Self {
        Self {
            base_directory,
            collection_depth,
            chapter_name_regex,
            page_name_regex,
            image_analysis_sensibility: image_analysis_sensibility.min(100),
        }
    }

    /// Collects chapter directories from the base directory
    ///
    /// # Arguments
    ///
    /// * `custom_sorter` - Optional function to sort the collected chapters
    ///
    /// # Returns
    ///
    /// * `Result<Vec<PathBuf>>` - Vector of paths to chapter directories
    pub async fn collect_chapters<F>(&self, custom_sorter: Option<F>) -> Result<Vec<PathBuf>>
    where
        F: Fn(&PathBuf, &PathBuf) -> Ordering + Sync,
    {
        let mut chapters = if self.collection_depth == CollectionDepth::Shallow {
            // In shallow mode, the base_directory itself is the single "chapter"
            vec![self.base_directory.clone()]
        } else {
            // In deep mode, find subdirectories
            Self::collect_parallel(self.base_directory, true).await?
        };

        if let Some(sorter) = custom_sorter {
            chapters.par_sort_by(sorter);
        } else {
            // Default sort for chapters if no custom sorter provided
            chapters.par_sort_by(&Collector::sort_name_by_number_default);
        }
        Ok(chapters)
    }

    /// Collects page images from each chapter directory
    ///
    /// # Arguments
    ///
    /// * `chapters` - Vector of chapter directory paths
    /// * `custom_sorter` - Optional function to sort the collected pages
    ///
    /// # Returns
    ///
    /// * `Result<Vec<Vec<PathBuf>>>` - Vector of vectors containing page paths for each chapter
    pub async fn collect_pages(
        &self,
        chapters: Vec<PathBuf>,
        custom_sorter: Option<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Sync + Send + 'static>>,
    ) -> Result<Vec<Vec<PathBuf>>> {
        let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_DIRS));
        let mut handles: Vec<JoinHandle<Result<(usize, Vec<PathBuf>)>>> = Vec::new();

        for (index, chapter_dir) in chapters.into_iter().enumerate() {
            let semaphore = Arc::clone(&semaphore);
            let page_sorter = custom_sorter.clone();

            handles.push(spawn(async move {
                let _permit = semaphore.acquire().await?;

                let mut chapter_images = Self::collect_parallel(&chapter_dir, false).await?;

                if let Some(sorter) = page_sorter.as_ref() {
                    chapter_images.par_sort_by(sorter.as_ref());
                } else {
                    chapter_images.par_sort_by(&Collector::sort_name_by_number_default);
                }
                Ok((index, chapter_images))
            }));
        }

        let results = try_join_all(handles).await.map_err(|e| {
            Error::AsyncTaskError(format!("Failed to join page collection tasks: {}", e))
        })?;

        let mut pages_per_chapter = vec![Vec::new(); results.len()];
        for res in results {
            let (index, chapter_images) = res?;
            pages_per_chapter[index] = chapter_images;
        }

        Ok(pages_per_chapter)
    }

    /// Identifies chapters that are likely to be the start of a new volume
    /// by analyzing the cover image (first image) of each chapter
    ///
    /// # Arguments
    ///
    /// * `images_per_chapter` - Nested vector of image paths organized by chapter
    /// * `sensibility` - Custom sensibility override (0.0 to 1.0), or None to use instance setting
    ///
    /// # Returns
    ///
    /// * `Result<Vec<usize>>` - Indices of chapters that start new volumes
    pub async fn determine_volume_start_chapters(
        &self,
        images_per_chapter: Vec<Vec<PathBuf>>,
        sensibility: Option<f64>,
    ) -> Result<Vec<usize>> {
        if images_per_chapter.is_empty() {
            return Ok(Vec::new());
        }

        let effective_sensibility =
            sensibility.unwrap_or(self.image_analysis_sensibility as f64 / 100.0);

        let semaphore = Arc::new(Semaphore::new(num_cpus::get().min(8)));
        let mut handles: Vec<JoinHandle<Result<Option<usize>>>> = Vec::new();

        for (i, images_in_chapter) in images_per_chapter.into_iter().enumerate() {
            if images_in_chapter.is_empty() {
                continue;
            }

            let cover_path = images_in_chapter[0].clone();
            let semaphore = Arc::clone(&semaphore);

            handles.push(spawn(async move {
                let _permit = semaphore.acquire().await?;
                // image::open is blocking, so move it to a blocking thread
                spawn_blocking(move || {
                    let cover_image = image::open(&cover_path)?;
                    Ok(
                        if Collector::is_grayscale(&cover_image, effective_sensibility) {
                            None // Is grayscale, likely not a cover
                        } else {
                            Some(i) // Not grayscale, likely a cover/volume start
                        },
                    )
                })
                .await?
            }));
        }

        let results = try_join_all(handles).await.map_err(|e| {
            Error::AsyncTaskError(format!("Failed to join volume detection tasks: {}", e))
        })?;

        let mut volume_start_chapters: Vec<usize> = results
            .into_iter()
            .filter_map(|result| result.ok().flatten())
            .collect();

        if !volume_start_chapters.contains(&0) {
            volume_start_chapters.insert(0, 0);
        }

        volume_start_chapters.par_sort_unstable();
        volume_start_chapters.dedup();

        Ok(volume_start_chapters)
    }

    /// Calculates how many chapters belong to each volume given start indices.
    ///
    /// # Arguments
    ///
    /// * `volume_start_chapters` - Vector of chapter indices that start new volumes (must be sorted and unique)
    /// * `total_chapters` - Total number of chapters
    ///
    /// # Returns
    ///
    /// * `Result<Vec<usize>>` - Vector of chapter counts for each volume
    pub fn calculate_volume_sizes(
        &self,
        volume_start_chapters: Vec<usize>, // Assumed sorted and unique
        total_chapters: usize,
    ) -> Result<Vec<usize>> {
        let mut volume_chapters: Vec<usize> = Vec::new();

        if volume_start_chapters.is_empty() {
            // If no explicit starts, and total_chapters > 0, treat all as one volume
            if total_chapters > 0 {
                return Ok(vec![total_chapters]);
            } else {
                return Ok(Vec::new());
            }
        }

        let mut prev_chapter_idx = *volume_start_chapters.first().unwrap_or(&0);
        for &current_chapter_idx in volume_start_chapters.iter().skip(1) {
            let chapter_count = current_chapter_idx - prev_chapter_idx;
            if chapter_count > 0 {
                volume_chapters.push(chapter_count);
            }
            prev_chapter_idx = current_chapter_idx;
        }

        // Add the remaining chapters for the last volume
        let remaining = total_chapters.saturating_sub(prev_chapter_idx);
        if remaining > 0 {
            volume_chapters.push(remaining);
        } else if volume_chapters.is_empty() && total_chapters > 0 {
            // This could happen if volume_start_chapters only contained the first chapter
            volume_chapters.push(total_chapters);
        }
        Ok(volume_chapters)
    }

    /// Collects and analyzes the source content, producing a detailed report.
    ///
    /// # Returns
    ///
    /// * `Result<CollectedContent>` - The collected chapters and pages along with an analysis
    pub async fn analyze_source_content(&self) -> Result<CollectedContent> {
        let mut findings = Vec::new();

        // 1. Collect chapters and pages
        let chapters = self
            .collect_chapters(None::<fn(&PathBuf, &PathBuf) -> Ordering>)
            .await?;
        if chapters.is_empty() {
            findings.push(AnalyzeFinding::NoChaptersFound);
            return Ok(CollectedContent {
                chapters_with_pages: Vec::new(),
                report: AnalyzeReport {
                    findings,
                    ..Default::default()
                },
            });
        }
        let pages_per_chapter = self.collect_pages(chapters.clone(), None).await?;
        if pages_per_chapter.par_iter().all(Vec::is_empty) {
            findings.push(AnalyzeFinding::NoPagesFound);
            return Ok(CollectedContent {
                chapters_with_pages: pages_per_chapter,
                report: AnalyzeReport {
                    findings,
                    ..Default::default()
                },
            });
        }

        // 2. Perform various checks and populate findings

        // Example Check: Naming conventions and strategy recommendation
        let has_name_pattern = chapters.iter().any(|path| {
            get_file_name_safe(path)
                .map(|name| DEFAULT_NAME_GROUPING_REGEX.is_match(&name))
                .unwrap_or(false)
        });

        let recommended_strategy = if has_name_pattern {
            findings.push(AnalyzeFinding::ConsistentNamingFound {
                count: chapters.len(),
                pattern: "Volume-Chapter".to_string(),
            });
            VolumeGroupingStrategy::Name
        } else {
            // Default fallback if naming is not consistent
            VolumeGroupingStrategy::ImageAnalysis
        };

        // Additional analysis checks

        // Check for unsupported file types by comparing raw directory contents with collected pages
        for (chapter_idx, chapter_pages) in pages_per_chapter.iter().enumerate() {
            if chapter_idx < chapters.len() {
                // Get all files in this chapter directory (without filtering)
                if let Ok(all_files) = Self::collect_all_files(&chapters[chapter_idx]).await {
                    // Find files that were in the directory but not collected (i.e., unsupported)
                    for file_path in &all_files {
                        if !chapter_pages.contains(file_path) {
                            if let Err(_) = crate::types::get_file_info(file_path) {
                                findings.push(AnalyzeFinding::UnsupportedFileIgnored {
                                    path: file_path.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }

        // Check for page count consistency
        if pages_per_chapter.len() > 1 {
            let page_counts: Vec<usize> = pages_per_chapter
                .iter()
                .map(|chapter| chapter.len())
                .collect();
            let avg_page_count =
                page_counts.iter().sum::<usize>() as f64 / page_counts.len() as f64;
            let threshold = (avg_page_count * 0.3).max(1.0); // 30% deviation threshold

            for (chapter_idx, &count) in page_counts.iter().enumerate() {
                let deviation = (count as f64 - avg_page_count).abs();
                if deviation > threshold {
                    findings.push(AnalyzeFinding::InconsistentPageCount {
                        chapter_path: chapters[chapter_idx].clone(),
                        expected: avg_page_count.round() as usize,
                        found: count,
                    });
                }
            }
        }

        // Check for file permissions by attempting to read metadata
        for chapter_pages in &pages_per_chapter {
            for page_path in chapter_pages {
                if let Err(e) = std::fs::metadata(page_path) {
                    if e.kind() == std::io::ErrorKind::PermissionDenied {
                        findings.push(AnalyzeFinding::PermissionDenied {
                            path: page_path.clone(),
                        });
                    }
                }
            }
        }

        // Check for special characters in paths that might cause issues
        for chapter_pages in &pages_per_chapter {
            for page_path in chapter_pages {
                if let Err(_) = validate_path(page_path) {
                    findings.push(AnalyzeFinding::SpecialCharactersInPath {
                        path: page_path.clone(),
                    });
                }
            }
        }

        // Check for unusual file sizes
        if pages_per_chapter.len() > 0 {
            let mut all_sizes: Vec<u64> = Vec::new();
            for chapter_pages in &pages_per_chapter {
                for page_path in chapter_pages {
                    if let Ok(metadata) = std::fs::metadata(page_path) {
                        all_sizes.push(metadata.len());
                    }
                }
            }

            if !all_sizes.is_empty() {
                let avg_size = all_sizes.iter().sum::<u64>() / all_sizes.len() as u64;
                let avg_size_kb = avg_size / 1024;

                for chapter_pages in &pages_per_chapter {
                    for page_path in chapter_pages {
                        if let Ok(metadata) = std::fs::metadata(page_path) {
                            let size_kb = metadata.len() / 1024;
                            // Flag files that are significantly larger or smaller than average
                            if size_kb > avg_size_kb * 3
                                || (avg_size_kb > 10 && size_kb < avg_size_kb / 3)
                            {
                                findings.push(AnalyzeFinding::UnusualFileSize {
                                    file_path: page_path.clone(),
                                    size_kb,
                                    average_kb: avg_size_kb,
                                });
                            }
                        }
                    }
                }
            }
        }

        // 3. Assemble and return the final structure
        let report = AnalyzeReport {
            findings,
            recommended_strategy,
        };

        Ok(CollectedContent {
            chapters_with_pages: pages_per_chapter,
            report,
        })
    }

    // Helper methods

    /// Determines whether an image is predominantly grayscale
    ///
    /// # Arguments
    ///
    /// * `img` - Dynamic image to analyze
    /// * `sensibility` - Threshold value (0.0-1.0) determining how many pixels must be gray
    ///
    /// # Returns
    ///
    /// * `bool` - True if the image is predominantly grayscale
    pub fn is_grayscale(img: &DynamicImage, sensibility: f64) -> bool {
        // Downsample image if it's too large to improve performance
        let working_img;
        let img_to_use =
            if img.width() > GRAYSCALE_MAX_DIMENSION || img.height() > GRAYSCALE_MAX_DIMENSION {
                let scale = GRAYSCALE_MAX_DIMENSION as f32 / img.width().max(img.height()) as f32;
                let new_width = (img.width() as f32 * scale) as u32;
                let new_height = (img.height() as f32 * scale) as u32;
                working_img = img.thumbnail(new_width, new_height);
                &working_img
            } else {
                img
            };

        let total_pixels = (img_to_use.width() * img_to_use.height()) as f64;
        let gray_threshold = total_pixels * sensibility;

        // Create chunks of pixels to process in parallel
        let width = img_to_use.width();
        let height = img_to_use.height();

        // Consider only every Nth pixel to speed up processing
        let samples = (0..height)
            .step_by(GRAYSCALE_SAMPLE_RATE as usize)
            .flat_map(|y| {
                (0..width)
                    .step_by(GRAYSCALE_SAMPLE_RATE as usize)
                    .map(move |x| (x, y))
            })
            .collect::<Vec<_>>();

        if samples.is_empty() {
            return false; // Cannot determine grayscale for empty image/samples
        }

        let sample_count = samples.len();

        let gray_pixels = samples
            .par_iter()
            .map(|(x, y)| {
                let pixel = img_to_use.get_pixel(*x, *y);
                let rgb = pixel.to_rgb();
                let r = rgb.0[0];
                let g = rgb.0[1];
                let b = rgb.0[2];

                // Check if the RGB values are close to each other
                let r_diff = r.abs_diff(g);
                let g_diff = g.abs_diff(b);
                let b_diff = b.abs_diff(r);

                r_diff <= RGB_GRAYSCALE_THRESHOLD
                    && g_diff <= RGB_GRAYSCALE_THRESHOLD
                    && b_diff <= RGB_GRAYSCALE_THRESHOLD
            })
            .filter(|&is_gray| is_gray)
            .count();

        // Scale back to estimate the full image
        let estimated_gray_pixels = (gray_pixels as f64 * total_pixels) / sample_count as f64;

        estimated_gray_pixels > gray_threshold
    }

    /// Collects directory contents in parallel with filtering options
    ///
    /// # Arguments
    ///
    /// * `directory` - Directory to scan
    /// * `only_dirs` - When true, only directories are collected; when false, only files
    ///
    /// # Returns
    ///
    /// * `Result<Vec<PathBuf>>` - Paths meeting the criteria
    pub async fn collect_parallel(directory: &PathBuf, only_dirs: bool) -> Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = Vec::new();

        // Read directory contents
        let mut paths: ReadDir = read_dir(directory).await.map_err(|e| Error::Io(e))?;

        while let Some(entry) = paths.next_entry().await.map_err(|e| Error::Io(e))? {
            let path = entry.path();

            // Skip hidden files
            if is_hidden_file(&path) {
                continue;
            }

            // Apply directory/file filter
            let is_dir = path.is_dir();
            if (only_dirs && !is_dir) || (!only_dirs && is_dir) {
                continue; // Just skip, don't return an error for mixed content
            }

            // For files (when only_dirs is false), also filter by supported image formats
            if !only_dirs && !is_dir {
                if let Err(_) = crate::types::get_file_info(&path) {
                    continue; // Skip unsupported file formats
                }
            }

            entries.push(path);
        }

        Ok(entries)
    }

    /// Collects all files in a directory without any filtering (used for analysis)
    ///
    /// # Arguments
    ///
    /// * `directory` - Directory to scan
    ///
    /// # Returns
    ///
    /// * `Result<Vec<PathBuf>>` - All non-hidden files in the directory
    pub async fn collect_all_files(directory: &PathBuf) -> Result<Vec<PathBuf>> {
        let mut entries: Vec<PathBuf> = Vec::new();

        // Read directory contents
        let mut paths: ReadDir = read_dir(directory).await.map_err(|e| Error::Io(e))?;

        while let Some(entry) = paths.next_entry().await.map_err(|e| Error::Io(e))? {
            let path = entry.path();

            // Skip hidden files
            if is_hidden_file(&path) {
                continue;
            }

            // Only include files, not directories
            if !path.is_dir() {
                entries.push(path);
            }
        }

        Ok(entries)
    }

    /// Filters paths based on a test condition
    ///
    /// # Arguments
    ///
    /// * `paths` - Vector of paths to check
    /// * `test_case` - Function that returns true if the path passes the test
    ///
    /// # Returns
    ///
    /// * `Result<Vec<PathBuf>>` - Paths that failed the test
    pub fn check_path<F>(paths: &[PathBuf], test_case: F) -> Result<Vec<PathBuf>>
    where
        F: Fn(&PathBuf) -> bool + Sync + Send,
    {
        let invalid_paths: Vec<PathBuf> = paths
            .par_iter()
            .filter(|path| !test_case(path))
            .cloned()
            .collect();

        Ok(invalid_paths)
    }

    /// Extracts a numeric value from a path using the configured regex or a default.
    ///
    /// # Arguments
    ///
    /// * `s` - Path to extract number from
    /// * `for_chapter_name` - True if extracting for a chapter name, false for a page name
    ///
    /// # Returns
    ///
    /// * `Option<f64>` - Extracted number or None if not found
    pub fn regex_parser(&self, s: &PathBuf, for_chapter_name: bool) -> Option<f64> {
        let active_regex = if for_chapter_name {
            self.chapter_name_regex.unwrap_or(&DEFAULT_NUMBER_REGEX)
        } else {
            self.page_name_regex.unwrap_or(&DEFAULT_NUMBER_REGEX)
        };

        extract_number_from_filename_safe(s, active_regex)
    }

    /// Sorts paths by numeric values in their file stem using default regex.
    /// This is mainly for internal use when no specific sorting or custom regex is provided.
    pub fn sort_name_by_number_default(a: &PathBuf, b: &PathBuf) -> Ordering {
        compare_paths_by_number_safe(a, b, &DEFAULT_NUMBER_REGEX)
    }

    /// Sorts paths by numeric values found in their names using the collector's configured regex.
    pub fn sort_name_by_number(&self, a: &PathBuf, b: &PathBuf) -> Ordering {
        let an = self.regex_parser(a, false); // Assuming this is for pages or chapters where a single number is expected
        let bn = self.regex_parser(b, false);

        an.partial_cmp(&bn).unwrap_or(Ordering::Equal)
    }

    /// Sorts paths by volume and chapter numbers in filenames.
    /// Expects filenames in format "volume-chapter" (e.g., "1-15.jpg") or similar pattern.
    /// Uses the default grouping regex for volume/chapter identification.
    pub fn sort_by_name_volume_chapter_default(a: &PathBuf, b: &PathBuf) -> Ordering {
        fn parse_numbers(path: &PathBuf) -> (Option<f64>, Option<f64>) {
            let file_name = get_file_name_lossy(path);
            if let Some(caps) = DEFAULT_NAME_GROUPING_REGEX.captures(&file_name) {
                let full_match = caps.get(0).unwrap().as_str(); // e.g., "01-23.5"
                let parts: Vec<&str> = full_match.split('-').collect();
                let volume_part = parts.first().unwrap_or(&"0");
                let chapter_part_with_ext = parts.get(1).unwrap_or(&"0");

                let volume = volume_part.trim_start_matches('0').parse::<f64>().ok();
                let chapter = chapter_part_with_ext
                    .split('.')
                    .next() // "23.5" -> "23"
                    .unwrap_or("0")
                    .trim_start_matches('0')
                    .parse::<f64>()
                    .ok();

                // For the decimal part, try to append it if present
                let decimal_part = chapter_part_with_ext.split('.').nth(1);
                let chapter = if let (Some(c), Some(d_str)) = (chapter, decimal_part) {
                    d_str
                        .parse::<f64>()
                        .ok()
                        .map(|d| c + d / (10_f64.powi(d_str.len() as i32)))
                } else {
                    chapter
                };

                return (volume, chapter);
            }
            (None, None)
        }

        let (a_vol, a_chap) = parse_numbers(a);
        let (b_vol, b_chap) = parse_numbers(b);

        match a_vol.partial_cmp(&b_vol) {
            Some(Ordering::Equal) => a_chap.partial_cmp(&b_chap).unwrap_or(Ordering::Equal),
            Some(order) => order,
            None => Ordering::Equal, // If cannot parse volume, treat as equal
        }
    }
}
