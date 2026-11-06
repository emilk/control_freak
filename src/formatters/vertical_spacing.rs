use ra_ap_syntax::{
    AstNode as _, NodeOrToken, SyntaxElement, SyntaxKind, SyntaxNode, T, ast, ast::HasAttrs as _,
};

/// Insert a blank line before items, and before documented fields and
/// variants, that directly follow something else.
///
/// Runs of single-line items, like `fn a();\nfn b();`, and runs of type aliases
/// are left alone.
pub fn add_vertical_spacing(source: &str) -> String {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    let lines: Vec<&str> = source.lines().collect();

    let mut insertions: Vec<usize> = parse
        .syntax_node()
        .descendants()
        .filter(wants_blank_line_before)
        .filter_map(|node| blank_line_insertion(&node))
        .filter(|&offset| {
            let (line, _) = crate::lints::offset_to_line_col(source, offset);
            !crate::is_nolint(&lines, line)
        })
        .collect();

    // Apply insertions from end to start to preserve offsets
    insertions.sort_unstable_by(|a, b| b.cmp(a));
    insertions.dedup();

    let mut result = source.to_owned();
    for offset in insertions {
        result.insert(offset, '\n');
    }
    result
}

fn wants_blank_line_before(node: &SyntaxNode) -> bool {
    use SyntaxKind::{ENUM, FN, IMPL, RECORD_FIELD, STRUCT, TRAIT, TYPE_ALIAS, UNION, VARIANT};

    if matches!(
        node.kind(),
        ENUM | FN | IMPL | STRUCT | TRAIT | TYPE_ALIAS | UNION
    ) {
        return true;
    }

    let is_documentable =
        ast::Item::can_cast(node.kind()) || matches!(node.kind(), RECORD_FIELD | VARIANT);
    is_documentable
        && ast::AnyHasAttrs::cast(node.clone())
            .is_some_and(|node| node.doc_comments().next().is_some())
}

/// Byte offset to insert a newline at, if `node` directly follows another
/// statement, item, field, or variant on the line above.
fn blank_line_insertion(node: &SyntaxNode) -> Option<usize> {
    let whitespace = node.prev_sibling_or_token()?.into_token()?;
    if whitespace.kind() != SyntaxKind::WHITESPACE {
        return None;
    }
    let text = whitespace.text();
    if text.matches('\n').count() != 1 {
        return None; // Already separated, or on the same line
    }

    let previous = previous_sibling(&whitespace.clone().into())?;
    let is_both_single_line = is_single_line(node) && is_single_line(&previous);
    let is_both_type_alias =
        node.kind() == SyntaxKind::TYPE_ALIAS && previous.kind() == SyntaxKind::TYPE_ALIAS;
    if is_both_single_line || is_both_type_alias {
        return None;
    }

    let newline = text.find('\n')?;
    Some(usize::from(node.text_range().start()) - text.len() + newline + 1)
}

/// The previous field, variant, item, or statement, skipping separating commas.
///
/// `None` if `element` is the first one in its list, or follows a comment.
fn previous_sibling(element: &SyntaxElement) -> Option<SyntaxNode> {
    let mut previous = element.prev_sibling_or_token()?;
    loop {
        match previous {
            NodeOrToken::Node(node) => return Some(node),
            NodeOrToken::Token(token) => {
                if !matches!(token.kind(), SyntaxKind::WHITESPACE | T![,] | T![;]) {
                    return None; // `{`, `(`, a comment, …
                }
                previous = token.prev_sibling_or_token()?;
            }
        }
    }
}

fn is_single_line(node: &SyntaxNode) -> bool {
    !node.text().contains_char('\n')
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    #[test]
    fn separates_items() {
        let input = unindent(
            "
            use std::fmt;
            struct A {
                x: i32,
            }
            impl A {
                fn new() -> Self {
                    Self { x: 0 }
                }
                /// Docs
                fn get(&self) -> i32 {
                    self.x
                }
            }
            ",
        );
        insta::assert_snapshot!(add_vertical_spacing(&input), @r"
        use std::fmt;

        struct A {
            x: i32,
        }

        impl A {
            fn new() -> Self {
                Self { x: 0 }
            }

            /// Docs
            fn get(&self) -> i32 {
                self.x
            }
        }
        ");
    }

    #[test]
    fn leaves_runs_of_single_line_items() {
        let input = unindent(
            "
            trait T {
                fn a(&self);
                fn b(&self);
                type A;
                type B = Box<
                    dyn Fn(),
                >;
            }

            struct Marker;
            impl T for Marker {}
            ",
        );
        assert_eq!(add_vertical_spacing(&input), input);
    }

    #[test]
    fn separates_documented_fields_and_variants() {
        let input = unindent(
            "
            struct S {
                a: i32,
                /// Docs
                b: i32,
                #[serde(default)]
                c: i32,
                /// Docs
                d: i32,
            }
            enum E {
                A(i32),
                #[error(\"b\")]
                B,
                /// Docs
                C,
            }
            ",
        );
        insta::assert_snapshot!(add_vertical_spacing(&input), @r#"
        struct S {
            a: i32,

            /// Docs
            b: i32,
            #[serde(default)]
            c: i32,

            /// Docs
            d: i32,
        }

        enum E {
            A(i32),
            #[error("b")]
            B,

            /// Docs
            C,
        }
        "#);
    }

    #[test]
    fn puts_blank_line_above_attached_comment() {
        let input = unindent(
            "
            fn outer() {
                let x = 1;
                // A helper
                fn helper() {}
            }
            ",
        );
        insta::assert_snapshot!(add_vertical_spacing(&input), @r"
        fn outer() {
            let x = 1;

            // A helper
            fn helper() {}
        }
        ");
    }

    #[test]
    fn leaves_first_items_and_macro_bodies() {
        let input = unindent(
            "
            mod m {
                fn a() {}
            }
            macro_rules! m {
                () => {
                    fn a() {}
                    fn b() {}
                };
            }

            fn f() { let x = 1; fn g() {} }
            ",
        );
        assert_eq!(add_vertical_spacing(&input), input);
    }

    #[test]
    fn respects_nolint() {
        let input = unindent(
            "
            struct A;
            fn f() {
            } // NOLINT
            fn g() {
            }
            ",
        );
        insta::assert_snapshot!(add_vertical_spacing(&input), @r"
        struct A;

        fn f() {
        } // NOLINT
        fn g() {
        }
        ");
    }
}
