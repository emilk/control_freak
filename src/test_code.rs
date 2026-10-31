//! Detect test code: `#[test]` functions, `#[cfg(test)]` items, and integration tests.

use std::ops::RangeInclusive;
use std::path::Path;

use ra_ap_syntax::{AstNode as _, ast, ast::HasAttrs as _};

use crate::lints::offset_to_line_col;

/// 1-based line ranges of items that are only compiled for tests.
pub(crate) fn test_line_ranges(source: &str) -> Vec<RangeInclusive<usize>> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    parse
        .syntax_node()
        .descendants()
        .filter_map(ast::Item::cast)
        .filter(|item| item.attrs().any(|attr| is_test_attribute(&attr)))
        .map(|item| {
            let range = item.syntax().text_range();
            let (start, _) = offset_to_line_col(source, usize::from(range.start()));
            let (end, _) = offset_to_line_col(source, usize::from(range.end()));
            start..=end
        })
        .collect()
}

/// Is `path` an integration test, i.e. inside a `tests/` directory next to a `Cargo.toml`?
pub(crate) fn is_integration_test_file(path: &Path) -> bool {
    path.ancestors().skip(1).any(|dir| {
        dir.file_name().is_some_and(|name| name == "tests")
            && dir
                .parent()
                .is_some_and(|crate_dir| crate_dir.join("Cargo.toml").is_file())
    })
}

/// `#[test]`, `#[tokio::test]`, `#[cfg(test)]`, `#[cfg(all(test, …))]`, …
fn is_test_attribute(attr: &ast::Attr) -> bool {
    match attr.meta() {
        Some(ast::Meta::CfgMeta(cfg)) => {
            let condition = cfg.syntax().text().to_string();
            let words: Vec<&str> = condition
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .collect();
            words.contains(&"test") && !words.contains(&"not") && !words.contains(&"any")
        }
        Some(meta) => meta.path().is_some_and(|path| {
            let path = path.syntax().text().to_string();
            path == "test" || path.ends_with("::test")
        }),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    #[test]
    fn finds_test_items() {
        let source = unindent(
            "
            fn real() {}
            #[test]
            fn unit() {}
            #[tokio::test]
            async fn async_unit() {}
            #[cfg(test)]
            mod tests {
                fn helper() {}
            }
            #[cfg(all(test, feature = \"x\"))]
            fn gated() {}
            #[cfg(not(test))]
            fn not_test() {}
            #[cfg(any(test, feature = \"x\"))]
            fn maybe_test() {}
            ",
        );
        assert_eq!(
            test_line_ranges(&source),
            vec![2..=3, 4..=5, 6..=9, 10..=11]
        );
    }

    #[test]
    fn integration_tests_need_a_crate() -> anyhow::Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let root = temp_dir.path();
        std::fs::create_dir_all(root.join("my_crate/tests"))?;
        std::fs::write(root.join("my_crate/Cargo.toml"), "")?;

        assert!(is_integration_test_file(&root.join("my_crate/tests/it.rs")));
        assert!(is_integration_test_file(
            &root.join("my_crate/tests/common/mod.rs")
        ));
        assert!(!is_integration_test_file(&root.join("my_crate/src/lib.rs")));
        assert!(!is_integration_test_file(
            &root.join("crates/tests/my_test_crate/src/lib.rs")
        ));
        Ok(())
    }
}
