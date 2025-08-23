use crate::error::{Error, Result};
use crate::generator::Generator;
use crate::types::{EbookMetadata, get_file_info};
use async_trait::async_trait;
use chrono::prelude::*;
use memmap2::MmapOptions;
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
}

#[async_trait]
impl Generator for Cbz {
    fn new(output_dir: &Path, base_filename: &str) -> Result<Self> {
        let options: SimpleFileOptions = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(0o755);

        // Ensure output directory exists
        if !output_dir.exists() {
            std::fs::create_dir_all(output_dir)?;
        }

        let output_file_path = output_dir.join(format!("{}.cbz", base_filename));

        let file = File::create(&output_file_path)?;

        let zip = ZipWriter::new(file);

        Ok(Cbz {
            zip: Some(zip),
            options,
            page_index: 0,
        })
    }

    async fn add_page(&mut self, image_path: &PathBuf) -> Result<&mut Self> {
        let (image_extension, _) = get_file_info(image_path)?;

        // Open the file
        let file = fs::File::open(image_path).await?;

        let file_std = file.into_std().await;
        let options = self.options;
        let file_name = format!("page_{:03}.{}", self.page_index + 1, image_extension);

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

        // Basic fields
        xml = xml.replace("%title%", &series_metadata.title);
        xml = xml.replace("%series%", series_metadata.series.as_deref().unwrap_or(""));
        xml = xml.replace("%volume%", &file_volume_number.unwrap_or(1).to_string());
        xml = xml.replace("%pagecount%", &total_pages_in_file.to_string());
        xml = xml.replace(
            "%description%",
            series_metadata.description.as_deref().unwrap_or(""),
        );
        xml = xml.replace("%language%", &series_metadata.language);
        xml = xml.replace(
            "%publisher%",
            series_metadata.publisher.as_deref().unwrap_or(""),
        );
        xml = xml.replace(
            "%identifier%",
            series_metadata.identifier.as_deref().unwrap_or(""),
        );
        xml = xml.replace("%rights%", series_metadata.rights.as_deref().unwrap_or(""));
        xml = xml.replace("%web%", series_metadata.web.as_deref().unwrap_or(""));
        xml = xml.replace("%genre%", series_metadata.genre.as_deref().unwrap_or(""));

        // Authors (as one comma-separated string for "Writer" and "Penciller" if applicable)
        let authors_str = series_metadata.authors.join(", ");
        xml = xml.replace("%writer%", &authors_str);
        xml = xml.replace("%penciller%", &authors_str);
        xml = xml.replace("%inker%", &authors_str);
        xml = xml.replace("%colorist%", &authors_str);
        xml = xml.replace("%letterer%", &authors_str);

        // Tags
        xml = xml.replace("%tags%", &series_metadata.tags.join(", "));

        // Dates
        let now_utc = Utc::now();
        let release_date = series_metadata.release_date.unwrap_or(now_utc);
        xml = xml.replace("%year%", &release_date.year().to_string());
        xml = xml.replace("%month%", &release_date.month().to_string());
        xml = xml.replace("%day%", &release_date.day().to_string());

        // Custom fields (can be added as <Genre> tags or custom tags if ComicInfo.xml supports)
        let custom_fields_xml: String = series_metadata
            .custom_fields
            .iter()
            .map(|(key, value)| {
                format!("<{}>{}</{}>\n", key, value, key) // TODO: This might not be valid for ComicInfo.xml schema
            })
            .collect();
        xml = xml.replace("%customfields%", &custom_fields_xml);

        // Chapter titles (can be added as a comment or custom tag)
        let chapter_titles_str = collected_chapter_titles.join(", ");
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
