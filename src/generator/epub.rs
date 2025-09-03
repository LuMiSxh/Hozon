use std::fs::File;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::generator::Generator;
use crate::path_utils::{normalize_path, path_to_string_lossy};
use crate::types::{Direction, EbookMetadata, get_file_info};
use async_trait::async_trait;
use epub_builder::{EpubBuilder, EpubContent, EpubVersion, ZipLibrary};
use memmap2::MmapOptions;
use tokio::task::spawn_blocking;

/// Generates XHTML content for an image to be included in the EPUB.
///
/// # Arguments
///
/// * `image_source` - Path to the image file relative to the EPUB root
///
/// # Returns
///
/// * `Result<String>` - The generated XHTML content or an error
fn generate_xhtml(image_source: &str, page_title: &str) -> Result<String> {
    const TEMPLATE: &str = include_str!("../../templates/Epub.xhtml");
    let xhtml = TEMPLATE
        .replace("%title%", page_title)
        .replace("%src%", image_source)
        .replace("%alt%", page_title); // Use page title as alt text
    Ok(xhtml)
}

/// A generator for creating EPUB files with images.
///
/// This struct wraps the `EpubBuilder` functionality and implements the `Generator` trait
/// to provide a standardized interface for creating EPUB documents with images.
pub struct EPub {
    epub: EpubBuilder<ZipLibrary>,
    output_path: PathBuf,
    filename_base: String,
    reading_direction: Direction,
}

impl EPub {
    /// Sets the cover image for the EPUB file.
    ///
    /// # Arguments
    ///
    /// * `cover_image_path` - Path to the cover image file
    ///
    /// # Returns
    ///
    /// * `Result<&mut Self>` - Self reference for method chaining or an error
    pub fn set_cover(&mut self, cover_image_path: &PathBuf) -> Result<&mut Self> {
        // Normalize the cover image path to handle long paths and special characters
        let normalized_path = normalize_path(cover_image_path).map_err(|e| {
            Error::InvalidPath(
                cover_image_path.clone(),
                format!("Failed to normalize cover image path: {}", e),
            )
        })?;

        let (cover_extension, cover_mime) = get_file_info(&normalized_path)?;

        let cover_file = File::open(&normalized_path).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open cover image '{}': {}",
                    path_to_string_lossy(&normalized_path),
                    e
                ),
            ))
        })?;

        // Add cover image as `cover.ext` inside `images/` directory
        let internal_cover_path = format!("images/cover.{}", cover_extension);
        self.epub
            .add_cover_image(internal_cover_path, cover_file, cover_mime)?;
        Ok(self)
    }

    /// Adds a chapter containing multiple image pages to the EPUB.
    ///
    /// # Arguments
    ///
    /// * `chapter_index` - 1-based chapter index for ordering
    /// * `chapter_title` - The title of the chapter (for TOC)
    /// * `image_paths` - Vector of paths to the images in this chapter
    ///
    /// # Returns
    ///
    /// * `Result<&mut Self>` - Self reference for method chaining or an error
    pub async fn add_chapter(
        &mut self,
        chapter_index: usize,
        chapter_title: &str,
        image_paths: &[PathBuf],
    ) -> Result<&mut Self> {
        let mut page_xhtml_files = Vec::new(); // To build chapter content in TOC
        let chapter_base_path = format!("chapters/chapter_{:03}", chapter_index);

        for (i, path) in image_paths.iter().enumerate() {
            let (image_extension, _image_mime) = get_file_info(path)?;

            // Internal path for the image within the EPUB
            let image_name_in_epub = format!(
                "{}/page_{:03}.{}",
                chapter_base_path,
                i + 1,
                image_extension
            );
            let page_title = format!("{} - Page {}", chapter_title, i + 1);
            let xhtml_content = generate_xhtml(&image_name_in_epub, &page_title)?;

            // Add the image resource to the EPUB
            self.add_resource_mmap(&image_name_in_epub, path).await?;

            // Add XHTML content for the page
            let xhtml_file_name = format!("{}/page_{:03}.xhtml", chapter_base_path, i + 1);
            self.epub.add_content(
                EpubContent::new(xhtml_file_name.clone(), xhtml_content.as_bytes())
                    .title(&page_title), // Title for TOC
            )?;

            page_xhtml_files.push(xhtml_file_name);
        }
        Ok(self)
    }

    /// Adds a resource to the EPUB using memory mapping for efficient handling of large files.
    ///
    /// # Arguments
    ///
    /// * `resource_path` - Path where the resource will be stored in the EPUB (e.g., "images/chapter1/page001.jpg")
    /// * `image_path` - Path to the image file on the filesystem
    ///
    /// # Returns
    ///
    /// * `Result<&mut Self>` - Self reference for method chaining or an error
    pub async fn add_resource_mmap(
        &mut self,
        resource_path: &str,
        image_path: &PathBuf,
    ) -> Result<&mut Self> {
        // Normalize the image path to handle long paths and special characters
        let normalized_path = normalize_path(image_path).map_err(|e| {
            Error::InvalidPath(
                image_path.clone(),
                format!("Failed to normalize image path: {}", e),
            )
        })?;

        let (_, image_mime) = get_file_info(&normalized_path)?;

        // Open the file asynchronously using the normalized path
        let file = tokio::fs::File::open(&normalized_path).await.map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open image file '{}': {}",
                    path_to_string_lossy(&normalized_path),
                    e
                ),
            ))
        })?;

        let file_std = file.into_std().await;
        let epub_ref = &mut self.epub;
        let path = resource_path.to_string();
        let mime = image_mime.to_string();

        let mmap = spawn_blocking(move || unsafe { MmapOptions::new().map(&file_std) })
            .await
            .map_err(|e| Error::AsyncTaskError(e.to_string()))??;

        // Add resource directly from memory-mapped data
        epub_ref.add_resource(&path, Cursor::new(&mmap[..]), &mime)?;

        Ok(self)
    }
}

#[async_trait]
impl Generator for EPub {
    fn new(output_dir: &Path, filename_base: &str) -> Result<Self> {
        let mut epub = EpubBuilder::new(ZipLibrary::new()?)?;

        epub.epub_version(EpubVersion::V30);

        epub.stylesheet(include_bytes!("../../templates/Epub.css").as_slice())?;

        // Normalize the output directory path to handle long paths
        let normalized_output_dir = normalize_path(output_dir)?;

        // Ensure output directory exists
        if !normalized_output_dir.exists() {
            std::fs::create_dir_all(&normalized_output_dir)?;
        }

        Ok(EPub {
            epub,
            output_path: normalized_output_dir,
            filename_base: filename_base.to_string(),
            reading_direction: Direction::Ltr, // Default, will be updated by set_metadata
        })
    }

    async fn add_page(&mut self, image_path: &PathBuf) -> Result<&mut Self> {
        let (image_extension, _) = get_file_info(image_path)?;

        // This `add_page` is for flat content where each page is its own "chapter" in EPUB context
        let page_index = 0; // Simplified index for this
        let chapter_idx = 1;

        let image_name = format!(
            "images/{}/page_{:03}.{}",
            chapter_idx,
            page_index + 1,
            image_extension
        );

        let page_title = format!("Page {}", page_index + 1);
        let xhtml_content = generate_xhtml(&image_name, &page_title)?;

        self.add_resource_mmap(&image_name, image_path).await?;

        let content_path = format!("chapter_1/page_{:03}.xhtml", page_index + 1);
        self.epub.add_content(
            EpubContent::new(content_path.clone(), xhtml_content.as_bytes()).title(&page_title),
        )?;

        Ok(self)
    }

    async fn set_metadata(
        &mut self,
        _file_name_base: &str, // Passed to `new`, used for filename already
        file_volume_number: Option<usize>,
        series_metadata: &EbookMetadata,
        _total_pages_in_file: usize,
        _collected_chapter_titles: &[String],
    ) -> Result<&mut Self> {
        // Main Title (use the specific title for this output file)
        let mut full_title = series_metadata.title.clone();
        if let Some(series) = &series_metadata.series {
            full_title = format!("{} - {}", series, series_metadata.title);
        }
        if let Some(vol_num) = file_volume_number {
            full_title = format!("{} Vol {}", full_title, vol_num);
        }
        self.epub.metadata("title", &full_title)?;

        // Series Title (if different from main title)
        if let Some(series_title) = &series_metadata.series {
            if series_title != &series_metadata.title {
                self.epub.metadata("series", series_title)?;
            }
        }

        // Creators/Authors
        for author in &series_metadata.authors {
            self.epub.metadata("creator", author)?;
        }
        self.epub.set_lang(&series_metadata.language);

        self.epub
            .metadata("direction", self.reading_direction.to_string())?;

        // Description
        if let Some(description) = &series_metadata.description {
            self.epub.metadata("description", description)?;
        }
        // Publisher
        if let Some(publisher) = &series_metadata.publisher {
            self.epub.metadata("publisher", publisher)?;
        }
        // Rights
        if let Some(rights) = &series_metadata.rights {
            self.epub.metadata("rights", rights)?;
        }
        // Identifier
        if let Some(identifier) = &series_metadata.identifier {
            self.epub.metadata("identifier", identifier)?;
        }
        // Release Date
        if let Some(release_date) = &series_metadata.release_date {
            self.epub.metadata("date", &release_date.to_rfc3339())?;
        }
        // Tags
        for tag in &series_metadata.tags {
            self.epub.metadata("subject", tag)?;
        }

        // Custom fields (EPUB doesn't have a direct "custom field" area like ComicInfo.xml,
        // but we can add them as meta properties or additional subjects if meaningful)
        for (key, value) in &series_metadata.custom_fields {
            self.epub.metadata(key, value)?; // Attempt to add as generic metadata
        }

        Ok(self)
    }

    async fn save(mut self) -> Result<()> {
        let output_file_path = self
            .output_path
            .join(format!("{}.epub", self.filename_base));

        // Normalize the output file path as well
        let normalized_output_file = normalize_path(&output_file_path)?;

        let file = File::create(&normalized_output_file).map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to create EPUB file '{}': {}",
                    path_to_string_lossy(&normalized_output_file),
                    e
                ),
            ))
        })?;

        self.epub.generate(file)?;
        Ok(())
    }
}
