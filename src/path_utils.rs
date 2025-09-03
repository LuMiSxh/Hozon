//! Path utilities for safe and robust file path handling.
//!
//! This module provides utilities for handling file paths safely, especially on Windows
//! where long paths and special characters can cause issues. It includes functions for
//! path validation, UTF-8 conversion, and Windows long path support.

use crate::error::{Error, Result};

use std::path::{Path, PathBuf};

/// Maximum path length for Windows without long path support
const WINDOWS_MAX_PATH: usize = 260;

/// Windows long path prefix
const WINDOWS_LONG_PATH_PREFIX: &str = r"\\?\";

/// Safely converts a path to a string, handling UTF-8 conversion errors gracefully.
///
/// # Arguments
///
/// * `path` - The path to convert
///
/// # Returns
///
/// * `Result<String>` - The path as a UTF-8 string, or an error if conversion fails
pub fn path_to_string_safe(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| Error::PathUtf8Error(path.to_path_buf()))
}

/// Safely gets the file name from a path as a string.
///
/// # Arguments
///
/// * `path` - The path to extract the file name from
///
/// # Returns
///
/// * `Result<String>` - The file name as a UTF-8 string, or an error if conversion fails
pub fn get_file_name_safe(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::PathUtf8Error(path.to_path_buf()))
}

/// Gets the file name from a path with fallback to lossy conversion.
///
/// # Arguments
///
/// * `path` - The path to extract the file name from
///
/// # Returns
///
/// * `String` - The file name, using lossy conversion if necessary
pub fn get_file_name_lossy(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Converts a path to a string with fallback to lossy conversion.
///
/// # Arguments
///
/// * `path` - The path to convert
///
/// # Returns
///
/// * `String` - The path as a string, using lossy conversion if necessary
pub fn path_to_string_lossy(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Checks if a path is potentially problematic due to length or special characters.
///
/// # Arguments
///
/// * `path` - The path to validate
///
/// # Returns
///
/// * `Result<()>` - Ok if the path is valid, or an error describing the issue
pub fn validate_path(path: &Path) -> Result<()> {
    let path_str = path_to_string_lossy(path);

    // Check path length (Windows limitation)
    if cfg!(windows)
        && path_str.len() > WINDOWS_MAX_PATH
        && !path_str.starts_with(WINDOWS_LONG_PATH_PREFIX)
    {
        return Err(Error::PathTooLong(path.to_path_buf()));
    }

    // Check for problematic characters that might cause issues in zip files or file systems
    // Skip validation for Windows long path prefix (\\?\) which contains valid question marks
    let path_to_check = if path_str.starts_with(WINDOWS_LONG_PATH_PREFIX) {
        &path_str[WINDOWS_LONG_PATH_PREFIX.len()..]
    } else {
        &path_str
    };

    if path_to_check
        .chars()
        .any(|c| matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(Error::InvalidPath(
            path.to_path_buf(),
            "Path contains invalid characters".to_string(),
        ));
    }

    Ok(())
}

/// Prepares a path for Windows long path support if needed.
///
/// # Arguments
///
/// * `path` - The path to prepare
///
/// # Returns
///
/// * `Result<PathBuf>` - The prepared path, potentially with long path prefix
pub fn prepare_long_path(path: &Path) -> Result<PathBuf> {
    let path_str = path_to_string_safe(path)?;

    // On Windows, add long path prefix if the path is long and doesn't already have it
    if cfg!(windows)
        && path_str.len() > WINDOWS_MAX_PATH
        && !path_str.starts_with(WINDOWS_LONG_PATH_PREFIX)
    {
        // Convert to absolute path first
        let absolute_path = path.canonicalize().map_err(|e| {
            Error::InvalidPath(
                path.to_path_buf(),
                format!("Cannot canonicalize path: {}", e),
            )
        })?;

        let absolute_str = path_to_string_safe(&absolute_path)?;
        let long_path = format!("{}{}", WINDOWS_LONG_PATH_PREFIX, absolute_str);
        Ok(PathBuf::from(long_path))
    } else {
        Ok(path.to_path_buf())
    }
}

/// Extracts numbers from a filename using safe string conversion.
///
/// # Arguments
///
/// * `path` - The path to extract numbers from
/// * `regex` - The regex pattern to use for extraction
///
/// # Returns
///
/// * `Option<f64>` - The extracted number, or None if not found or conversion failed
pub fn extract_number_from_filename_safe(path: &Path, regex: &regex::Regex) -> Option<f64> {
    let file_name = get_file_name_lossy(path);

    regex
        .captures_iter(&file_name)
        .last() // Take the last match, often more specific for versions/numbers
        .and_then(|cap| {
            let capture = cap.get(1).or_else(|| cap.get(0))?.as_str();
            // Attempt to parse as f64, trimming leading zeros if it's an integer part
            if capture.contains('.') {
                capture.parse::<f64>().ok()
            } else {
                capture.trim_start_matches('0').parse::<f64>().ok()
            }
        })
}

/// Safely compares two paths by their numeric content.
///
/// # Arguments
///
/// * `a` - First path to compare
/// * `b` - Second path to compare
/// * `regex` - The regex pattern to use for number extraction
///
/// # Returns
///
/// * `std::cmp::Ordering` - The comparison result
pub fn compare_paths_by_number_safe(
    a: &Path,
    b: &Path,
    regex: &regex::Regex,
) -> std::cmp::Ordering {
    let a_num = extract_number_from_filename_safe(a, regex);
    let b_num = extract_number_from_filename_safe(b, regex);

    a_num
        .partial_cmp(&b_num)
        .unwrap_or(std::cmp::Ordering::Equal)
}

/// Checks if a filename starts with a dot (hidden file) using safe conversion.
///
/// # Arguments
///
/// * `path` - The path to check
///
/// # Returns
///
/// * `bool` - True if the file is hidden (starts with a dot)
pub fn is_hidden_file(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().starts_with('.'))
        .unwrap_or(false)
}

/// Sanitizes a filename by replacing invalid characters with safe alternatives.
///
/// # Arguments
///
/// * `filename` - The filename to sanitize
///
/// # Returns
///
/// * `String` - The sanitized filename
pub fn sanitize_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| match c {
            '<' | '>' | '"' | '|' | '?' | '*' => '-',
            ':' => '-',
            '/' | '\\' => '-',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

/// Normalizes a path for consistent handling across platforms.
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// * `Result<PathBuf>` - The normalized path
pub fn normalize_path(path: &Path) -> Result<PathBuf> {
    // First validate the path
    validate_path(path)?;

    // Try to canonicalize the path to resolve any relative components
    // and get the absolute path
    match path.canonicalize() {
        Ok(canonical) => {
            // If canonicalization succeeds, prepare for long path support if needed
            prepare_long_path(&canonical)
        }
        Err(e) => {
            // If canonicalization fails (e.g., path doesn't exist),
            // just return the original path if it's valid
            if path.exists() {
                Err(Error::InvalidPath(
                    path.to_path_buf(),
                    format!("Cannot access path: {}", e),
                ))
            } else {
                // For non-existent paths (e.g., output paths), just validate and return
                Ok(path.to_path_buf())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::path::Path;

    #[test]
    fn test_path_to_string_lossy() {
        let path = Path::new("test/path");
        let result = path_to_string_lossy(path);
        assert!(result.contains("test"));
        assert!(result.contains("path"));
    }

    #[test]
    fn test_get_file_name_lossy() {
        let path = Path::new("test/file.txt");
        let result = get_file_name_lossy(path);
        assert_eq!(result, "file.txt");
    }

    #[test]
    fn test_is_hidden_file() {
        let hidden = Path::new(".hidden");
        let normal = Path::new("normal.txt");

        assert!(is_hidden_file(hidden));
        assert!(!is_hidden_file(normal));
    }

    #[test]
    fn test_extract_number_from_filename_safe() {
        let regex = Regex::new(r"(\d+)").unwrap();
        let path = Path::new("chapter_123.jpg");

        let result = extract_number_from_filename_safe(path, &regex);
        assert_eq!(result, Some(123.0));
    }

    #[test]
    fn test_validate_path_with_invalid_chars() {
        let path = Path::new("test<invalid>path");
        let result = validate_path(path);
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test<file>"), "test-file-");
        assert_eq!(sanitize_filename("test|file"), "test-file");
        assert_eq!(sanitize_filename("test?file"), "test-file");
        assert_eq!(sanitize_filename("test*file"), "test-file");
        assert_eq!(sanitize_filename("test\"file"), "test-file");
        assert_eq!(sanitize_filename("test:file"), "test-file");
        assert_eq!(sanitize_filename("test/file"), "test-file");
        assert_eq!(sanitize_filename("test\\file"), "test-file");
        assert_eq!(sanitize_filename("normal_file.txt"), "normal_file.txt");
    }
}
