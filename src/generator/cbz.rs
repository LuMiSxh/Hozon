use crate::error::{Error, Result};
use crate::generator::Generator;
use crate::path_utils::{normalize_path, path_to_string_lossy};
use crate::types::{EbookMetadata, get_file_info};
use async_trait::async_trait;
use chrono::prelude::*;
use memmap2::MmapOptions;
use rayon::prelude::*;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::task::spawn_blocking;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A generator for creating CBZ (Comic Book ZIP) files.
///
/// This struct implements the `Generator` trait to package images into
/// a properly formatted CBZ archive with optional metadata (ComicInfo.xml).
pub struct Cbz {
    zip: Option<ZipWriter<File>>,
    options: SimpleFileOptions,
    page_index: usize, // 0-based index for pages added
    has_cover: bool,   // Track if a custom cover has been added
}

impl Cbz {
    /// Adds a custom cover page to the CBZ archive.
    /// This will be added as "000_cover.jpg" and should be called before adding regular pages.
    pub async fn add_cover_page(&mut self, cover_path: &PathBuf) -> Result<&mut Self> {
        if self.has_cover {
            return Err(Error::Unsupported("Cover already set".to_string()));
        }

        // Normalize the cover path to handle long paths and special characters
        let normalized_path = normalize_path(cover_path).map_err(|e| {
            Error::InvalidPath(
                cover_path.clone(),
                format!("Failed to normalize cover path: {}", e),
            )
        })?;

        let (cover_extension, _) = get_file_info(&normalized_path)?;

        // Open the file using the normalized path
        let file = fs::File::open(&normalized_path).await.map_err(|e| {
            Error::Io(std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to open cover file '{}': {}",
                    path_to_string_lossy(&normalized_path),
                    e
                ),
            ))
        })?;

        let file_std = file.into_std().await;
        let options = self.options;
        let cover_file_name = format!("000_cover.{}", cover_extension);

        let zip = match self.zip.as_mut() {
            Some(z) => z,
            None => {
                return Err(Error::Unsupported("Zip writer not available".to_string()));
            }
        };

        // Create the read-only memory map
        let mmap = match spawn_blocking(move || unsafe { MmapOptions::new().map(&file_std) })
            .await
            .map_err(|e| Error::AsyncTaskError(e.to_string()))?
        {
            Ok(map) => map,
            Err(e) => {
                return Err(e.into());
            }
        };

        // Add cover to zip
        zip.start_file(cover_file_name.clone(), options)?;
        zip.write_all(&mmap[..])?;

        self.has_cover = true;

        Ok(self)
    }
}

#[async_trait]
impl Generator for Cbz {
    fn new(output_dir: &Path, base_filename: &str) -> Result<Self> {
        let options: SimpleFileOptions = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o755);

        // Normalize the output directory path to handle long paths
        let normalized_output_dir = normalize_path(output_dir)?;

        // Ensure output directory exists
        if !normalized_output_dir.exists() {
            std::fs::create_dir_all(&normalized_output_dir)?;
        }

        let output_file_path = normalized_output_dir.join(format!("{}.cbz", base_filename));

        // Normalize the output file path as well
        let normalized_output_file = normalize_path(&output_file_path)?;

        let file = File::create(&normalized_output_file)?;

        let zip = ZipWriter::new(file);

        Ok(Cbz {
            zip: Some(zip),
            options,
            page_index: 0,
            has_cover: false,
        })
    }

    async fn add_page(&mut self, image_path: &PathBuf) -> Result<&mut Self> {
        // Normalize the image path to handle long paths and special characters
        let normalized_path = normalize_path(image_path).map_err(|e| {
            Error::InvalidPath(
                image_path.clone(),
                format!("Failed to normalize image path: {}", e),
            )
        })?;

        let (image_extension, _) = get_file_info(&normalized_path)?;

        // Open the file using the normalized path
        let file = fs::File::open(&normalized_path).await.map_err(|e| {
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
        let options = self.options;
        // If we have a cover, start numbering pages from 001, otherwise from 001 as well
        // but the cover would be 000_cover if present
        let page_number = if self.has_cover {
            self.page_index + 1
        } else {
            self.page_index + 1
        };
        let file_name = format!("page_{:03}.{}", page_number, image_extension);

        let zip = match self.zip.as_mut() {
            Some(z) => z,
            None => {
                return Err(Error::Unsupported("Zip writer not available".to_string()));
            }
        };

        // Create the read-only memory map
        let mmap = match spawn_blocking(move || unsafe { MmapOptions::new().map(&file_std) })
            .await
            .map_err(|e| Error::AsyncTaskError(e.to_string()))?
        {
            Ok(map) => map,
            Err(e) => {
                return Err(e.into());
            }
        };

        // Add to zip
        zip.start_file(file_name.clone(), options)?;

        zip.write_all(&mmap[..])?;

        // Increment page index
        self.page_index += 1;

        Ok(self)
    }

    async fn set_metadata(
        &mut self,
        _file_name_base: &str,
        file_volume_number: Option<usize>,
        series_metadata: &EbookMetadata,
        total_pages_in_file: usize,
        collected_chapter_titles: &[String],
    ) -> Result<&mut Self> {
        const TEMPLATE: &str = include_str!("../../templates/ComicInfo.xml");

        let mut xml = TEMPLATE.to_string();

        // Helper function to escape XML characters
        let escape_xml = |text: &str| -> String {
            text.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;")
                .replace('\'', "&apos;")
        };

        // Basic fields (with XML escaping)
        xml = xml.replace("%title%", &escape_xml(&series_metadata.title));
        xml = xml.replace(
            "%series%",
            &escape_xml(series_metadata.series.as_deref().unwrap_or("")),
        );
        xml = xml.replace("%volume%", &file_volume_number.unwrap_or(1).to_string());
        xml = xml.replace("%pagecount%", &total_pages_in_file.to_string());
        xml = xml.replace(
            "%description%",
            &escape_xml(series_metadata.description.as_deref().unwrap_or("")),
        );
        xml = xml.replace("%language%", &escape_xml(&series_metadata.language));
        xml = xml.replace(
            "%publisher%",
            &escape_xml(series_metadata.publisher.as_deref().unwrap_or("")),
        );
        xml = xml.replace(
            "%identifier%",
            &escape_xml(series_metadata.identifier.as_deref().unwrap_or("")),
        );
        xml = xml.replace(
            "%rights%",
            &escape_xml(series_metadata.rights.as_deref().unwrap_or("")),
        );
        xml = xml.replace(
            "%web%",
            &escape_xml(series_metadata.web.as_deref().unwrap_or("")),
        );
        xml = xml.replace(
            "%genre%",
            &escape_xml(series_metadata.genre.as_deref().unwrap_or("")),
        );

        // Authors (as one comma-separated string for "Writer" and "Penciller" if applicable)
        let authors_str = escape_xml(&series_metadata.authors.join(", "));
        xml = xml.replace("%writer%", &authors_str);
        xml = xml.replace("%penciller%", &authors_str);
        xml = xml.replace("%inker%", &authors_str);
        xml = xml.replace("%colorist%", &authors_str);
        xml = xml.replace("%letterer%", &authors_str);

        // Tags
        xml = xml.replace("%tags%", &escape_xml(&series_metadata.tags.join(", ")));

        // Dates
        let now_utc = Utc::now();
        let release_date = series_metadata.release_date.unwrap_or(now_utc);
        xml = xml.replace("%year%", &release_date.year().to_string());
        xml = xml.replace("%month%", &release_date.month().to_string());
        xml = xml.replace("%day%", &release_date.day().to_string());

        // Custom fields are safely embedded in the Notes section as key-value pairs
        // This follows ComicInfo.xml best practices for custom metadata
        let custom_fields_xml: String = if series_metadata.custom_fields.is_empty() {
            String::new()
        } else {
            series_metadata
                .custom_fields
                .par_iter()
                .map(|(key, value)| {
                    // Escape XML characters in key and value to prevent XML parsing issues
                    let escaped_key = escape_xml(key);
                    let escaped_value = escape_xml(value);
                    format!("    {}: {}", escaped_key, escaped_value)
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        xml = xml.replace("%customfields%", &custom_fields_xml);

        // Chapter titles (can be added as a comment or custom tag)
        let chapter_titles_str = escape_xml(&collected_chapter_titles.join(", "));
        xml = xml.replace("%chaptertitles%", &chapter_titles_str);

        let xml_bytes = spawn_blocking(move || xml.as_bytes().to_vec())
            .await
            .map_err(|e| Error::AsyncTaskError(e.to_string()))?;

        let zip = match self.zip.as_mut() {
            Some(z) => z,
            None => {
                return Err(Error::Unsupported("Zip writer not available".to_string()));
            }
        };

        // Add the metadata file to zip
        zip.start_file("ComicInfo.xml", self.options)?;

        zip.write_all(&xml_bytes)?;

        Ok(self)
    }

    async fn save(mut self) -> Result<()> {
        // Take ownership of the zip writer
        let zip = match self.zip.take() {
            Some(z) => z,
            None => {
                return Err(Error::Unsupported("Zip writer not available".to_string()));
            }
        };

        // Finish writing the zip file in a blocking task
        spawn_blocking(move || match zip.finish() {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::Zip(e)),
        })
        .await
        .map_err(|e| Error::AsyncTaskError(e.to_string()))??;

        Ok(())
    }
}
