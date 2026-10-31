//! Detect generated files using `linguist-generated` in `.gitattributes`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// One `<pattern> linguist-generated…` line of a `.gitattributes` file.
struct Rule {
    pattern: Gitignore,
    is_generated: bool,
}

/// Answers whether a file is marked as generated, caching parsed `.gitattributes` files.
#[derive(Default)]
pub(crate) struct GeneratedFiles {
    /// Rules of the `.gitattributes` in each directory, in file order.
    rules_per_dir: HashMap<PathBuf, Rc<Vec<Rule>>>,
}

impl GeneratedFiles {
    /// Is `path` marked `linguist-generated` by a `.gitattributes` file?
    ///
    /// Deeper `.gitattributes` files and later lines take precedence, like in git.
    /// The search stops at the repository root (the directory containing `.git`).
    pub fn is_generated(&mut self, path: &Path) -> bool {
        for dir in path.ancestors().skip(1) {
            let rules = self.rules(dir);
            if let Some(rule) = rules
                .iter()
                .rev()
                .find(|rule| rule.pattern.matched(path, false).is_ignore())
            {
                return rule.is_generated;
            }
            if dir.join(".git").exists() {
                break;
            }
        }
        false
    }

    fn rules(&mut self, dir: &Path) -> Rc<Vec<Rule>> {
        Rc::clone(
            self.rules_per_dir
                .entry(dir.to_path_buf())
                .or_insert_with(|| Rc::new(parse_gitattributes(dir))),
        )
    }
}

fn parse_gitattributes(dir: &Path) -> Vec<Rule> {
    let Ok(contents) = fs::read_to_string(dir.join(".gitattributes")) else {
        return Vec::new();
    };
    contents
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pattern = parts.next().filter(|pattern| !pattern.starts_with('#'))?;
            let is_generated = parts.find_map(|attribute| match attribute {
                "linguist-generated" | "linguist-generated=true" => Some(true),
                "-linguist-generated" | "!linguist-generated" | "linguist-generated=false" => {
                    Some(false)
                }
                _ => None,
            })?;
            let mut builder = GitignoreBuilder::new(dir);
            builder.add_line(None, pattern).ok()?;
            Some(Rule {
                pattern: builder.build().ok()?,
                is_generated,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deeper_files_and_later_lines_win() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();
        let nested = root.join("crate/src");
        fs::create_dir_all(&nested)?;
        fs::create_dir(root.join(".git"))?;
        fs::write(
            root.join(".gitattributes"),
            "*.rs linguist-generated\nhand.rs -linguist-generated\nfmt.rs text\n",
        )?;
        fs::write(
            root.join("crate/.gitattributes"),
            "src/kept.rs linguist-generated=false\n",
        )?;

        let mut generated = GeneratedFiles::default();
        assert!(generated.is_generated(&nested.join("gen.rs")));
        assert!(!generated.is_generated(&nested.join("hand.rs")));
        assert!(!generated.is_generated(&nested.join("kept.rs")));
        assert!(generated.is_generated(&nested.join("fmt.rs")));
        assert!(!generated.is_generated(&root.join("README.md")));
        Ok(())
    }

    #[test]
    fn stops_at_repository_root() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let repo = temp_dir.path().join("repo");
        fs::create_dir_all(repo.join(".git"))?;
        fs::write(
            temp_dir.path().join(".gitattributes"),
            "*.rs linguist-generated\n",
        )?;

        assert!(!GeneratedFiles::default().is_generated(&repo.join("lib.rs")));
        Ok(())
    }
}
