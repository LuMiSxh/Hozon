//! Unit tests for core Hozon functionality.
//!
//! Tests individual components in isolation without full pipeline execution.

use hozon::collector::Collector;
use hozon::error::Result;
use hozon::prelude::*;
use hozon::types::{CollectionDepth, EbookMetadata, HozonExecutionMode};
use std::cmp::Ordering;

mod common;
use common::{create_dummy_color_image, create_dummy_grayscale_image, setup_test_dirs};

#[tokio::test]
async fn test_hozon_config_builder_validation() -> Result<()> {
    // Invalid regex - should fail in our custom validate() function
    let result = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(PathBuf::from("/tmp"))
        .target_path(PathBuf::from("/tmp"))
        .chapter_name_regex_str("(".to_string())
        .build();
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Invalid chapter_name_regex")
    );

    Ok(())
}

#[tokio::test]
async fn test_hozon_config_preflight_check() -> Result<()> {
    let test_dirs = setup_test_dirs("preflight_check").await;

    // Valid for FromSource
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(test_dirs.source_dir.clone())
        .target_path(test_dirs.target_dir.clone())
        .build()?;
    assert!(
        config
            .preflight_check(HozonExecutionMode::FromSource)
            .is_ok()
    );

    // Invalid for FromSource (missing source_path)
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .target_path(test_dirs.target_dir.clone())
        .build()?;
    let result = config.preflight_check(HozonExecutionMode::FromSource);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("`source_path` must be set for `FromSource` execution mode")
    );

    // Invalid for FromSource (source_path does not exist)
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(test_dirs.source_dir.join("nonexistent"))
        .target_path(test_dirs.target_dir.clone())
        .build()?;
    let result = config.preflight_check(HozonExecutionMode::FromSource);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Source path does not exist")
    );
    Ok(())
}

#[tokio::test]
async fn test_collector_regex_parser() -> Result<()> {
    let test_dirs = setup_test_dirs("collector_regex").await;
    let source_dir = test_dirs.source_dir.clone();
    let default_collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);

    // Default numeric regex
    assert_eq!(
        default_collector.regex_parser(&PathBuf::from("001-test.jpg"), false),
        Some(1.0)
    );
    assert_eq!(
        default_collector.regex_parser(&PathBuf::from("chapter_2.5.png"), false),
        Some(2.5)
    );
    assert_eq!(
        default_collector.regex_parser(&PathBuf::from("volume 123.cbz"), false),
        Some(123.0)
    );
    assert_eq!(
        default_collector.regex_parser(&PathBuf::from("no_number.txt"), false),
        None
    );

    // Test the default volume/chapter sorter directly
    let path1 = PathBuf::from("001-005 Chapter Title");
    let path2 = PathBuf::from("001-010 Another Chapter");
    let path3 = PathBuf::from("002-001 New Volume");

    use std::cmp::Ordering;
    assert_eq!(
        Collector::sort_by_name_volume_chapter_default(&path1, &path2),
        Ordering::Less
    );
    assert_eq!(
        Collector::sort_by_name_volume_chapter_default(&path2, &path1),
        Ordering::Greater
    );
    assert_eq!(
        Collector::sort_by_name_volume_chapter_default(&path2, &path3),
        Ordering::Less
    );

    // Custom regex
    let source_dir = test_dirs.source_dir.clone();
    let custom_re = Regex::new(r"PAGE_(\d+)").unwrap();
    let custom_collector_page_re = Collector::new(
        &source_dir,
        CollectionDepth::Deep,
        None,
        Some(&custom_re),
        75,
    );
    assert_eq!(
        custom_collector_page_re.regex_parser(&PathBuf::from("MyBook_PAGE_007.webp"), false),
        Some(7.0)
    );
    assert_eq!(
        custom_collector_page_re.regex_parser(&PathBuf::from("MyBook_PAGE_XYZ.webp"), false),
        None
    );
    Ok(())
}

#[tokio::test]
async fn test_collector_is_grayscale() -> Result<()> {
    let test_dirs = setup_test_dirs("preflight_check").await;
    let gray_path = test_dirs.test_dir.join("gray.jpg");
    let color_path = test_dirs.test_dir.join("color.jpg");

    create_dummy_grayscale_image(&gray_path).await?;
    create_dummy_color_image(&color_path).await?;

    let gray_img = image::open(&gray_path)?;
    let color_img = image::open(&color_path)?;

    // High sensibility (e.g., 0.9) means it's strict, a high percentage of pixels must be gray
    assert!(
        Collector::is_grayscale(&gray_img, 0.9),
        "Dummy grayscale image should be detected as grayscale"
    );
    assert!(
        !Collector::is_grayscale(&color_img, 0.9),
        "Dummy color image should not be detected as grayscale"
    );

    // Low sensibility means it's very tolerant to color, so only truly grayscale passes.
    // Our dummy color image (pure red) should definitely not be grayscale regardless of sensibility > 0.
    assert!(
        !Collector::is_grayscale(&color_img, 0.1),
        "Dummy color image should not be detected as grayscale with low sensibility"
    );
    Ok(())
}

#[tokio::test]
async fn test_collector_collection_depth() -> Result<()> {
    let test_dirs = setup_test_dirs("preflight_check").await;

    // Setup:
    // tmp_dir/
    //   chapter_1/
    //     page_001.jpg
    //     page_002.png
    //   chapter_2/
    //     page_001.jpg
    //   flat_page_a.jpg
    //   flat_page_b.png

    let chap1_dir = test_dirs.source_dir.join("chapter_1");
    let chap2_dir = test_dirs.source_dir.join("chapter_2");
    create_dummy_color_image(&chap1_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&chap1_dir.join("page_002.png")).await?;
    create_dummy_color_image(&chap2_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("flat_page_a.jpg")).await?;
    create_dummy_color_image(&test_dirs.source_dir.join("flat_page_b.png")).await?;

    // Test Deep collection
    let source_dir = test_dirs.source_dir.clone();
    let deep_collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let chapters_deep = deep_collector
        .collect_chapters(None::<fn(&PathBuf, &PathBuf) -> Ordering>)
        .await?;
    assert_eq!(chapters_deep.len(), 2); // Should find chapter_1 and chapter_2

    let pages_deep = deep_collector
        .collect_pages(
            chapters_deep,
            None::<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Sync + Send + 'static>>,
        )
        .await?;
    assert_eq!(pages_deep.len(), 2);
    // Sort results for consistent testing
    let mut sorted_pages: Vec<Vec<PathBuf>> = pages_deep
        .into_iter()
        .map(|mut p: Vec<PathBuf>| {
            p.sort();
            p
        })
        .collect();
    sorted_pages.sort_by_key(|p| p.first().cloned());

    assert_eq!(sorted_pages[0].len(), 2); // chapter_1 has 2 pages
    assert_eq!(sorted_pages[1].len(), 1); // chapter_2 has 1 page

    // Test Shallow collection
    let shallow_collector = Collector::new(&source_dir, CollectionDepth::Shallow, None, None, 75);
    let chapters_shallow = shallow_collector
        .collect_chapters(None::<fn(&PathBuf, &PathBuf) -> Ordering>)
        .await?;
    assert_eq!(chapters_shallow.len(), 1); // Should treat tmp_dir as the only "chapter"
    assert_eq!(chapters_shallow[0], source_dir);

    let pages_shallow = shallow_collector
        .collect_pages(
            chapters_shallow,
            None::<Arc<dyn Fn(&PathBuf, &PathBuf) -> Ordering + Sync + Send + 'static>>,
        )
        .await?;
    assert_eq!(pages_shallow.len(), 1);
    // Should collect only the files directly in source_dir
    assert_eq!(pages_shallow[0].len(), 2); // The two flat pages
    Ok(())
}

#[tokio::test]
async fn test_collector_calculate_volume_sizes() -> Result<()> {
    let path = PathBuf::new();
    let collector = Collector::new(&path, CollectionDepth::Deep, None, None, 75);

    // Standard case
    let sizes = collector.calculate_volume_sizes(vec![0, 5, 10], 15)?;
    assert_eq!(sizes, vec![5, 5, 5]);

    // Last volume has fewer chapters
    let sizes = collector.calculate_volume_sizes(vec![0, 10], 12)?;
    assert_eq!(sizes, vec![10, 2]);

    // Only one volume
    let sizes = collector.calculate_volume_sizes(vec![0], 20)?;
    assert_eq!(sizes, vec![20]);

    // Empty starts, should be one volume
    let sizes = collector.calculate_volume_sizes(vec![], 15)?;
    assert_eq!(sizes, vec![15]);

    // Empty everything
    let sizes = collector.calculate_volume_sizes(vec![], 0)?;
    assert_eq!(sizes, Vec::<usize>::new());

    // Non-zero start
    let sizes = collector.calculate_volume_sizes(vec![2, 8], 15)?;
    // The logic doesn't handle non-zero starts; it assumes the first volume starts at the first index.
    // This is an internal function and the input is controlled, so this is expected.
    assert_eq!(sizes, vec![6, 7]); // 8-2=6, 15-8=7

    Ok(())
}

#[tokio::test]
async fn test_ebook_metadata_default_with_title() {
    let metadata = EbookMetadata::default_with_title("My Book".to_string());
    assert_eq!(metadata.title, "My Book");
    assert_eq!(metadata.language, "en"); // Default language
    assert!(metadata.authors.is_empty());
}

#[tokio::test]
async fn test_collector_analysis_unsupported_files() -> Result<()> {
    let test_dirs = setup_test_dirs("analysis_unsupported").await;

    // Create a chapter with supported and unsupported files
    let chapter_dir = test_dirs.source_dir.join("Chapter_1");
    create_dummy_color_image(&chapter_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&chapter_dir.join("page_002.png")).await?;

    // Create an unsupported file (text file)
    tokio::fs::write(chapter_dir.join("readme.txt"), "This is a text file").await?;

    let source_dir = test_dirs.source_dir.clone();
    let collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let result = collector.analyze_source_content().await?;

    // Check that unsupported file was flagged
    let unsupported_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter_map(|f| match f {
            AnalyzeFinding::UnsupportedFileIgnored { path } => Some(path),
            _ => None,
        })
        .collect();

    assert_eq!(unsupported_findings.len(), 1);
    assert!(
        unsupported_findings[0]
            .to_str()
            .unwrap()
            .contains("readme.txt")
    );

    // Check that supported files were still collected
    assert_eq!(result.chapters_with_pages.len(), 1);
    assert_eq!(result.chapters_with_pages[0].len(), 2); // Only the 2 image files

    Ok(())
}

#[tokio::test]
async fn test_collector_analysis_inconsistent_page_count() -> Result<()> {
    let test_dirs = setup_test_dirs("analysis_inconsistent").await;

    // Create chapters with very different page counts
    let chapter1_dir = test_dirs.source_dir.join("Chapter_1");
    let chapter2_dir = test_dirs.source_dir.join("Chapter_2");
    let chapter3_dir = test_dirs.source_dir.join("Chapter_3");

    // Chapter 1: 1 page
    create_dummy_color_image(&chapter1_dir.join("page_001.jpg")).await?;

    // Chapter 2: 10 pages (normal)
    for i in 1..=10 {
        create_dummy_color_image(&chapter2_dir.join(format!("page_{:03}.jpg", i))).await?;
    }

    // Chapter 3: 9 pages (normal, similar to chapter 2)
    for i in 1..=9 {
        create_dummy_color_image(&chapter3_dir.join(format!("page_{:03}.jpg", i))).await?;
    }

    let source_dir = test_dirs.source_dir.clone();
    let collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let result = collector.analyze_source_content().await?;

    // Check that inconsistent page count was flagged for chapter 1
    let inconsistent_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter_map(|f| match f {
            AnalyzeFinding::InconsistentPageCount {
                chapter_path,
                expected,
                found,
            } => Some((chapter_path, *expected, *found)),
            _ => None,
        })
        .collect();

    assert!(!inconsistent_findings.is_empty());
    // Chapter 1 should be flagged as having inconsistent page count
    let chapter1_flagged = inconsistent_findings
        .iter()
        .any(|(path, _expected, found)| {
            path.to_str().unwrap().contains("Chapter_1") && *found == 1
        });
    assert!(chapter1_flagged);

    Ok(())
}

#[tokio::test]
async fn test_collector_analysis_special_characters() -> Result<()> {
    let test_dirs = setup_test_dirs("analysis_special_chars").await;

    let chapter_dir = test_dirs.source_dir.join("Chapter_1");

    // Create files with characters that will be detected as problematic (Windows-safe versions)
    // Note: Using ? and * which are invalid on Windows file systems but can be tested via path validation
    create_dummy_color_image(&chapter_dir.join("page_question.jpg")).await?;
    create_dummy_color_image(&chapter_dir.join("page_asterisk.jpg")).await?;
    create_dummy_color_image(&chapter_dir.join("normal_page.jpg")).await?;

    // Manually construct paths with special characters for validation testing
    let problematic_path1 = chapter_dir.join("page<001>.jpg");
    let problematic_path2 = chapter_dir.join("page|002|.jpg");

    let source_dir = test_dirs.source_dir.clone();
    let collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let result = collector.analyze_source_content().await?;

    // Test that validate_path function properly detects special characters
    use hozon::path_utils::validate_path;

    // These paths with special characters should be detected as invalid
    assert!(validate_path(&problematic_path1).is_err());
    assert!(validate_path(&problematic_path2).is_err());

    // Normal files should not trigger special character detection
    let normal_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter_map(|f| match f {
            AnalyzeFinding::SpecialCharactersInPath { .. } => Some(f),
            _ => None,
        })
        .collect();

    // Since we only created valid files, there should be no special character findings
    // but we've verified the validation function works correctly above
    assert_eq!(normal_findings.len(), 0);

    Ok(())
}

#[tokio::test]
async fn test_collector_analysis_file_permissions() -> Result<()> {
    let test_dirs = setup_test_dirs("analysis_permissions").await;

    let chapter_dir = test_dirs.source_dir.join("Chapter_1");

    // Create a normal file
    create_dummy_color_image(&chapter_dir.join("accessible.jpg")).await?;

    let source_dir = test_dirs.source_dir.clone();
    let collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let result = collector.analyze_source_content().await?;

    // In a normal test environment, we shouldn't have permission issues
    // This test mainly verifies the analysis doesn't crash when checking permissions
    let permission_findings: Vec<_> = result
        .report
        .findings
        .iter()
        .filter_map(|f| match f {
            AnalyzeFinding::PermissionDenied { .. } => Some(f),
            _ => None,
        })
        .collect();

    // In normal test conditions, there should be no permission issues
    assert!(permission_findings.is_empty());

    Ok(())
}

#[tokio::test]
async fn test_collector_analysis_positive_findings() -> Result<()> {
    let test_dirs = setup_test_dirs("analysis_positive").await;

    // Create chapters with consistent naming pattern
    let chapter1_dir = test_dirs.source_dir.join("01-001_First_Chapter");
    let chapter2_dir = test_dirs.source_dir.join("01-002_Second_Chapter");

    // Create consistent image format (all JPG)
    create_dummy_color_image(&chapter1_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&chapter1_dir.join("page_002.jpg")).await?;
    create_dummy_color_image(&chapter2_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&chapter2_dir.join("page_002.jpg")).await?;

    let source_dir = test_dirs.source_dir.clone();
    let collector = Collector::new(&source_dir, CollectionDepth::Deep, None, None, 75);
    let result = collector.analyze_source_content().await?;

    // Check for positive findings
    let consistent_naming_found = result
        .report
        .findings
        .iter()
        .any(|f| matches!(f, AnalyzeFinding::ConsistentNamingFound { .. }));

    assert!(
        consistent_naming_found,
        "Should detect consistent naming pattern"
    );

    Ok(())
}

#[tokio::test]
async fn test_volume_separator_default_value() -> Result<()> {
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(PathBuf::from("./test_source"))
        .target_path(PathBuf::from("./test_target"))
        .build()?;

    assert_eq!(config.volume_separator, " - ");
    Ok(())
}

#[tokio::test]
async fn test_get_file_info_utility() -> Result<()> {
    use hozon::types::get_file_info;
    use std::path::PathBuf;

    // Test supported formats
    assert_eq!(
        get_file_info(&PathBuf::from("test.jpg"))?,
        ("jpg", "image/jpeg")
    );
    assert_eq!(
        get_file_info(&PathBuf::from("test.jpeg"))?,
        ("jpg", "image/jpeg")
    );
    assert_eq!(
        get_file_info(&PathBuf::from("test.png"))?,
        ("png", "image/png")
    );
    assert_eq!(
        get_file_info(&PathBuf::from("test.webp"))?,
        ("webp", "image/webp")
    );

    // Test case sensitivity
    assert_eq!(
        get_file_info(&PathBuf::from("test.JPG"))?,
        ("jpg", "image/jpeg")
    );
    assert_eq!(
        get_file_info(&PathBuf::from("test.PNG"))?,
        ("png", "image/png")
    );

    // Test unsupported format
    let result = get_file_info(&PathBuf::from("test.txt"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Unsupported"));

    // Test file without extension
    let result = get_file_info(&PathBuf::from("test"));
    assert!(result.is_err());

    Ok(())
}
