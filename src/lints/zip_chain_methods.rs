use ra_ap_syntax::{AstNode as _, ast, ast::HasArgList as _};

use super::{LintWarning, offset_to_line_col};

/// Warn about `a.zip(b)` and `a.chain(b)`.
///
/// The free functions `std::iter::zip(a, b)` and `std::iter::chain(a, b)` treat
/// both sides the same, which reads better.
/// Without type information, `Option::zip` and `io::Read::chain` are also flagged.
pub fn lint_zip_chain_methods(source: &str) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);

    parse
        .syntax_node()
        .descendants()
        .filter_map(ast::MethodCallExpr::cast)
        .filter_map(|call| {
            let name_ref = call.name_ref()?;
            let name = name_ref.text();
            let suggestion = match name {
                "chain" => "`std::iter::chain(a, b)` or `itertools::chain!(a, b, …)`",
                "zip" => {
                    "`std::iter::zip(a, b)`, `itertools::izip!(a, b, …)`, or `Option::zip(a, b)`"
                }
                _ => return None,
            };
            if call.arg_list()?.args().count() != 1 {
                return None; // e.g. `anyhow::Error::chain()`
            }
            let (line, col) =
                offset_to_line_col(source, usize::from(name_ref.syntax().text_range().start()));
            Some(LintWarning {
                line,
                col,
                message: format!("prefer {suggestion} over `a.{name}(b)`"),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str) -> Vec<String> {
        lint_zip_chain_methods(source)
            .into_iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn lints_zip_and_chain() {
        let warnings = lint("fn f() { a.iter().zip(b).chain(c); }");
        assert_eq!(
            warnings,
            vec![
                "1:26: prefer `std::iter::chain(a, b)` or `itertools::chain!(a, b, …)` over `a.chain(b)`",
                "1:19: prefer `std::iter::zip(a, b)`, `itertools::izip!(a, b, …)`, or `Option::zip(a, b)` over `a.zip(b)`",
            ]
        );
    }

    #[test]
    fn allows_free_functions_and_other_arities() {
        let warnings =
            lint("fn f() { std::iter::zip(a, b); chain(a, b); err.chain(); x.zips(b); }");
        assert!(warnings.is_empty());
    }
}
