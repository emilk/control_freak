use anyhow::Result;
use clap::Parser;
use control_freak::format_path;
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "An opinionated Rust source code formatter and linter")]
struct Cli {
    /// File or directory to process
    path: PathBuf,

    /// Apply formatting fixes to the files (by default, only report them)
    #[arg(long)]
    fix: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let num_warnings = format_path(&cli.path, cli.fix)?;

    if num_warnings > 0 {
        std::process::exit(1);
    }

    Ok(())
}
