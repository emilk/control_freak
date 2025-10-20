pub mod formatters;

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use std::fs;
use std::path::Path;

/// Format a single file or all Rust files in a directory
pub fn format_path(path: &Path) -> Result<()> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to access path: {}", path.display()))?;

    if metadata.is_file() {
        format_file(path)?;
    } else if metadata.is_dir() {
        format_directory(path)?;
    } else {
        anyhow::bail!("Path is neither a file nor a directory: {}", path.display());
    }

    Ok(())
}

/// Format all Rust files in a directory, respecting .gitignore
pub fn format_directory(dir: &Path) -> Result<()> {
    let walker = WalkBuilder::new(dir).standard_filters(true).build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            println!("Processing: {}", path.display());
            format_file(path)?;
        }
    }

    Ok(())
}

/// Format a single Rust file
pub fn format_file(path: &Path) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let formatted = format_source(&content);

    if formatted != content {
        fs::write(path, formatted)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        println!("  Modified: {}", path.display());
    }

    Ok(())
}

/// Format Rust source code by applying all formatters
pub fn format_source(source: &str) -> String {
    // Apply all formatters in sequence
    let mut result = source.to_string();

    result = formatters::flip_negated_ifs(&result);

    // Add more formatters here as they are implemented

    result
}
