pub mod formatters;
pub mod lints;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs;
use std::path::Path;

/// Format and lint a single file or all Rust files in a directory.
///
/// When `dry_run` is true, no files are written.
/// Returns the number of lint warnings found.
pub fn format_path(path: &Path, dry_run: bool) -> Result<usize> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to access path: {}", path.display()))?;

    if metadata.is_file() {
        format_file(path, dry_run)
    } else if metadata.is_dir() {
        format_directory(path, dry_run)
    } else {
        anyhow::bail!("Path is neither a file nor a directory: {}", path.display());
    }
}

/// Format and lint all Rust files in a directory, respecting .gitignore.
///
/// When `dry_run` is true, no files are written.
/// Returns the number of lint warnings found.
pub fn format_directory(dir: &Path, dry_run: bool) -> Result<usize> {
    let walker = WalkBuilder::new(dir).standard_filters(true).build();

    let mut num_files = 0;
    let mut num_warnings = 0;

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            num_warnings += format_file(path, dry_run)?;
            num_files += 1;
        }
    }

    println!("Processed {num_files} files");

    Ok(num_warnings)
}

/// Format and lint a single Rust file.
///
/// When `dry_run` is true, no files are written.
/// Returns the number of lint warnings found.
pub fn format_file(path: &Path, dry_run: bool) -> Result<usize> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let formatted = format_source(&content);

    if formatted != content {
        if dry_run {
            println!("  Would modify: {}", path.display());
        } else {
            fs::write(path, &formatted)
                .with_context(|| format!("Failed to write file: {}", path.display()))?;
            println!("  Modified: {}", path.display());
        }
    }

    let warnings = lint_source(&formatted);
    for w in &warnings {
        eprintln!("  {}:{}:{}: {}", path.display(), w.line, w.col, w.message);
    }

    Ok(warnings.len())
}

/// Format Rust source code by applying all formatters
pub fn format_source(source: &str) -> String {
    // Apply all formatters in sequence
    let mut result = source.to_string();

    result = formatters::flip_negated_ifs(&result);

    // Add more formatters here as they are implemented

    result
}

/// Lint Rust source code, returning all warnings
pub fn lint_source(source: &str) -> Vec<lints::LintWarning> {
    let mut warnings = Vec::new();
    warnings.extend(lints::lint_negated_bool_names(source));
    warnings
}
