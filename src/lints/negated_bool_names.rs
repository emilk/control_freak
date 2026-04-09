use ra_ap_syntax::{AstNode, SyntaxNode, ast, ast::HasName};

use super::LintWarning;

const NEGATION_WORDS: &[&str] = &[
    "cannot",
    "cant",
    "disable",
    "disabled",
    "disallow",
    "disallowed",
    "disconnect",
    "disconnected",
    "dont",
    "illegal",
    "never",
    "no",
    "not",
    "unavailable",
    "unreachable",
    "unusable",
    "without",
];

// const NEGATION_PREFIXES: &[&str] = &["un", "non", "dis"]; // Too many false positives
const NEGATION_PREFIXES: &[&str] = &[];

fn negation_word(name: &str) -> Option<&str> {
    let name = name.strip_prefix("is_").unwrap_or(name);

    for word in name.split('_') {
        let lower = word.to_ascii_lowercase();
        if NEGATION_WORDS.contains(&lower.as_str()) {
            return Some(word);
        }
        if NEGATION_PREFIXES
            .iter()
            .any(|prefix| lower.starts_with(prefix))
        {
            return Some(word);
        }
    }
    None
}

fn is_bool_type(ty: &ast::Type) -> bool {
    ty.to_string().trim() == "bool"
}

fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i == offset {
            return (line, col);
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn lint_negated_bool_names(source: &str) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    let root = parse.syntax_node();

    let mut warnings = Vec::new();
    find_negated_bools(&root, source, &mut warnings);
    warnings
}

fn find_negated_bools(node: &SyntaxNode, source: &str, warnings: &mut Vec<LintWarning>) {
    // Struct/enum record fields: `name: bool`
    if let Some(field) = ast::RecordField::cast(node.clone()) {
        check_named_bool_item(&field.name(), &field.ty(), "field", source, warnings);
    }

    // Function/method definitions: `fn name() -> bool`
    if let Some(func) = ast::Fn::cast(node.clone()) {
        let ret_ty = func.ret_type().and_then(|r| r.ty());
        check_named_bool_item(&func.name(), &ret_ty, "function", source, warnings);
    }

    // Constants: `const NAME: bool = ...`
    if let Some(konst) = ast::Const::cast(node.clone()) {
        check_named_bool_item(&konst.name(), &konst.ty(), "constant", source, warnings);
    }

    // Function parameters: `fn foo(disabled: bool)`
    if let Some(param) = ast::Param::cast(node.clone())
        && let Some(pat) = param.pat()
        && let Some(ident_pat) = ast::IdentPat::cast(pat.syntax().clone())
    {
        check_named_bool_item(
            &ident_pat.name(),
            &param.ty(),
            "parameter",
            source,
            warnings,
        );
    }

    // Statics: `static NAME: bool = ...`
    if let Some(stat) = ast::Static::cast(node.clone()) {
        check_named_bool_item(&stat.name(), &stat.ty(), "static", source, warnings);
    }

    for child in node.children() {
        find_negated_bools(&child, source, warnings);
    }
}

fn check_named_bool_item(
    name: &Option<ast::Name>,
    ty: &Option<ast::Type>,
    kind: &str,
    source: &str,
    warnings: &mut Vec<LintWarning>,
) {
    let Some(name) = name else { return };
    let Some(ty) = ty else { return };
    if !is_bool_type(ty) {
        return;
    }
    let name_str = name.to_string();
    let Some(word) = negation_word(&name_str) else {
        return;
    };
    let offset: usize = name.syntax().text_range().start().into();
    let (line, col) = offset_to_line_col(source, offset);
    warnings.push(LintWarning {
        line,
        col,
        message: format!(
            "{kind} `{name_str}` uses negation word \"{word}\"; use a positive name instead"
        ),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    fn lint(source: &str) -> Vec<String> {
        lint_negated_bool_names(source)
            .into_iter()
            .map(|w| w.message)
            .collect()
    }

    #[test]
    fn test_struct_field_disabled() {
        let warnings = lint(&unindent(
            "
            struct Config {
                is_disabled: bool,
            }
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "field `is_disabled` uses negation word \"disabled\"; use a positive name instead",
            ]
        );
    }

    #[test]
    fn test_struct_field_no_prefix() {
        let warnings = lint(&unindent(
            "
            struct Options {
                no_cache: bool,
            }
            ",
        ));
        assert_eq!(
            warnings,
            vec!["field `no_cache` uses negation word \"no\"; use a positive name instead",]
        );
    }

    #[test]
    fn test_positive_name_no_warning() {
        let warnings = lint(&unindent(
            "
            struct Config {
                is_enabled: bool,
                visible: bool,
                active: bool,
            }
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_negated_name_non_bool_no_warning() {
        let warnings = lint(&unindent(
            "
            struct Config {
                disabled_count: u32,
                no_cache: String,
            }
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_function_returning_bool() {
        let warnings = lint(&unindent(
            "
            fn is_disabled() -> bool {
                false
            }
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "function `is_disabled` uses negation word \"disabled\"; use a positive name instead",
            ]
        );
    }

    #[test]
    fn test_method_returning_bool() {
        let warnings = lint(&unindent(
            "
            impl Foo {
                fn is_not_ready(&self) -> bool {
                    false
                }
            }
            ",
        ));
        assert_eq!(
            warnings,
            vec!["function `is_not_ready` uses negation word \"not\"; use a positive name instead",]
        );
    }

    #[test]
    fn test_function_positive_name_no_warning() {
        let warnings = lint(&unindent(
            "
            fn is_enabled() -> bool {
                true
            }
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_function_negated_name_non_bool_no_warning() {
        let warnings = lint(&unindent(
            "
            fn disable_feature() -> String {
                String::new()
            }
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_const_bool() {
        let warnings = lint(&unindent(
            "
            const DISABLED: bool = false;
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "constant `DISABLED` uses negation word \"DISABLED\"; use a positive name instead",
            ]
        );
    }

    #[test]
    fn test_static_bool() {
        let warnings = lint(&unindent(
            "
            static NOT_READY: bool = false;
            ",
        ));
        assert_eq!(
            warnings,
            vec!["static `NOT_READY` uses negation word \"NOT\"; use a positive name instead",]
        );
    }

    #[test]
    fn test_parameter_negated_bool() {
        let warnings = lint(&unindent(
            "
            fn setup(disabled: bool) {}
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "parameter `disabled` uses negation word \"disabled\"; use a positive name instead",
            ]
        );
    }

    #[test]
    fn test_parameter_positive_name_no_warning() {
        let warnings = lint(&unindent(
            "
            fn setup(enabled: bool) {}
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parameter_negated_non_bool_no_warning() {
        let warnings = lint(&unindent(
            "
            fn setup(no_cache: String) {}
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_parameter_and_return_type() {
        let warnings = lint(&unindent(
            "
            fn is_not_ready(no_cache: bool) -> bool {
                false
            }
            ",
        ));
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_all_negation_words() {
        let warnings = lint(&unindent(
            "
            struct NegationTest {
                cant_proceed: bool,
                cannot_proceed: bool,
                disable_feature: bool,
                disabled: bool,
                dont_retry: bool,
                never_show: bool,
                no_wrap: bool,
                not_ready: bool,
                without_border: bool,
            }
            ",
        ));
        assert_eq!(warnings.len(), 9);
    }

    #[test]
    fn test_word_boundary_no_false_positive() {
        let warnings = lint(&unindent(
            "
            struct Good {
                notable: bool,
                annotation: bool,
                nonce_valid: bool,
            }
            ",
        ));
        // "notable" does not contain "not" as a word boundary,
        // "annotation" does not contain "no" as a word boundary,
        // "nonce_valid" does not contain "no" as a word boundary.
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_case_insensitive_const() {
        // Consts/statics use UPPER_SNAKE_CASE
        let warnings = lint(&unindent(
            "
            const NEVER_RETRY: bool = false;
            const NO_CACHE: bool = true;
            ",
        ));
        assert_eq!(warnings.len(), 2);
    }

    #[test]
    fn test_line_and_col() {
        let source = unindent(
            "
            struct Config {
                is_disabled: bool,
            }
            ",
        );
        let warnings = lint_negated_bool_names(&source);
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].line, 2);
        assert_eq!(warnings[0].col, 5);
    }

    #[test]
    fn test_multiple_fields_multiple_warnings() {
        let warnings = lint(&unindent(
            "
            struct Config {
                is_disabled: bool,
                not_visible: bool,
                is_enabled: bool,
            }
            ",
        ));
        assert_eq!(warnings.len(), 2);
    }
}
