use std::collections::{BTreeMap, BTreeSet};

use ra_ap_syntax::{AstNode as _, SyntaxNode, ast, ast::HasName as _};

use super::{LintWarning, offset_to_line_col};

const DEFAULT_MIN_FIELDS: usize = 3;

pub fn lint_use_destructuring(source: &str) -> Vec<LintWarning> {
    lint_use_destructuring_with_min_fields(source, DEFAULT_MIN_FIELDS)
}

pub fn lint_use_destructuring_with_min_fields(source: &str, min_fields: usize) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    let root = parse.syntax_node();
    let structs = structs(&root);

    root.descendants()
        .filter_map(ast::Impl::cast)
        .flat_map(|imp| lint_impl(&imp, &structs, source, min_fields))
        .collect()
}

fn structs(root: &SyntaxNode) -> BTreeMap<String, Vec<String>> {
    root.descendants()
        .filter_map(ast::Struct::cast)
        .filter_map(|item| {
            let name = item.name()?.to_string();
            let fields = match item.field_list()? {
                ast::FieldList::RecordFieldList(fields) => fields
                    .fields()
                    .filter_map(|field| field.name().map(|name| name.to_string()))
                    .collect(),
                ast::FieldList::TupleFieldList(fields) => (0..fields.fields().count())
                    .map(|index| index.to_string())
                    .collect(),
            };
            Some((name, fields))
        })
        .collect()
}

fn lint_impl(
    imp: &ast::Impl,
    structs: &BTreeMap<String, Vec<String>>,
    source: &str,
    min_fields: usize,
) -> Vec<LintWarning> {
    let Some(type_name) = impl_type_name(imp) else {
        return Vec::new();
    };
    let Some(fields) = structs.get(&type_name) else {
        return Vec::new();
    };
    if fields.len() < min_fields {
        return Vec::new();
    }

    imp.syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .param_list()
                .and_then(|params| params.self_param())
                .is_some()
        })
        .filter_map(|function| lint_function(&function, &type_name, fields, source))
        .collect()
}

fn impl_type_name(imp: &ast::Impl) -> Option<String> {
    let types: Vec<_> = imp
        .syntax()
        .children()
        .filter_map(ast::Type::cast)
        .collect();
    let ty = if imp.for_token().is_some() {
        types.last()?
    } else {
        types.first()?
    };
    ty.to_string()
        .split('<')
        .next()?
        .rsplit("::")
        .next()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
}

fn lint_function(
    function: &ast::Fn,
    type_name: &str,
    fields: &[String],
    source: &str,
) -> Option<LintWarning> {
    let body = function.body()?;
    let field_names: BTreeSet<_> = fields.iter().map(String::as_str).collect();
    let mut accessed_fields = BTreeSet::new();
    let mut first_access = None;
    let mut used_as_a_whole = false;
    let mut mutates_field = false;

    for node in body.syntax().descendants() {
        if let Some(path) = ast::PathExpr::cast(node.clone())
            && path.to_string() == "self"
            && !is_field_receiver(&path)
        {
            used_as_a_whole = true;
        }

        let Some(field) = ast::FieldExpr::cast(node) else {
            continue;
        };
        let Some(receiver) = field
            .expr()
            .and_then(|expr| ast::PathExpr::cast(expr.syntax().clone()))
        else {
            continue;
        };
        if receiver.to_string() != "self" {
            continue;
        }
        let Some(name) = field.name_ref().map(|name| name.to_string()) else {
            continue;
        };
        if !field_names.contains(name.as_str()) {
            continue;
        }

        if is_assignment_target(&field) {
            mutates_field = true;
        }
        first_access.get_or_insert(field.syntax().text_range().start());
        accessed_fields.insert(name);
    }

    if used_as_a_whole || mutates_field || accessed_fields.len() != fields.len() {
        return None;
    }

    let bindings = bindings(function);
    if fields.iter().any(|field| bindings.contains(field)) {
        return None;
    }

    let offset = usize::from(first_access?);
    let (line, col) = offset_to_line_col(source, offset);
    let pattern = if fields
        .first()
        .is_some_and(|field| field.chars().all(char::is_numeric))
    {
        let bindings = (0..fields.len())
            .map(|index| format!("field_{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("let {type_name}({bindings}) = self;")
    } else {
        format!("let {type_name} {{ {} }} = self;", fields.join(", "))
    };
    Some(LintWarning {
        line,
        col,
        message: format!(
            "all {} fields of `{type_name}` are accessed individually; consider destructuring: `{pattern}`",
            fields.len()
        ),
    })
}

fn is_field_receiver(path: &ast::PathExpr) -> bool {
    path.syntax()
        .parent()
        .and_then(ast::FieldExpr::cast)
        .is_some_and(|field| {
            field
                .expr()
                .is_some_and(|expr| expr.syntax() == path.syntax())
        })
}

fn is_assignment_target(field: &ast::FieldExpr) -> bool {
    field
        .syntax()
        .parent()
        .and_then(ast::BinExpr::cast)
        .is_some_and(|assignment| {
            matches!(assignment.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                && assignment
                    .lhs()
                    .is_some_and(|lhs| lhs.syntax() == field.syntax())
        })
}

fn bindings(function: &ast::Fn) -> BTreeSet<String> {
    function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter_map(|pat| pat.name().map(|name| name.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    fn lint(source: &str) -> Vec<String> {
        lint_use_destructuring(source)
            .into_iter()
            .map(|warning| warning.message)
            .collect()
    }

    #[test]
    fn lints_named_struct_fields_accessed_through_self() {
        let warnings = lint(&unindent(
            "
            struct Point { x: i32, y: i32, z: i32 }
            impl Point {
                fn sum(&self) -> i32 { self.x + self.y + self.z }
            }
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "all 3 fields of `Point` are accessed individually; consider destructuring: `let Point { x, y, z } = self;`",
            ]
        );
    }

    #[test]
    fn lints_tuple_struct_fields_accessed_through_self() {
        let warnings = lint(&unindent(
            "
            struct Triple(i32, i32, i32);
            impl Triple {
                fn sum(self) -> i32 { self.0 + self.1 + self.2 }
            }
            ",
        ));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("let Triple(field_0, field_1, field_2) = self;"));
    }

    #[test]
    fn does_not_lint_partial_or_small_structs() {
        let warnings = lint(&unindent(
            "
            struct Triple { a: i32, b: i32, c: i32 }
            struct Pair { a: i32, b: i32 }
            impl Triple { fn partial(&self) -> i32 { self.a + self.b } }
            impl Pair { fn sum(&self) -> i32 { self.a + self.b } }
            ",
        ));
        assert!(warnings.is_empty());
    }

    #[test]
    fn respects_configured_minimum_field_count() {
        let source = unindent(
            "
            struct Triple { a: i32, b: i32, c: i32 }
            impl Triple { fn sum(&self) -> i32 { self.a + self.b + self.c } }
            ",
        );

        assert!(lint_use_destructuring_with_min_fields(&source, 4).is_empty());
        assert_eq!(lint_use_destructuring_with_min_fields(&source, 3).len(), 1);
    }

    #[test]
    fn does_not_lint_whole_self_use_mutation_or_name_collision() {
        let warnings = lint(&unindent(
            "
            struct Point { x: i32, y: i32, z: i32 }
            fn consume(_: Point) {}
            impl Point {
                fn whole(self) { let _ = self.x + self.y + self.z; consume(self); }
                fn mutate(&mut self) { self.x = 0; let _ = self.y + self.z; }
                fn mutate_add(&mut self) { self.x += 1; let _ = self.y + self.z; }
                fn collision(&self, x: i32) -> i32 { x + self.x + self.y + self.z }
            }
            ",
        ));
        assert!(warnings.is_empty());
    }
}
