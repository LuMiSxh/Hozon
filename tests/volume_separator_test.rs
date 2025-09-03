//! Tests for custom volume separator functionality
//!
//! These tests verify that the volume_separator configuration option works correctly
//! for customizing the separator between series title and volume numbers in filenames.
//! When multiple volumes are generated, the format is: "{title}{separator}Volume {number}.{ext}"

use hozon::error::Result;
use hozon::prelude::*;
use tokio::time::timeout;

mod common;
use common::{LONG_TEST_TIMEOUT, assert_valid_zip_file, create_dummy_color_image, setup_test_dirs};

/// Test using pipe separator " | " (gets sanitized to dash due to Windows compatibility)
#[tokio::test]
async fn test_custom_volume_separator_pipe() -> Result<()> {
    let test_dirs = setup_test_dirs("volume_separator_pipe").await;

    // Setup: Create multiple chapters to trigger volume creation
    create_dummy_color_image(&test_dirs.source_dir.join("01-001").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("01-002").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("02-001").join("img.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Pipe Separator Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .volume_separator(" | ".to_string()) // Custom pipe separator
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Pipe Separator Series");
    assert!(expected_output_dir.exists());

    // Should create files with pipe separator (but sanitized to dash)
    let vol1_cbz = expected_output_dir.join("Pipe Separator Series - Volume 1.cbz");
    let vol2_cbz = expected_output_dir.join("Pipe Separator Series - Volume 2.cbz");
    assert_valid_zip_file(&vol1_cbz).await;
    assert_valid_zip_file(&vol2_cbz).await;

    Ok(())
}

/// Test using underscore separator "_" (remains as underscore since it's valid)
#[tokio::test]
async fn test_custom_volume_separator_underscore() -> Result<()> {
    let test_dirs = setup_test_dirs("volume_separator_underscore").await;

    // Setup: Create multiple chapters to trigger volume creation
    create_dummy_color_image(&test_dirs.source_dir.join("01-001").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("01-002").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("02-001").join("img.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Underscore Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .volume_separator("_".to_string()) // Custom underscore separator
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Underscore Series");
    assert!(expected_output_dir.exists());

    // Should create files with underscore separator
    let vol1_cbz = expected_output_dir.join("Underscore Series_Volume 1.cbz");
    let vol2_cbz = expected_output_dir.join("Underscore Series_Volume 2.cbz");
    assert_valid_zip_file(&vol1_cbz).await;
    assert_valid_zip_file(&vol2_cbz).await;

    Ok(())
}

/// Test using just space " " separator for minimal separation
#[tokio::test]
async fn test_custom_volume_separator_space() -> Result<()> {
    let test_dirs = setup_test_dirs("volume_separator_space").await;

    // Setup: Create multiple chapters to trigger volume creation
    create_dummy_color_image(&test_dirs.source_dir.join("01-001").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("01-002").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("02-001").join("img.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Space Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .volume_separator(" ".to_string()) // Just a space separator
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Space Series");
    assert!(expected_output_dir.exists());

    // Should create files with just space separator
    let vol1_cbz = expected_output_dir.join("Space Series Volume 1.cbz");
    let vol2_cbz = expected_output_dir.join("Space Series Volume 2.cbz");
    assert_valid_zip_file(&vol1_cbz).await;
    assert_valid_zip_file(&vol2_cbz).await;

    Ok(())
}

/// Test the default separator " - " when no custom separator is specified
#[tokio::test]
async fn test_default_volume_separator() -> Result<()> {
    let test_dirs = setup_test_dirs("volume_separator_default").await;

    // Setup: Create multiple chapters to trigger volume creation
    create_dummy_color_image(&test_dirs.source_dir.join("01-001").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("01-002").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("02-001").join("img.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Default Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        // No custom separator - should use default " - "
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Default Series");
    assert!(expected_output_dir.exists());

    // Should create files with default " - " separator
    let vol1_cbz = expected_output_dir.join("Default Series - Volume 1.cbz");
    let vol2_cbz = expected_output_dir.join("Default Series - Volume 2.cbz");
    assert_valid_zip_file(&vol1_cbz).await;
    assert_valid_zip_file(&vol2_cbz).await;

    Ok(())
}

/// Test that single volumes don't use the separator (no volume numbering)
#[tokio::test]
async fn test_single_volume_no_separator() -> Result<()> {
    let test_dirs = setup_test_dirs("single_volume_no_separator").await;

    // Setup: Create a single chapter (should not trigger volume numbering)
    create_dummy_color_image(&test_dirs.source_dir.join("chapter").join("img1.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("chapter").join("img2.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Single Volume".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_separator(" | ".to_string()) // Custom separator should be ignored for single volume
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Single Volume");
    assert!(expected_output_dir.exists());

    // Should create file without volume number or separator
    let single_cbz = expected_output_dir.join("Single Volume.cbz");
    assert_valid_zip_file(&single_cbz).await;

    Ok(())
}
