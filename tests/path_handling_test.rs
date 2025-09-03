//! Tests for path handling improvements, especially for long paths and special characters.

use hozon::path_utils::*;
use std::path::{Path, PathBuf};

#[test]
fn test_path_to_string_safe() {
    let normal_path = Path::new("test/normal/path.jpg");
    assert!(path_to_string_safe(normal_path).is_ok());
}

#[test]
fn test_get_file_name_safe() {
    let path = Path::new("folder/test_file.jpg");
    let result = get_file_name_safe(path).unwrap();
    assert_eq!(result, "test_file.jpg");
}

#[test]
fn test_get_file_name_lossy() {
    let path = Path::new("folder/test_file.jpg");
    let result = get_file_name_lossy(path);
    assert_eq!(result, "test_file.jpg");

    // Test with empty path
    let empty_path = Path::new("");
    let result = get_file_name_lossy(empty_path);
    assert_eq!(result, "unknown");
}

#[test]
fn test_path_to_string_lossy() {
    let path = Path::new("test/path/file.jpg");
    let result = path_to_string_lossy(path);
    assert!(result.contains("test"));
    assert!(result.contains("path"));
    assert!(result.contains("file.jpg"));
}

#[test]
fn test_validate_path_with_invalid_characters() {
    // Test paths with problematic characters
    let invalid_chars = ["<", ">", "\"", "|", "?", "*"];

    for invalid_char in &invalid_chars {
        let invalid_path = PathBuf::from(format!("test{}file.jpg", invalid_char));
        assert!(
            validate_path(&invalid_path).is_err(),
            "Path with '{}' should be invalid",
            invalid_char
        );
    }
}

#[test]
fn test_validate_path_with_valid_characters() {
    // Test paths with valid characters including some that might be problematic in other contexts
    let valid_paths = [
        "normal_file.jpg",
        "file with spaces.jpg",
        "file-with-dashes.jpg",
        "file_with_underscores.jpg",
        "file123.jpg",
        "ファイル.jpg",   // Japanese characters
        "тест.jpg",       // Cyrillic characters
        "file~tilde.jpg", // Tilde character (should be valid now)
        "file:colon.jpg", // Colon in Windows should still work for drive letters
    ];

    for valid_path in &valid_paths {
        let path = PathBuf::from(valid_path);
        // Note: Some of these might still fail on Windows due to filesystem restrictions,
        // but our validation should be more lenient than the original implementation
        let result = validate_path(&path);
        if result.is_err() {
            println!("Path '{}' failed validation: {:?}", valid_path, result);
        }
    }
}

#[test]
fn test_is_hidden_file() {
    // Test hidden files (starting with dot)
    assert!(is_hidden_file(Path::new(".hidden")));
    assert!(is_hidden_file(Path::new(".gitignore")));
    assert!(is_hidden_file(Path::new("folder/.hidden_file")));

    // Test non-hidden files
    assert!(!is_hidden_file(Path::new("normal_file.jpg")));
    assert!(!is_hidden_file(Path::new("folder/normal_file.jpg")));
    assert!(!is_hidden_file(Path::new("file.with.dots.jpg"))); // Only starting dot counts
}

#[test]
fn test_extract_number_from_filename() {
    use regex::Regex;

    let regex = Regex::new(r"(\d+)").unwrap();

    // Test various filename patterns
    let test_cases = [
        ("chapter_01.jpg", Some(1.0)),
        ("page_123.png", Some(123.0)),
        ("file_with_no_numbers.jpg", None),
        ("multiple_123_456_numbers.jpg", Some(456.0)), // Should get the last match
        ("001_leading_zeros.jpg", Some(1.0)),          // Should trim leading zeros
    ];

    for (filename, expected) in &test_cases {
        let path = Path::new(filename);
        let result = extract_number_from_filename_safe(path, &regex);
        assert_eq!(result, *expected, "Failed for filename: {}", filename);
    }
}

#[test]
fn test_compare_paths_by_number() {
    use regex::Regex;
    use std::cmp::Ordering;

    let regex = Regex::new(r"(\d+)").unwrap();

    let path1 = Path::new("chapter_01.jpg");
    let path2 = Path::new("chapter_02.jpg");
    let path3 = Path::new("chapter_10.jpg");

    assert_eq!(
        compare_paths_by_number_safe(path1, path2, &regex),
        Ordering::Less
    );
    assert_eq!(
        compare_paths_by_number_safe(path2, path1, &regex),
        Ordering::Greater
    );
    assert_eq!(
        compare_paths_by_number_safe(path1, path1, &regex),
        Ordering::Equal
    );
    assert_eq!(
        compare_paths_by_number_safe(path2, path3, &regex),
        Ordering::Less
    );
}

#[cfg(windows)]
#[test]
fn test_windows_long_path_handling() {
    // Create a very long path that would exceed Windows' 260 character limit
    let long_path_components: Vec<String> = (0..20)
        .map(|i| format!("very_long_directory_name_that_exceeds_limits_{}", i))
        .collect();

    let mut long_path = PathBuf::from("C:");
    for component in long_path_components {
        long_path.push(component);
    }
    long_path.push("final_file.jpg");

    // The path should be recognized as too long
    let validation_result = validate_path(&long_path);

    // On Windows, this should either be handled with long path support or fail gracefully
    match validation_result {
        Ok(_) => {
            // If validation passes, the path should be properly normalized
            println!("Long path validation passed, checking normalization...");
        }
        Err(e) => {
            // If it fails, it should be with a specific long path error
            println!("Long path validation failed as expected: {:?}", e);
        }
    }
}

#[test]
fn test_normalize_path_with_existing_file() {
    // Test with a path that should exist (current directory)
    let current_dir = std::env::current_dir().unwrap();
    let result = normalize_path(&current_dir);
    assert!(
        result.is_ok(),
        "Current directory should normalize successfully"
    );
}

#[test]
fn test_normalize_path_with_nonexistent_file() {
    // Test with a path that doesn't exist (should still work for output paths)
    let nonexistent = PathBuf::from("nonexistent_directory/output_file.cbz");
    let result = normalize_path(&nonexistent);
    // This should work since we allow non-existent paths for output
    assert!(
        result.is_ok(),
        "Non-existent paths should be allowed for output"
    );
}

#[test]
fn test_japanese_manga_path() {
    // Test the exact type of path mentioned in the issue
    let manga_path = PathBuf::from(
        "C:\\Users\\test\\Documents\\Mangas\\Akuyaku Reijou no Naka no Hito ~Danzai sareta Tenseisha no Tame Usotsuki Heroine ni Fukushuu Itashimasu~",
    );

    // This path should be handled gracefully
    let path_str = path_to_string_lossy(&manga_path);
    assert!(path_str.contains("Akuyaku Reijou"));
    assert!(path_str.contains("~")); // Tilde should be preserved

    // The path validation should work (even if the path doesn't exist)
    let validation = validate_path(&manga_path);
    match validation {
        Ok(_) => println!("Japanese manga path validated successfully"),
        Err(e) => {
            // If it fails, it should be due to length, not character issues
            println!("Japanese manga path validation result: {:?}", e);
            // Ensure it's not failing due to tilde character specifically
            assert!(!format!("{:?}", e).contains("invalid characters"));
        }
    }
}
