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
            let changed_lines = changed_line_numbers(&content, &formatted);
            println!(
                "  Would modify: {} (line{}{})",
                path.display(),
                if changed_lines.len() == 1 { " " } else { "s " },
                format_line_numbers(&changed_lines),
            );
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

fn changed_line_numbers(old: &str, new: &str) -> Vec<usize> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut changed = Vec::new();

    let max_len = old_lines.len().max(new_lines.len());
    for i in 0..max_len {
        let old_line = old_lines.get(i).copied();
        let new_line = new_lines.get(i).copied();
        if old_line != new_line {
            changed.push(i + 1);
        }
    }

    changed
}

fn format_line_numbers(lines: &[usize]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    // Group consecutive line numbers into ranges
    let mut parts = Vec::new();
    let mut start = lines[0];
    let mut end = lines[0];

    for &line in &lines[1..] {
        if line == end + 1 {
            end = line;
        } else {
            parts.push(if start == end {
                format!("{start}")
            } else {
                format!("{start}-{end}")
            });
            start = line;
            end = line;
        }
    }
    parts.push(if start == end {
        format!("{start}")
    } else {
        format!("{start}-{end}")
    });

    parts.join(", ")
}

/// Format Rust source code by applying all formatters.
///
/// Lines protected by a `// NOLINT` comment (on the same line or the line above
/// a contiguous block of changes) are left unchanged.
pub fn format_source(source: &str) -> String {
    // Apply all formatters in sequence
    let mut result = source.to_string();

    result = formatters::flip_negated_ifs(&result);

    // Add more formatters here as they are implemented

    restore_nolint_lines(source, &result)
}

/// Compare original and formatted line-by-line. Merge nearby change groups
/// into regions (a single `} else {` line between two changed blocks is part
/// of the same construct). If the original has `// NOLINT` on or above the
/// first line of a region, restore the entire region from the original.
fn restore_nolint_lines(original: &str, formatted: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();

    if orig_lines.len() != fmt_lines.len() {
        return formatted.to_string();
    }

    // Find contiguous groups of changed lines (0-indexed, exclusive end)
    let mut groups: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < orig_lines.len() {
        if orig_lines[i] != fmt_lines[i] {
            let start = i;
            while i < orig_lines.len() && orig_lines[i] != fmt_lines[i] {
                i += 1;
            }
            groups.push((start, i));
        } else {
            i += 1;
        }
    }

    // Merge groups separated by at most 1 unchanged line into regions,
    // so that e.g. `} else {` between two changed blocks stays together.
    let mut regions: Vec<(usize, usize)> = Vec::new();
    for group in groups {
        if let Some(last) = regions.last_mut()
            && group.0 <= last.1 + 2
        {
            last.1 = group.1;
            continue;
        }
        regions.push(group);
    }

    let mut result_lines = fmt_lines;
    for (start, end) in regions {
        if is_nolint(&orig_lines, start + 1) {
            result_lines[start..end].copy_from_slice(&orig_lines[start..end]);
        }
    }

    let mut result = result_lines.join("\n");
    if original.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Lint Rust source code, returning all warnings.
///
/// Warnings on lines with a `// NOLINT` comment (or the line above) are suppressed.
pub fn lint_source(source: &str) -> Vec<lints::LintWarning> {
    let lines: Vec<&str> = source.lines().collect();
    let mut warnings = Vec::new();
    warnings.extend(lints::lint_negated_bool_names(source));
    warnings.retain(|w| !is_nolint(&lines, w.line));
    warnings
}

/// Check if a given 1-based line (or the line above it) has a `// NOLINT` comment.
pub(crate) fn is_nolint(lines: &[&str], line: usize) -> bool {
    let idx = line.saturating_sub(1);
    if lines.get(idx).is_some_and(|l| l.contains("// NOLINT")) {
        return true;
    }
    idx > 0 && lines.get(idx - 1).is_some_and(|l| l.contains("// NOLINT"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    #[test]
    fn test_nolint_suppresses_lint_same_line() {
        let source = unindent(
            "
            struct Config {
                is_disabled: bool, // NOLINT
            }
            ",
        );
        assert!(lint_source(&source).is_empty());
    }

    #[test]
    fn test_nolint_suppresses_lint_line_above() {
        let source = unindent(
            "
            struct Config {
                // NOLINT
                is_disabled: bool,
            }
            ",
        );
        assert!(lint_source(&source).is_empty());
    }

    #[test]
    fn test_nolint_does_not_suppress_distant_line() {
        let source = unindent(
            "
            // NOLINT
            struct Config {
                is_disabled: bool,
            }
            ",
        );
        assert_eq!(lint_source(&source).len(), 1);
    }

    #[test]
    fn test_without_nolint_lint_fires() {
        let source = unindent(
            "
            struct Config {
                is_disabled: bool,
            }
            ",
        );
        assert_eq!(lint_source(&source).len(), 1);
    }

    #[test]
    fn test_nolint_suppresses_formatter_same_line() {
        let input = unindent(
            "
            fn example(value: bool) {
                if !value { // NOLINT
                    println!(\"false branch\");
                } else {
                    println!(\"true branch\");
                }
            }
            ",
        );

        let output = format_source(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_nolint_suppresses_formatter_line_above() {
        let input = unindent(
            "
            fn example(value: bool) {
                // NOLINT
                if !value {
                    println!(\"false branch\");
                } else {
                    println!(\"true branch\");
                }
            }
            ",
        );

        let output = format_source(&input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_nolint_does_not_suppress_distant_formatter() {
        let input = unindent(
            "
            // NOLINT
            fn example(value: bool) {
                if !value {
                    println!(\"false branch\");
                } else {
                    println!(\"true branch\");
                }
            }
            ",
        );

        let output = format_source(&input);
        assert_ne!(output, input);
    }
}
