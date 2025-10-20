use anyhow::Result;
use control_freak::format_path;
use std::env;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file_or_directory>", args[0]);
        std::process::exit(1);
    }

    let path = Path::new(&args[1]);
    format_path(path)?;

    Ok(())
}
