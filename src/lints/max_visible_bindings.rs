use std::collections::BTreeMap;

use ra_ap_syntax::{
    AstNode, TextRange, TextSize, ast,
    ast::{HasLoopBody, HasName},
};

use super::{LintWarning, offset_to_line_col};

/// A named binding (parameter, `let`, pattern, …) and where it is visible.
struct Binding {
    name: String,

    /// Where the name is introduced.
    site: TextSize,

    /// The range of code that can see the binding.
    scope: TextRange,

    /// Allows mutation, e.g. `let mut x`, `x: &mut T`, or `&mut self`.
    is_mut: bool,
}

/// Which bindings to count.
#[derive(Clone, Copy)]
enum Count {
    All,
    Mut,
}

/// Warn about functions where more than `max_bindings` names are visible at once.
///
/// Counts parameters (including `self`), `let` bindings, closure parameters,
/// and bindings introduced by `if let`, `while let`, `for`, and `match` arms.
/// A shadowed name counts once.
pub fn lint_max_visible_bindings(source: &str, max_bindings: usize) -> Vec<LintWarning> {
    lint(source, max_bindings, Count::All)
}

/// Like [`lint_max_visible_bindings`], but only counts `mut` bindings.
///
/// A `mut` binding shadowed by an immutable one no longer counts.
pub fn lint_max_mut_bindings(source: &str, max_bindings: usize) -> Vec<LintWarning> {
    lint(source, max_bindings, Count::Mut)
}

fn lint(source: &str, max_bindings: usize, count: Count) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);

    parse
        .syntax_node()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter_map(|function| lint_function(&function, source, max_bindings, count))
        .collect()
}

fn lint_function(
    function: &ast::Fn,
    source: &str,
    max_bindings: usize,
    count: Count,
) -> Option<LintWarning> {
    let body = function.body()?.syntax().text_range();
    let mut bindings: Vec<Binding> = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter(|pat| pat.syntax().ancestors().find_map(ast::Fn::cast).as_ref() == Some(function))
        .filter_map(|pat| {
            let name = pat.name()?;
            // `None`, `Ordering::Less`, etc. parse as identifier patterns too.
            if name.text().starts_with(char::is_uppercase) {
                return None;
            }
            Some(Binding {
                name: name.text().to_string(),
                site: pat.syntax().text_range().start(),
                scope: scope(&pat, body)?,
                is_mut: is_mut(&pat),
            })
        })
        .collect();
    if let Some(self_param) = function.param_list().and_then(|params| params.self_param()) {
        bindings.push(Binding {
            name: "self".to_owned(),
            site: self_param.syntax().text_range().start(),
            scope: body,
            is_mut: self_param.mut_token().is_some()
                || self_param.ty().is_some_and(|ty| is_mut_ref(&ty)),
        });
    }
    bindings.sort_by_key(|binding| (binding.scope.start(), binding.site));

    let is_counted = |binding: &Binding| match count {
        Count::All => true,
        Count::Mut => binding.is_mut,
    };

    let (binding, num_visible) = bindings
        .iter()
        .filter(|binding| is_counted(binding))
        .find_map(|binding| {
            // Later bindings shadow earlier ones with the same name.
            let visible: BTreeMap<&str, &Binding> = bindings
                .iter()
                .filter(|other| other.scope.contains(binding.scope.start()))
                .map(|other| (other.name.as_str(), other))
                .collect();
            let num_visible = visible.values().filter(|other| is_counted(other)).count();
            (max_bindings < num_visible).then_some((binding, num_visible))
        })?;
    let kind = match count {
        Count::All => "bindings",
        Count::Mut => "mutable bindings",
    };

    let (line, col) = offset_to_line_col(source, usize::from(binding.site));
    let function_name = function.name()?;
    Some(LintWarning {
        line,
        col,
        message: format!(
            "{num_visible} {kind} are visible in `{function_name}` (max {max_bindings}); consider splitting it up"
        ),
    })
}

/// Is `pat` declared with `mut`, or (as far as we can tell) a `&mut` reference?
fn is_mut(pat: &ast::IdentPat) -> bool {
    if pat.mut_token().is_some() {
        return true;
    }
    let Some(parent) = pat.syntax().parent() else {
        return false;
    };
    if let Some(param) = ast::Param::cast(parent.clone()) {
        param.ty().is_some_and(|ty| is_mut_ref(&ty))
    } else if let Some(stmt) = ast::LetStmt::cast(parent) {
        stmt.ty().is_some_and(|ty| is_mut_ref(&ty))
            || matches!(stmt.initializer(), Some(ast::Expr::RefExpr(expr)) if expr.mut_token().is_some())
    } else {
        false
    }
}

fn is_mut_ref(ty: &ast::Type) -> bool {
    matches!(ty, ast::Type::RefType(ty) if ty.mut_token().is_some())
}

/// The range in which the binding `pat` is visible.
fn scope(pat: &ast::IdentPat, fn_body: TextRange) -> Option<TextRange> {
    for node in pat.syntax().ancestors().skip(1) {
        if let Some(param) = ast::Param::cast(node.clone()) {
            let owner = param.syntax().parent()?.parent()?;
            return match ast::ClosureExpr::cast(owner) {
                Some(closure) => Some(closure.body()?.syntax().text_range()),
                None => Some(fn_body),
            };
        }
        if let Some(stmt) = ast::LetStmt::cast(node.clone()) {
            let block = stmt.syntax().parent()?.text_range();
            return Some(TextRange::new(
                stmt.syntax().text_range().end(),
                block.end(),
            ));
        }
        if let Some(let_expr) = ast::LetExpr::cast(node.clone()) {
            let body = let_expr.syntax().ancestors().find_map(|ancestor| {
                if let Some(if_expr) = ast::IfExpr::cast(ancestor.clone()) {
                    Some(if_expr.then_branch()?.syntax().text_range())
                } else {
                    let while_expr = ast::WhileExpr::cast(ancestor)?;
                    Some(while_expr.loop_body()?.syntax().text_range())
                }
            })?;
            return Some(TextRange::new(
                let_expr.syntax().text_range().end(),
                body.end(),
            ));
        }
        if let Some(for_expr) = ast::ForExpr::cast(node.clone()) {
            return Some(for_expr.loop_body()?.syntax().text_range());
        }
        if let Some(arm) = ast::MatchArm::cast(node.clone()) {
            return Some(TextRange::new(
                arm.pat()?.syntax().text_range().end(),
                arm.syntax().text_range().end(),
            ));
        }
        if ast::Fn::can_cast(node.kind()) {
            return None;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str, max_bindings: usize) -> Vec<String> {
        lint_max_visible_bindings(source, max_bindings)
            .into_iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn counts_params_and_lets() {
        let source = "fn f(a: i32) {\n    let b = a;\n    let c = b;\n}";
        assert!(lint(source, 3).is_empty());
        assert_eq!(
            lint(source, 2),
            vec!["3:9: 3 bindings are visible in `f` (max 2); consider splitting it up"]
        );
    }

    #[test]
    fn counts_self_and_closure_params() {
        assert_eq!(lint("impl S { fn f(&self, a: i32) {} }", 1).len(), 1);
        assert_eq!(lint("fn f() { let g = |a, b| a + b; }", 1).len(), 1);
    }

    #[test]
    fn shadowed_names_count_once() {
        assert!(lint("fn f(a: i32) { let a = a; let a = a; }", 1).is_empty());
    }

    #[test]
    fn bindings_end_with_their_scope() {
        let sources = [
            "fn f() { { let a = 0; let b = 0; } let c = 0; let d = 0; }",
            "fn f(x: Option<(i32, i32)>) { if let Some((a, b)) = x {} else { let c = 0; } }",
            "fn f(x: Option<(i32, i32)>) { match x { Some((a, b)) => {} None => { let c = 0; } } }",
            "fn f(x: Vec<(i32, i32)>) { for (a, b) in x {} let c = 0; }",
            "fn f() { let g = |a, b| a + b; let c = 0; }",
        ];
        for source in sources {
            assert!(lint(source, 3).is_empty(), "{source}");
        }
    }

    #[test]
    fn nested_functions_are_separate() {
        assert!(
            lint(
                "fn outer(a: i32, b: i32) { fn inner(c: i32, d: i32) {} }",
                2
            )
            .is_empty()
        );
    }

    #[test]
    fn mut_counts_only_mut_bindings() {
        let source = "fn f(mut a: i32, b: i32) {\n    let c = 0;\n    let mut d = 0;\n}";
        assert!(lint_max_mut_bindings(source, 2).is_empty());
        assert_eq!(
            lint_max_mut_bindings(source, 1)
                .into_iter()
                .map(|warning| warning.to_string())
                .collect::<Vec<_>>(),
            vec!["3:9: 2 mutable bindings are visible in `f` (max 1); consider splitting it up"]
        );
    }

    #[test]
    fn mut_counts_mut_references() {
        let sources = [
            "impl S { fn f(&mut self, a: &mut i32) {} }",
            "impl S { fn f(self: &mut Self, ref mut a: i32) {} }",
            "fn f(a: &mut i32) { let b: &mut i32 = a; }",
            "fn f(a: &mut i32) { let b = &mut *a; }",
        ];
        for source in sources {
            assert_eq!(lint_max_mut_bindings(source, 1).len(), 1, "{source}");
        }
    }

    #[test]
    fn mut_ignores_shared_references_and_shadowed_mut() {
        let sources = [
            "impl S { fn f(&self, a: &i32, ref b: i32) { let c = &a; } }",
            "fn f(mut a: i32) { let a = a; let mut b = 0; }",
        ];
        for source in sources {
            assert!(lint_max_mut_bindings(source, 1).is_empty(), "{source}");
        }
        assert_eq!(
            lint_max_mut_bindings("impl S { fn f(mut self, mut a: i32) {} }", 1).len(),
            1
        );
    }

    #[test]
    fn reports_once_per_function() {
        assert_eq!(lint("fn f(a: i32, b: i32, c: i32, d: i32) {}", 1).len(), 1);
    }
}
