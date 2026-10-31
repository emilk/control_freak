pub mod config;
pub mod formatters;
mod generated;
pub mod lints;

use anyhow::{Context as _, Result};
use config::Config;
use ignore::WalkBuilder;
use std::fs;
use std::path::Path;

/// Name and summary of a lint or formatter, as shown in `--help`.
pub struct RuleInfo {
    /// Same as the name of the lint's section in `control_freak.toml`.
    pub name: &'static str,

    /// One-line summary.
    pub summary: &'static str,
}

/// Format and lint a single file or all Rust files in a directory.
///
/// When `fix` is true, formatting changes are written back to the files.
/// Returns the number of problems found: lint warnings, plus needed formatting
/// changes unless `fix` is true.
///
/// # Errors
/// If a path can't be read or written, or the configuration is invalid.
pub fn format_path(path: &Path, fix: bool) -> Result<usize> {
    let config = Config::load_for_path(path)?;
    format_path_with_config(&config, path, fix)
}

/// Like [`format_path`], but with an explicit `config`.
///
/// # Errors
/// If a path can't be read or written.
pub fn format_path_with_config(config: &Config, path: &Path, fix: bool) -> Result<usize> {
    let metadata =
        fs::metadata(path).with_context(|| format!("Failed to access path: {}", path.display()))?;

    if metadata.is_file() {
        format_file_with_config(path, fix, config)
    } else if metadata.is_dir() {
        format_directory_with_config(path, fix, config)
    } else {
        anyhow::bail!("Path is neither a file nor a directory: {}", path.display());
    }
}

/// Format and lint all Rust files in a directory, respecting .gitignore.
///
/// When `fix` is true, formatting changes are written back to the files.
/// Returns the number of problems found: lint warnings, plus needed formatting
/// changes unless `fix` is true.
///
/// # Errors
/// If a file can't be read or written, or the configuration is invalid.
pub fn format_directory(dir: &Path, fix: bool) -> Result<usize> {
    let config = Config::load_for_path(dir)?;
    format_directory_with_config(dir, fix, &config)
}

#[expect(clippy::print_stdout, reason = "CLI output")]
fn format_directory_with_config(dir: &Path, fix: bool, config: &Config) -> Result<usize> {
    let walker = WalkBuilder::new(dir).standard_filters(true).build();
    let mut generated_files = generated::GeneratedFiles::default();

    let mut num_files = 0;
    let mut num_generated = 0;
    let mut num_problems = 0;

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
            if config.skip_generated && generated_files.is_generated(path) {
                num_generated += 1;
                continue;
            }
            num_problems += format_file_with_config(path, fix, config)?;
            num_files += 1;
        }
    }

    if num_generated == 0 {
        println!("Processed {num_files} files");
    } else {
        println!("Processed {num_files} files (skipped {num_generated} generated)");
    }

    Ok(num_problems)
}

/// Format and lint a single Rust file.
///
/// When `fix` is true, formatting changes are written back to the files.
/// Returns the number of problems found: lint warnings, plus needed formatting
/// changes unless `fix` is true.
///
/// # Errors
/// If the file can't be read or written, or the configuration is invalid.
pub fn format_file(path: &Path, fix: bool) -> Result<usize> {
    let config = Config::load_for_path(path)?;
    format_file_with_config(path, fix, &config)
}

#[expect(clippy::print_stdout, clippy::print_stderr, reason = "CLI output")]
fn format_file_with_config(path: &Path, fix: bool, config: &Config) -> Result<usize> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read file: {}", path.display()))?;

    let formatted = format_source(&content);

    let mut problems = if fix {
        if formatted != content {
            fs::write(path, &formatted)
                .with_context(|| format!("Failed to write file: {}", path.display()))?;
            println!("  Modified: {}", path.display());
        }
        lint_source_with_config(&formatted, config)
    } else {
        let mut problems = formatting_problems(&content, &formatted);
        problems.extend(lint_source_with_config(&content, config));
        problems
    };
    problems.sort_by_key(|problem| (problem.line, problem.col));

    for problem in &problems {
        let lints::LintWarning { line, col, message } = problem;
        eprintln!("  {}:{line}:{col}: {message}", path.display());
    }

    Ok(problems.len())
}

/// One warning per changed region between `old` and `new`.
fn formatting_problems(old: &str, new: &str) -> Vec<lints::LintWarning> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    changed_regions(&old_lines, &new_lines)
        .into_iter()
        .map(|(start, end)| {
            let lines = if start + 1 == end {
                format!("line {end}")
            } else {
                format!("lines {}-{end}", start + 1)
            };
            lints::LintWarning {
                line: start + 1,
                col: 1,
                message: format!("formatting needed ({lines}); run with `--fix` to apply"),
            }
        })
        .collect()
}

/// Line ranges (0-based, exclusive end) that differ between `old` and `new`.
///
/// Changes separated by a single unchanged line are merged into one region,
/// so that e.g. `} else {` between two changed blocks stays together.
fn changed_regions(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let mut regions: Vec<(usize, usize)> = Vec::new();
    let num_lines = old.len().max(new.len());
    let mut line = 0;
    while line < num_lines {
        if old.get(line) == new.get(line) {
            line += 1;
            continue;
        }
        let start = line;
        while line < num_lines && old.get(line) != new.get(line) {
            line += 1;
        }
        if let Some(last) = regions.last_mut()
            && start <= last.1 + 1
        {
            last.1 = line;
        } else {
            regions.push((start, line));
        }
    }
    regions
}

/// Format Rust source code by applying all formatters.
///
/// Lines protected by a `// NOLINT` comment (on the same line or the line above
/// a contiguous block of changes) are left unchanged.
pub fn format_source(source: &str) -> String {
    // Apply all formatters in sequence
    let mut result = source.to_owned();

    result = formatters::flip_negated_ifs(&result);

    // Add more formatters here as they are implemented

    restore_nolint_lines(source, &result)
}

/// If the original has `// NOLINT` on or above the first line of a changed
/// region, restore the entire region from the original.
fn restore_nolint_lines(original: &str, formatted: &str) -> String {
    let orig_lines: Vec<&str> = original.lines().collect();
    let fmt_lines: Vec<&str> = formatted.lines().collect();

    if orig_lines.len() != fmt_lines.len() {
        return formatted.to_owned();
    }

    let regions = changed_regions(&orig_lines, &fmt_lines);
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
    lint_source_with_config(source, &Config::default())
}

/// Lint Rust source code using `config`, returning all warnings.
pub fn lint_source_with_config(source: &str, config: &Config) -> Vec<lints::LintWarning> {
    let lines: Vec<&str> = source.lines().collect();
    let mut warnings = Vec::new();
    if config.lints.max_mut_bindings.enabled {
        warnings.extend(lints::lint_max_mut_bindings(
            source,
            config.lints.max_mut_bindings.max_bindings,
        ));
    }
    if config.lints.max_struct_fields.enabled {
        warnings.extend(lints::lint_max_struct_fields(
            source,
            config.lints.max_struct_fields.max_fields,
        ));
    }
    if config.lints.max_visible_bindings.enabled {
        warnings.extend(lints::lint_max_visible_bindings(
            source,
            config.lints.max_visible_bindings.max_bindings,
        ));
    }
    if config.lints.negated_bool_names.enabled {
        warnings.extend(lints::lint_negated_bool_names(source));
    }
    if config.lints.use_destructuring.enabled {
        warnings.extend(lints::lint_use_destructuring_with_min_fields(
            source,
            config.lints.use_destructuring.min_fields,
        ));
    }
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
    fn lint_configuration_can_disable_individual_lints() {
        let source = unindent(
            "
            struct Config { is_disabled: bool, a: i32, b: i32 }
            impl Config { fn sum(&self) -> i32 { self.a + self.b + self.is_disabled as i32 } }
            ",
        );
        let mut config = Config::default();
        config.lints.max_mut_bindings.enabled = false;
        config.lints.max_struct_fields.enabled = false;
        config.lints.max_visible_bindings.enabled = false;
        config.lints.negated_bool_names.enabled = false;
        config.lints.use_destructuring.enabled = false;

        assert!(lint_source_with_config(&source, &config).is_empty());
    }

    #[test]
    fn formatting_problems_reports_each_changed_range() {
        let lines = |problems: Vec<lints::LintWarning>| {
            problems
                .into_iter()
                .map(|problem| problem.to_string())
                .collect::<Vec<_>>()
        };
        assert!(formatting_problems("a\nb\n", "a\nb\n").is_empty());
        assert_eq!(
            lines(formatting_problems("a\nb\nc\nd\ne\n", "x\nb\nc\ny\nz\nw\n")),
            vec![
                "1:1: formatting needed (line 1); run with `--fix` to apply",
                "4:1: formatting needed (lines 4-6); run with `--fix` to apply",
            ]
        );
        assert_eq!(
            lines(formatting_problems("a\nb\nc\n", "x\nb\ny\n")),
            vec!["1:1: formatting needed (lines 1-3); run with `--fix` to apply"]
        );
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
