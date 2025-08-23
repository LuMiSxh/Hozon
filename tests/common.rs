//! Common test utilities and constants for the Hozon crate.
//!
//! Provides functions for setting up and tearing down test directories,
//! creating dummy image files, and shared test constants.

use hozon::error::{Error, Result};
use image::{Rgb, RgbImage};
use rand::{Rng, distributions::Alphanumeric};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;

#[allow(dead_code)]
pub const TEST_TMP_DIR: &str = "tests/tmp";
#[allow(dead_code)]
pub const TEST_TIMEOUT: Duration = Duration::from_secs(30);
#[allow(dead_code)]
pub const LONG_TEST_TIMEOUT: Duration = Duration::from_secs(120); // For full conversions if they are slow

/// Helper function to create a clean test directory with source and target subdirectories.
/// Ensures the base directory is empty before a test runs.
/// Returns the base test path, the source path, and the target path.
#[allow(dead_code)]
pub async fn setup_test_dirs(sub_path: &str) -> (PathBuf, PathBuf, PathBuf) {
    let rand_string: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect();
    let unique_sub_path = format!("{}-{}", sub_path, rand_string);
    let test_dir = PathBuf::from(TEST_TMP_DIR).join(unique_sub_path);
    if test_dir.exists() {
        fs::remove_dir_all(&test_dir).await.unwrap();
    }
    let source_dir = test_dir.join("source");
    let target_dir = test_dir.join("target");

    fs::create_dir_all(&source_dir).await.unwrap();
    fs::create_dir_all(&target_dir).await.unwrap();

    (test_dir, source_dir, target_dir)
}

/// Helper function to clean up the entire test temporary directory.
#[allow(dead_code)]
pub async fn cleanup_all_test_dirs() {
    let test_dir = PathBuf::from(TEST_TMP_DIR);
    if test_dir.exists() {
        let _ = fs::remove_dir_all(&test_dir).await;
    }
}

/// Creates a minimal dummy JPEG image at the given path.
#[allow(dead_code)]
pub async fn create_dummy_image(path: &Path, color: Rgb<u8>) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }
    let mut img = RgbImage::new(100, 100);
    for x in 0..100 {
        for y in 0..100 {
            img.put_pixel(x, y, color);
        }
    }
    let path_clone = path.to_path_buf();
    tokio::task::spawn_blocking(move || img.save_with_format(path_clone, image::ImageFormat::Jpeg))
        .await
        .map_err(|e| Error::AsyncTaskError(e.to_string()))?
        .map_err(Error::Image)?;
    Ok(())
}

/// Creates a dummy grayscale JPEG image at the given path.
#[allow(dead_code)]
pub async fn create_dummy_grayscale_image(path: &Path) -> Result<()> {
    create_dummy_image(path, Rgb([128, 128, 128])).await
}

/// Creates a dummy color JPEG image at the given path.
#[allow(dead_code)]
pub async fn create_dummy_color_image(path: &Path) -> Result<()> {
    create_dummy_image(path, Rgb([255, 0, 0])).await // Red
}

/// Checks if a ZIP file (CBZ or EPUB) exists and contains at least one entry.
#[allow(dead_code)]
pub async fn assert_valid_zip_file(path: &Path) {
    assert!(path.exists(), "Output ZIP file does not exist: {:?}", path);
    assert!(path.is_file(), "Output ZIP path is not a file: {:?}", path);

    let file = fs::File::open(path).await.unwrap();
    let file_std = file.into_std().await;
    let zip = zip::ZipArchive::new(file_std).unwrap();
    assert!(zip.len() > 0, "Output ZIP file is empty: {:?}", path);
}

/// Reads the ComicInfo.xml from a CBZ file and returns its content.
#[allow(dead_code)]
pub async fn get_comic_info_xml(cbz_path: &Path) -> String {
    let file = fs::File::open(cbz_path).await.unwrap();
    let file_std = file.into_std().await;
    let mut archive = zip::ZipArchive::new(file_std).unwrap();
    let mut file = archive.by_name("ComicInfo.xml").unwrap();
    let mut content = String::new();
    std::io::Read::read_to_string(&mut file, &mut content).unwrap();
    content
}
