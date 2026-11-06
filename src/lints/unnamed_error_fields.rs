use ra_ap_syntax::{AstNode as _, ast, ast::HasAttrs as _};

use super::{LintWarning, offset_to_line_col};

/// Warn about `#[error(…)]` on a tuple variant or struct with two or more fields.
///
/// `#[error("{0} {1}")]` is easy to get wrong; named fields read better.
pub fn lint_unnamed_error_fields(source: &str) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);

    parse
        .syntax_node()
        .descendants()
        .filter_map(|node| {
            let (attrs, field_list) = if let Some(variant) = ast::Variant::cast(node.clone()) {
                (variant.attrs(), variant.field_list()?)
            } else {
                let item = ast::Struct::cast(node)?;
                (item.attrs(), item.field_list()?)
            };
            let ast::FieldList::TupleFieldList(fields) = field_list else {
                return None;
            };
            let num_fields = fields.fields().count();
            if num_fields < 2 {
                return None;
            }
            let error_attr = attrs
                .into_iter()
                .find(|attr| attr.simple_name().is_some_and(|name| name == "error"))?;
            let (line, col) =
                offset_to_line_col(source, usize::from(error_attr.syntax().text_range().start()));
            Some(LintWarning {
                line,
                col,
                message: format!(
                    "error with {num_fields} unnamed fields; use named fields instead of `{{0}}`, `{{1}}`, …"
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str) -> Vec<String> {
        lint_unnamed_error_fields(source)
            .into_iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn lints_tuple_errors_with_many_fields() {
        let warnings = lint(
            r#"enum E {
                #[error("{0} {1}")]
                Pair(String, i32),
            }
            #[error("{0}: {1}")]
            struct S(String, i32);"#,
        );
        assert_eq!(
            warnings,
            vec![
                "2:17: error with 2 unnamed fields; use named fields instead of `{0}`, `{1}`, …",
                "5:13: error with 2 unnamed fields; use named fields instead of `{0}`, `{1}`, …",
            ]
        );
    }

    #[test]
    fn allows_single_and_named_fields() {
        let warnings = lint(
            r#"enum E {
                #[error("{0}")]
                One(String),
                #[error("{path}: {err}")]
                Named { path: String, err: io::Error },
                #[error(transparent)]
                Unit,
                Pair(String, i32),
            }"#,
        );
        assert!(warnings.is_empty());
    }
}
