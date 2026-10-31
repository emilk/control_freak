use anyhow::{Context as _, Result};
use clap::Parser;
use control_freak::{
    RuleInfo,
    config::{CONFIG_FILE_NAME, Config},
    format_path_with_config,
    formatters::FORMATTERS,
    lints::LINTS,
};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    about = "An opinionated Rust source code formatter and linter",
    after_help = rules_help()
)]
struct Cli {
    /// File or directory to process
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Apply formatting fixes to the files (by default, only report them)
    #[arg(long)]
    fix: bool,

    /// Use this configuration file instead of searching for `control_freak.toml`
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Write a configuration file with all the defaults (to --config, or into PATH), then exit
    #[arg(long)]
    init: bool,
}

fn rules_help() -> String {
    let section = |title: &str, rules: &[RuleInfo]| {
        let width = rules.iter().map(|rule| rule.name.len()).max().unwrap_or(0);
        let mut text = format!("{title}:");
        for RuleInfo { name, summary } in rules {
            write!(text, "\n  {name:width$}  {summary}").ok();
        }
        text
    };
    let formatters = section("Formatters (applied with --fix)", FORMATTERS);
    let lints = section("Lints", LINTS);
    format!(
        "{formatters}\n\n{lints}\n\n\
        Configuration:\n  \
        Settings are read from the closest `{CONFIG_FILE_NAME}` at or above PATH, or from --config.\n  \
        Run `control_freak --init` to create one with all the defaults."
    )
}

#[expect(clippy::print_stdout, reason = "CLI output")]
fn init_config(path: &Path) -> Result<()> {
    anyhow::ensure!(!path.exists(), "{} already exists", path.display());
    fs::write(path, Config::default_toml()?)
        .with_context(|| format!("Failed to write {}", path.display()))?;
    println!("Created {}", path.display());
    Ok(())
}

fn main() -> Result<()> {
    let Cli {
        path,
        fix,
        config,
        init,
    } = Cli::parse();

    if init {
        return init_config(&config.unwrap_or_else(|| path.join(CONFIG_FILE_NAME)));
    }

    let config = match config {
        Some(config_path) => Config::load(&config_path)?,
        None => Config::load_for_path(&path)?,
    };
    let num_problems = format_path_with_config(&config, &path, fix)?;

    if num_problems > 0 {
        std::process::exit(1);
    }

    Ok(())
}
