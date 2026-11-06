use ra_ap_syntax::{AstNode as _, ast};

use super::{LintWarning, offset_to_line_col};

/// Warn about parameters of type `&dyn Any` or `&mut dyn Any`.
///
/// `Box<dyn Any>` itself implements `Any`, so it is easy to pass `&boxed`
/// where `&*boxed` was meant, and then every downcast fails.
pub fn lint_dyn_any_params(source: &str) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);

    parse
        .syntax_node()
        .descendants()
        .filter_map(ast::Param::cast)
        .filter_map(|param| {
            let ty = param.ty()?;
            let ast::Type::RefType(reference) = &ty else {
                return None;
            };
            if !is_dyn_any(&reference.ty()?) {
                return None;
            }
            let (line, col) = offset_to_line_col(source, usize::from(ty.syntax().text_range().start()));
            Some(LintWarning {
                line,
                col,
                message: format!(
                    "parameter of type `{ty}`; `Box<dyn Any>` is itself `Any`, so it is easy to pass the wrong thing. Use a purpose-made trait instead"
                ),
            })
        })
        .collect()
}

/// `dyn Any`, `dyn std::any::Any`, `(dyn Any + Send)`, …
fn is_dyn_any(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::ParenType(paren) => paren.ty().is_some_and(|ty| is_dyn_any(&ty)),
        ast::Type::DynTraitType(dyn_trait) => dyn_trait.type_bound_list().is_some_and(|bounds| {
            bounds.bounds().any(|bound| {
                let Some(ast::Type::PathType(path)) = bound.ty() else {
                    return false;
                };
                path.path()
                    .and_then(|path| path.segment())
                    .is_some_and(|segment| segment.to_string() == "Any")
            })
        }),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lint(source: &str) -> Vec<String> {
        lint_dyn_any_params(source)
            .into_iter()
            .map(|warning| warning.to_string())
            .collect()
    }

    #[test]
    fn lints_references_to_dyn_any() {
        let warnings = lint(
            "fn a(x: &dyn Any) {}
            fn b(x: &'a mut dyn std::any::Any) {}
            fn c(x: &(dyn Any + Send)) {}
            fn d() { let f = |x: &dyn core::any::Any| (); }",
        );
        assert_eq!(warnings.len(), 4);
        assert!(warnings[0].starts_with("1:9: parameter of type `&dyn Any`"));
    }

    #[test]
    fn allows_other_types() {
        let warnings = lint(
            "fn a(x: Box<dyn Any>, y: &dyn Debug, z: &impl Any, w: &dyn Many) {}
            fn b(x: dyn Any) {}",
        );
        assert!(warnings.is_empty());
    }
}
