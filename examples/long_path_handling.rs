//! Example demonstrating improved path handling for long paths and special characters.
//!
//! This example shows how Hozon v0.1.3+ handles problematic file paths that were
//! causing issues in earlier versions, particularly:
//! - Long Windows paths (exceeding 260 characters)
//! - Paths with special characters like tildes (~)
//! - Non-ASCII characters (Japanese, etc.)
//! - Proper UTF-8 handling and fallbacks

use hozon::path_utils::*;
use std::path::PathBuf;

fn main() {
    println!("=== Hozon Path Handling Improvements Demo ===\n");

    // Example 1: The problematic path from the issue
    let manga_path = PathBuf::from(
        r"C:\Users\schmlu\Documents\Mangas\Akuyaku Reijou no Naka no Hito ~Danzai sareta Tenseisha no Tame Usotsuki Heroine ni Fukushuu Itashimasu~",
    );

    println!("1. Testing the original problematic path:");
    println!("   Path: {:?}", manga_path);

    // Test path to string conversion (now safe)
    match path_to_string_safe(&manga_path) {
        Ok(path_str) => println!("   ✓ Safe string conversion: {}", path_str),
        Err(e) => println!("   ✗ Safe conversion failed: {}", e),
    }

    // Test lossy conversion (always works)
    let lossy_str = path_to_string_lossy(&manga_path);
    println!("   ✓ Lossy string conversion: {}", lossy_str);

    // Test path validation
    match validate_path(&manga_path) {
        Ok(_) => println!("   ✓ Path validation passed"),
        Err(e) => println!("   ⚠ Path validation warning: {}", e),
    }

    println!();

    // Example 2: Various problematic characters
    println!("2. Testing various character handling:");

    let test_paths = vec![
        ("Tilde character", "manga~folder/chapter.jpg"),
        ("Japanese characters", "マンガ/第1章.jpg"),
        ("Russian characters", "манга/глава1.jpg"),
        ("Spaces and dashes", "manga folder/chapter-01.jpg"),
        ("Underscores and numbers", "manga_001/page_001.jpg"),
    ];

    for (description, path_str) in test_paths {
        let path = PathBuf::from(path_str);
        println!("   {}: {}", description, path_to_string_lossy(&path));

        // Test file name extraction
        let file_name = get_file_name_lossy(&path);
        println!("     File name: {}", file_name);

        // Test validation
        match validate_path(&path) {
            Ok(_) => println!("     ✓ Valid"),
            Err(_) => println!("     ⚠ Has validation warnings"),
        }
        println!();
    }

    // Example 3: Hidden file detection
    println!("3. Testing hidden file detection:");
    let hidden_files = vec![".gitignore", ".hidden", "normal_file.jpg", "folder/.secret"];

    for file_path in hidden_files {
        let path = PathBuf::from(file_path);
        let is_hidden = is_hidden_file(&path);
        println!(
            "   {}: {}",
            file_path,
            if is_hidden { "Hidden" } else { "Visible" }
        );
    }

    println!();

    // Example 4: Number extraction for sorting
    println!("4. Testing improved number extraction for sorting:");

    use regex::Regex;
    let number_regex = Regex::new(r"(\d+)").unwrap();

    let manga_files = vec![
        "chapter_001.jpg",
        "chapter_010.jpg",
        "chapter_002.jpg",
        "volume_01_chapter_005.jpg",
        "マンガ_第123話.jpg", // Japanese with numbers
    ];

    println!("   Original order:");
    for file in &manga_files {
        println!("     {}", file);
    }

    let mut sorted_paths: Vec<PathBuf> = manga_files.iter().map(|s| PathBuf::from(s)).collect();
    sorted_paths.sort_by(|a, b| compare_paths_by_number_safe(a, b, &number_regex));

    println!("   Sorted by number:");
    for path in sorted_paths {
        let file_name = get_file_name_lossy(&path);
        let number = extract_number_from_filename_safe(&path, &number_regex);
        println!("     {} (extracted number: {:?})", file_name, number);
    }

    println!();

    // Example 5: Long path handling on Windows
    if cfg!(windows) {
        println!("5. Testing Windows long path handling:");

        // Create a path that would exceed the traditional 260-character limit
        let mut long_path = PathBuf::from("C:\\Users\\example\\Documents\\Very Long Path Names");
        for i in 0..10 {
            long_path.push(format!(
                "Very Long Directory Name That Makes The Path Exceed Limits {}",
                i
            ));
        }
        long_path.push("final_manga_chapter_with_very_long_name.jpg");

        println!(
            "   Path length: {} characters",
            path_to_string_lossy(&long_path).len()
        );

        match validate_path(&long_path) {
            Ok(_) => println!("   ✓ Long path validation passed"),
            Err(e) => println!("   ⚠ Long path validation: {}", e),
        }

        // Test normalization (this is where the magic happens for long paths)
        match normalize_path(&long_path) {
            Ok(normalized) => {
                println!("   ✓ Path normalization succeeded");
                let normalized_str = path_to_string_lossy(&normalized);
                if normalized_str.starts_with(r"\\?\") {
                    println!("   ✓ Long path prefix applied: \\\\?\\");
                }
            }
            Err(e) => println!("   ⚠ Path normalization: {}", e),
        }
    } else {
        println!("5. Long path handling is Windows-specific (skipped on this platform)");
    }

    println!("\n=== Summary ===");
    println!("The improved path handling in Hozon v0.1.3+ provides:");
    println!("• Safe UTF-8 conversion with lossy fallbacks");
    println!("• Better validation for special characters");
    println!("• Windows long path support (\\\\?\\ prefix)");
    println!("• Robust number extraction for sorting");
    println!("• Graceful handling of non-ASCII characters");
    println!("\nYour problematic path should now work correctly!");
}
