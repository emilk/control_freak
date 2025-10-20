use ra_ap_syntax::{AstNode, SyntaxNode, ast};

pub fn flip_negated_ifs(source: &str) -> String {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    let root = parse.syntax_node();

    let mut replacements = Vec::new();
    find_negated_ifs(&root, &mut replacements);

    // Apply replacements from end to start to preserve offsets
    replacements.sort_by(|a, b| b.0.cmp(&a.0));

    let mut result = source.to_string();
    for (start, end, replacement) in replacements {
        result.replace_range(start..end, &replacement);
    }

    result
}

fn find_negated_ifs(node: &SyntaxNode, replacements: &mut Vec<(usize, usize, String)>) {
    if let Some(if_expr) = ast::IfExpr::cast(node.clone())
        && let Some(condition) = if_expr.condition()
        && let Some(negation) = check_for_negation(&condition)
        && let Some(replacement) = build_flipped_if(&if_expr, &negation)
    {
        let start = node.text_range().start().into();
        let end = node.text_range().end().into();
        replacements.push((start, end, replacement));
        // Don't recurse into children since we're replacing this entire if expression
        return;
    }

    for child in node.children() {
        find_negated_ifs(&child, replacements);
    }
}

fn check_for_negation(expr: &ast::Expr) -> Option<String> {
    if let Some(prefix_expr) = ast::PrefixExpr::cast(expr.syntax().clone())
        && let Some(op) = prefix_expr.op_kind()
        && op == ast::UnaryOp::Not
        && let Some(inner_expr) = prefix_expr.expr()
    {
        return Some(inner_expr.to_string());
    }

    None
}

fn build_flipped_if(if_expr: &ast::IfExpr, negated_expr: &str) -> Option<String> {
    let then_branch = if_expr.then_branch()?;
    let else_branch = if_expr.else_branch()?;

    // Only handle simple if-else (not else-if)
    // else-if cases are too complex to flip correctly
    let else_block = match else_branch {
        ast::ElseBranch::Block(block) => block,
        ast::ElseBranch::IfExpr(_) => return None,
    };

    // Build the flipped if statement
    let mut result = String::new();

    result.push_str("if ");
    result.push_str(negated_expr);
    result.push(' ');
    result.push_str(&else_block.to_string());
    result.push_str(" else ");
    result.push_str(&then_branch.to_string());

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    #[test]
    fn test_simple_negation() {
        let input = unindent(
            "
            fn example(value: bool) {
                if !value {
                    println!(\"false branch\");
                } else {
                    println!(\"true branch\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(value: bool) {
            if value {
                println!("true branch");
            } else {
                println!("false branch");
            }
        }
        "#);
    }

    #[test]
    fn test_method_call_negation() {
        let input = unindent(
            "
            fn example(x: i32) {
                if !x.is_positive() {
                    println!(\"not positive\");
                } else {
                    println!(\"positive\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(x: i32) {
            if x.is_positive() {
                println!("positive");
            } else {
                println!("not positive");
            }
        }
        "#);
    }

    #[test]
    fn test_no_change_without_negation() {
        let input = unindent(
            "
            fn example(x: i32) {
                if x > 0 {
                    println!(\"greater than zero\");
                } else {
                    println!(\"zero or negative\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(x: i32) {
            if x > 0 {
                println!("greater than zero");
            } else {
                println!("zero or negative");
            }
        }
        "#);
    }

    #[test]
    fn test_nested_negations() {
        let input = unindent(
            "
            fn example(flag: bool) {
                if !flag {
                    if !flag {
                        println!(\"inner false\");
                    } else {
                        println!(\"inner true\");
                    }
                } else {
                    println!(\"outer true\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(flag: bool) {
            if flag {
                println!("outer true");
            } else {
                if !flag {
                    println!("inner false");
                } else {
                    println!("inner true");
                }
            }
        }
        "#);
    }

    #[test]
    fn test_else_if_not_flipped() {
        let input = unindent(
            "
            fn example(opt: Option<i32>) {
                if !opt.is_some() {
                    println!(\"none\");
                } else if opt.unwrap() > 5 {
                    println!(\"some and greater than 5\");
                } else {
                    println!(\"some and less than or equal to 5\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(opt: Option<i32>) {
            if !opt.is_some() {
                println!("none");
            } else if opt.unwrap() > 5 {
                println!("some and greater than 5");
            } else {
                println!("some and less than or equal to 5");
            }
        }
        "#);
    }

    #[test]
    fn test_no_else_branch() {
        let input = unindent(
            "
            fn example(x: bool) {
                if !x {
                    println!(\"x is false\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(x: bool) {
            if !x {
                println!("x is false");
            }
        }
        "#);
    }

    #[test]
    fn test_preserves_comments() {
        let input = unindent(
            "
            fn example(value: bool) {
                // This is a comment
                if !value {
                    // Comment in then branch
                    println!(\"false\");
                } else {
                    // Comment in else branch
                    println!(\"true\");
                }
            }
            ",
        );

        let output = flip_negated_ifs(&input);
        insta::assert_snapshot!(output, @r#"
        fn example(value: bool) {
            // This is a comment
            if value {
                // Comment in else branch
                println!("true");
            } else {
                // Comment in then branch
                println!("false");
            }
        }
        "#);
    }
}
