use ra_ap_syntax::{AstNode, ast, ast::HasName};

use super::{LintWarning, offset_to_line_col};

/// Warn about structs with more than `max_fields` fields.
pub fn lint_max_struct_fields(source: &str, max_fields: usize) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);

    parse
        .syntax_node()
        .descendants()
        .filter_map(ast::Struct::cast)
        .filter_map(|item| {
            let name = item.name()?;
            let num_fields = match item.field_list()? {
                ast::FieldList::RecordFieldList(fields) => fields.fields().count(),
                ast::FieldList::TupleFieldList(fields) => fields.fields().count(),
            };
            if num_fields <= max_fields {
                return None;
            }
            let (line, col) =
                offset_to_line_col(source, usize::from(name.syntax().text_range().start()));
            Some(LintWarning {
                line,
                col,
                message: format!(
                    "struct `{name}` has {num_fields} fields (max {max_fields}); consider splitting it up"
                ),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str, max_fields: usize) -> Vec<String> {
        lint_max_struct_fields(source, max_fields)
            .into_iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn lints_structs_above_the_limit() {
        let warnings = lint(
            "struct Ok { a: i32, b: i32 }\nstruct Big { a: i32, b: i32, c: i32 }",
            2,
        );
        assert_eq!(
            warnings,
            vec!["2:8: struct `Big` has 3 fields (max 2); consider splitting it up"]
        );
    }

    #[test]
    fn lints_tuple_structs() {
        assert_eq!(lint("struct Triple(i32, i32, i32);", 2).len(), 1);
    }

    #[test]
    fn ignores_unit_structs_and_enums() {
        assert!(lint("struct Unit; enum E { A { a: i32, b: i32, c: i32 } }", 0).is_empty());
    }
}
