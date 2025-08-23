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
    // Missing target_path - should fail in the builder's generated code
    let result = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(PathBuf::from("/tmp"))
        .build();
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("target_path"));

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
    let (_test_path, source_dir, target_dir) = setup_test_dirs("preflight_check").await;

    // Valid for FromSource
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .source_path(source_dir.clone())
        .target_path(target_dir.clone())
        .build()?;
    assert!(
        config
            .preflight_check(HozonExecutionMode::FromSource)
            .is_ok()
    );

    // Invalid for FromSource (missing source_path)
    let config = HozonConfig::builder()
        .metadata(EbookMetadata::default_with_title("Test".to_string()))
        .target_path(target_dir.clone())
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
        .source_path(source_dir.join("nonexistent"))
        .target_path(target_dir.clone())
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
    let (_test_path, source_dir, _target_dir) = setup_test_dirs("collector_regex").await;
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
    let (test_path, _source_dir, _target_dir) = setup_test_dirs("preflight_check").await;
    let gray_path = test_path.join("gray.jpg");
    let color_path = test_path.join("color.jpg");

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
    let (_test_path, source_dir, _target_dir) = setup_test_dirs("preflight_check").await;

    // Setup:
    // tmp_dir/
    //   chapter_1/
    //     page_001.jpg
    //     page_002.png
    //   chapter_2/
    //     page_001.jpg
    //   flat_page_a.jpg
    //   flat_page_b.png

    let chap1_dir = source_dir.join("chapter_1");
    let chap2_dir = source_dir.join("chapter_2");
    create_dummy_color_image(&chap1_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&chap1_dir.join("page_002.png")).await?;
    create_dummy_color_image(&chap2_dir.join("page_001.jpg")).await?;
    create_dummy_color_image(&source_dir.join("flat_page_a.jpg")).await?;
    create_dummy_color_image(&source_dir.join("flat_page_b.png")).await?;

    // Test Deep collection
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
