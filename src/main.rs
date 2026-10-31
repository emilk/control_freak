use anyhow::Result;
use clap::Parser;
use control_freak::{RuleInfo, format_path, formatters::FORMATTERS, lints::LINTS};
use std::fmt::Write as _;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    about = "An opinionated Rust source code formatter and linter",
    after_help = rules_help()
)]
struct Cli {
    /// File or directory to process
    path: PathBuf,

    /// Apply formatting fixes to the files (by default, only report them)
    #[arg(long)]
    fix: bool,
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
    format!(
        "{}\n\n{}",
        section("Formatters (applied with --fix)", FORMATTERS),
        section("Lints", LINTS),
    )
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let num_problems = format_path(&cli.path, cli.fix)?;

    if num_problems > 0 {
        std::process::exit(1);
    }

    Ok(())
}
