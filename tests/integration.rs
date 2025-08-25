//! Integration tests for the Hozon crate.
//!
//! These tests run full conversion pipelines from setup to output validation.

use chrono::{TimeZone, Utc};
use hozon::error::Result;
use hozon::prelude::*;
use std::collections::HashMap;
use tokio::time::timeout;

mod common;
use common::{
    LONG_TEST_TIMEOUT, assert_valid_zip_file, create_dummy_color_image,
    create_dummy_grayscale_image, get_comic_info_xml, setup_test_dirs,
};

#[tokio::test]
async fn test_full_pipeline_default_deep_cbz() -> Result<()> {
    let test_dirs = setup_test_dirs("full_pipeline_default_cbz").await;

    // Setup: source/Chapter 1/page_001.jpg, source/Chapter 2/page_001.jpg
    create_dummy_color_image(&test_dirs.source_dir.join("Chapter 1").join("001.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("Chapter 2").join("001.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "My Default Comic".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .create_output_directory(true) // Should create target/My Default Comic/
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("My Default Comic");
    assert!(expected_output_dir.exists());

    // Default strategy is Manual, which creates one volume by default.
    let expected_cbz_path = expected_output_dir.join("My Default Comic.cbz");
    assert_valid_zip_file(&expected_cbz_path).await;

    // Check ComicInfo.xml
    let comic_info = get_comic_info_xml(&expected_cbz_path).await;
    assert!(comic_info.contains("<Title>My Default Comic</Title>"));
    assert!(comic_info.contains("<PageCount>2</PageCount>"));
    Ok(())
}

#[tokio::test]
async fn test_flat_pages_workflow_epub() -> Result<()> {
    let test_dirs = setup_test_dirs("flat_pages_epub").await;

    // Setup: source_flat/001.jpg, 002.jpg
    create_dummy_color_image(&test_dirs.source_dir.join("001.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("002.jpg")).await?;

    // Manually collect the flat pages
    let collected_data = vec![vec![
        test_dirs.source_dir.join("001.jpg"),
        test_dirs.source_dir.join("002.jpg"),
    ]];

    let config = HozonConfig::builder()
        .metadata(EbookMetadata {
            title: "Flat Pages Book".to_string(),
            language: "ja".to_string(),
            ..Default::default()
        })
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::Flat)
        .build()?;

    timeout(
        LONG_TEST_TIMEOUT,
        config.convert_from_collected_data(collected_data),
    )
    .await
    .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Flat Pages Book");
    assert!(expected_output_dir.exists());
    let expected_epub_path = expected_output_dir.join("Flat Pages Book.epub");
    assert_valid_zip_file(&expected_epub_path).await;
    Ok(())
}

#[tokio::test]
async fn test_name_grouping_strategy_cbz() -> Result<()> {
    let test_dirs = setup_test_dirs("name_grouping_cbz").await;

    // Setup:
    // source_names/01-001/img_001.jpg
    // source_names/01-002/img_001.jpg
    // source_names/02-001/img_001.jpg
    create_dummy_color_image(&test_dirs.source_dir.join("01-001").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("01-002").join("img.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("02-001").join("img.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "My Name Grouped Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .volume_grouping_strategy(VolumeGroupingStrategy::Name)
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("My Name Grouped Series");
    assert!(expected_output_dir.exists());

    // Expecting 2 CBZ files
    let vol1_cbz = expected_output_dir.join("My Name Grouped Series | Volume 1.cbz");
    let vol2_cbz = expected_output_dir.join("My Name Grouped Series | Volume 2.cbz");
    assert_valid_zip_file(&vol1_cbz).await;
    assert_valid_zip_file(&vol2_cbz).await;

    let comic_info_vol1 = get_comic_info_xml(&vol1_cbz).await;
    assert!(comic_info_vol1.contains("<Title>My Name Grouped Series</Title>"));
    assert!(comic_info_vol1.contains("<Number>1</Number>"));
    assert!(comic_info_vol1.contains("<PageCount>2</PageCount>"));

    let comic_info_vol2 = get_comic_info_xml(&vol2_cbz).await;
    assert!(comic_info_vol2.contains("<Title>My Name Grouped Series</Title>"));
    assert!(comic_info_vol2.contains("<Number>2</Number>"));
    assert!(comic_info_vol2.contains("<PageCount>1</PageCount>"));
    Ok(())
}

#[tokio::test]
async fn test_image_analysis_grouping_epub() -> Result<()> {
    let test_dirs = setup_test_dirs("image_analysis_epub").await;

    // Setup:
    //   001-Chapter_A/cover.jpg (grayscale)
    //   002-Chapter_B/cover.jpg (color, implies new volume)
    //   003-Chapter_C/cover.jpg (grayscale)
    create_dummy_grayscale_image(&test_dirs.source_dir.join("001-Chapter_A").join("cover.jpg"))
        .await?;
    create_dummy_color_image(&test_dirs.source_dir.join("002-Chapter_B").join("cover.jpg")).await?;
    create_dummy_grayscale_image(&test_dirs.source_dir.join("003-Chapter_C").join("cover.jpg"))
        .await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Image Analysis Series".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::ImageAnalysis)
        .image_analysis_sensibility(90) // High sensibility means strict grayscale
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Image Analysis Series");
    assert!(expected_output_dir.exists());

    // Expected logic:
    // Vol 1 starts at Chapter A (index 0) because it's the first chapter.
    // Vol 2 starts at Chapter B (index 1) because its cover is color.
    // Chapter C (index 2) is part of Vol 2.
    // Result: Vol 1 has 1 chapter (A), Vol 2 has 2 chapters (B, C).
    let vol1_epub = expected_output_dir.join("Image Analysis Series | Volume 1.epub");
    let vol2_epub = expected_output_dir.join("Image Analysis Series | Volume 2.epub");
    assert_valid_zip_file(&vol1_epub).await;
    assert_valid_zip_file(&vol2_epub).await;
    Ok(())
}

#[tokio::test]
async fn test_manual_grouping_with_override_epub() -> Result<()> {
    let test_dirs = setup_test_dirs("manual_grouping_override_epub").await;

    // Setup: 4 chapters, each with one page
    for i in 1..=4 {
        create_dummy_color_image(
            &test_dirs
                .source_dir
                .join(format!("Chapter_{}", i))
                .join("p1.jpg"),
        )
        .await?;
    }

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Manual Grouping Book".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Epub)
        .volume_grouping_strategy(VolumeGroupingStrategy::Manual)
        .volume_sizes_override(vec![2, 2]) // Manual override: 2 volumes, 2 chapters each
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Manual Grouping Book");
    assert!(expected_output_dir.exists());

    let vol1_epub = expected_output_dir.join("Manual Grouping Book | Volume 1.epub");
    let vol2_epub = expected_output_dir.join("Manual Grouping Book | Volume 2.epub");
    assert_valid_zip_file(&vol1_epub).await;
    assert_valid_zip_file(&vol2_epub).await;
    Ok(())
}

#[tokio::test]
async fn test_metadata_propagation_and_custom_fields_cbz() -> Result<()> {
    let test_dirs = setup_test_dirs("metadata_cbz").await;

    create_dummy_color_image(&test_dirs.source_dir.join("Chapter 1").join("001.jpg")).await?;

    let mut custom_fields = HashMap::new();
    custom_fields.insert("CustomTag".to_string(), "Custom Value".to_string());

    let metadata = EbookMetadata {
        title: "Metadata Test Comic".to_string(),
        series: Some("The Metadata Saga".to_string()),
        authors: vec!["Author McAuthorface".to_string()],
        description: Some("This is a test comic.".to_string()),
        publisher: Some("Test Publisher".to_string()),
        language: "es".to_string(),
        genre: Some("Comedy".to_string()),
        web: Some("https://example.com/web".to_string()),
        tags: vec!["test".to_string(), "metadata".to_string()],
        release_date: Some(Utc.with_ymd_and_hms(2025, 8, 23, 10, 30, 0).unwrap()),
        custom_fields,
        ..Default::default()
    };

    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("Metadata Test Comic");
    let expected_cbz_path = expected_output_dir.join("Metadata Test Comic.cbz");
    assert_valid_zip_file(&expected_cbz_path).await;

    let comic_info = get_comic_info_xml(&expected_cbz_path).await;

    assert!(comic_info.contains("<Title>Metadata Test Comic</Title>"));
    assert!(comic_info.contains("<Series>The Metadata Saga</Series>"));
    assert!(comic_info.contains("<Writer>Author McAuthorface</Writer>"));
    assert!(comic_info.contains("<Publisher>Test Publisher</Publisher>"));
    assert!(comic_info.contains("<Genre>Comedy</Genre>"));
    assert!(comic_info.contains("<Web>https://example.com/web</Web>"));
    assert!(comic_info.contains("<PageCount>1</PageCount>"));
    assert!(comic_info.contains("<Language>es</Language>"));
    assert!(comic_info.contains("<Summary>This is a test comic.</Summary>"));
    assert!(comic_info.contains("Tags: test, metadata"));
    assert!(comic_info.contains("<Year>2025</Year>"));
    assert!(comic_info.contains("<Month>8</Month>"));
    assert!(comic_info.contains("<Day>23</Day>"));
    assert!(comic_info.contains("CustomTag: Custom Value"));
    Ok(())
}

#[tokio::test]
async fn test_metadata_xml_escaping_cbz() -> Result<()> {
    let test_dirs = setup_test_dirs("xml_escaping_cbz").await;

    create_dummy_color_image(&test_dirs.source_dir.join("Chapter 1").join("001.jpg")).await?;

    let mut custom_fields = HashMap::new();
    custom_fields.insert(
        "Tag<WithBrackets>".to_string(),
        "Value & \"quoted\"".to_string(),
    );
    custom_fields.insert(
        "Another'Tag".to_string(),
        "<script>alert('xss')</script>".to_string(),
    );

    let metadata = EbookMetadata {
        title: "XML Escaping Test".to_string(),
        description: Some("Description with <html> & \"quotes\"".to_string()),
        custom_fields,
        ..Default::default()
    };

    let config = HozonConfig::builder()
        .metadata(metadata)
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .output_format(FileFormat::Cbz)
        .build()?;

    timeout(LONG_TEST_TIMEOUT, config.convert_from_source())
        .await
        .expect("Test timed out")?;

    let expected_output_dir = test_dirs.target_dir.join("XML Escaping Test");
    let expected_cbz_path = expected_output_dir.join("XML Escaping Test.cbz");
    assert_valid_zip_file(&expected_cbz_path).await;

    let comic_info = get_comic_info_xml(&expected_cbz_path).await;

    // Verify XML escaping in description
    assert!(
        comic_info
            .contains("<Summary>Description with &lt;html&gt; &amp; &quot;quotes&quot;</Summary>")
    );

    // Verify custom fields are properly escaped in Notes section
    assert!(comic_info.contains("Tag&lt;WithBrackets&gt;: Value &amp; &quot;quoted&quot;"));
    assert!(
        comic_info
            .contains("Another&apos;Tag: &lt;script&gt;alert(&apos;xss&apos;)&lt;/script&gt;")
    );

    Ok(())
}

#[tokio::test]
async fn test_analyze_source_functionality() -> Result<()> {
    let test_dirs = setup_test_dirs("analyze_source").await;

    // Setup: Create chapters with different characteristics for analysis
    let chapter1_dir = test_dirs.source_dir.join("01-001_Chapter_One");
    let chapter2_dir = test_dirs.source_dir.join("01-002_Chapter_Two");
    let chapter3_dir = test_dirs.source_dir.join("01-003_Chapter_Three");

    // Chapter 1: 10 pages (normal)
    for i in 1..=10 {
        create_dummy_color_image(&chapter1_dir.join(format!("page_{:03}.jpg", i))).await?;
    }

    // Chapter 2: 9 pages (normal, similar to chapter 1)
    for i in 1..=9 {
        create_dummy_color_image(&chapter2_dir.join(format!("page_{:03}.jpg", i))).await?;
    }

    // Chapter 3: Only 1 page (significantly different) and special characters
    create_dummy_color_image(&chapter3_dir.join("page<001>.jpg")).await?;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title(
            "Analysis Test".to_string(),
        ))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .build()?;

    // Test analyze_source method
    let collected_content = timeout(LONG_TEST_TIMEOUT, config.analyze_source())
        .await
        .expect("Test timed out")?;

    // Verify the analysis results
    assert_eq!(collected_content.chapters_with_pages.len(), 3);
    assert!(!collected_content.report.findings.is_empty());

    // Check that consistent naming was detected
    let has_consistent_naming = collected_content
        .report
        .findings
        .iter()
        .any(|f| matches!(f, AnalyzeFinding::ConsistentNamingFound { .. }));
    assert!(has_consistent_naming);

    // Check that special characters were detected
    let has_special_chars = collected_content
        .report
        .findings
        .iter()
        .any(|f| matches!(f, AnalyzeFinding::SpecialCharactersInPath { .. }));
    assert!(has_special_chars);

    // Check that inconsistent page count was detected
    let has_inconsistent_pages = collected_content
        .report
        .findings
        .iter()
        .any(|f| matches!(f, AnalyzeFinding::InconsistentPageCount { .. }));
    assert!(has_inconsistent_pages);

    // Verify recommended strategy is set
    assert_ne!(
        collected_content.report.recommended_strategy,
        VolumeGroupingStrategy::Manual
    );

    Ok(())
}

#[tokio::test]
async fn test_error_on_non_existent_source() -> Result<()> {
    let test_dirs = setup_test_dirs("error_no_source").await;
    let non_existent_source = test_dirs.test_dir.join("non_existent_source");

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Error Test".to_string()))
        .source_path(non_existent_source)
        .target_path(test_dirs.target_dir.clone())
        .build()?;

    let result = config.convert_from_source().await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        hozon::error::Error::NotFound(_)
    ));
    Ok(())
}

#[tokio::test]
async fn test_error_on_empty_collected_data() -> Result<()> {
    let test_dirs = setup_test_dirs("error_empty_collected").await;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Error Test".to_string()))
        .target_path(test_dirs.target_dir.clone())
        .build()?;

    let collected_data: Vec<Vec<PathBuf>> = Vec::new();
    let result = config.convert_from_collected_data(collected_data).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Provided collected data is empty")
    );
    Ok(())
}

#[tokio::test]
async fn test_error_on_empty_structured_data() -> Result<()> {
    let test_dirs = setup_test_dirs("error_empty_structured").await;

    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Error Test".to_string()))
        .target_path(test_dirs.target_dir.clone())
        .build()?;

    let structured_data: Vec<Vec<Vec<PathBuf>>> = Vec::new();
    let result = config.convert_from_structured_data(structured_data).await;
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Provided structured data is empty")
    );
    Ok(())
}
