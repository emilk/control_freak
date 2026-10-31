use std::collections::{BTreeMap, BTreeSet};

use ra_ap_syntax::{
    AstNode as _, SyntaxNode, TextRange, TextSize, ast,
    ast::{HasAttrs as _, HasName as _},
};

use super::{LintWarning, offset_to_line_col};
use crate::config::UseDestructuringConfig;

/// A struct defined in the linted file.
struct StructInfo {
    /// Field names. Tuple struct fields are named `0`, `1`, …
    fields: Vec<String>,

    /// Has `#[derive(Copy)]`.
    is_copy: bool,
}

/// Does the binding own the value, or only borrow it?
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ownership {
    Owned,
    Borrowed,
}

/// A binding whose type is a struct defined in the same file.
struct Target {
    /// `self`, or the name of a parameter or `let` binding.
    name: String,

    type_name: String,
    ownership: Ownership,

    /// Where the binding is visible.
    scope: TextRange,
}

/// Warn about values whose fields are all (or, for owned values, almost all)
/// accessed individually, instead of being destructured.
///
/// Destructuring makes the compiler point out code that needs updating when a field is added.
pub fn lint_use_destructuring(source: &str, config: &UseDestructuringConfig) -> Vec<LintWarning> {
    let parse = ra_ap_syntax::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    let root = parse.syntax_node();
    let structs = structs(&root);

    root.descendants()
        .filter_map(ast::Fn::cast)
        .flat_map(|function| {
            targets(&function, &structs)
                .into_iter()
                .filter_map(|target| lint_target(config, source, &function, &structs, &target))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The structs defined in the file, by name.
fn structs(root: &SyntaxNode) -> BTreeMap<String, StructInfo> {
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
            let is_copy = item
                .attrs()
                .filter_map(|attr| attr.as_simple_call())
                .filter(|(name, _)| name == "derive")
                .any(|(_, derives)| {
                    derives
                        .syntax()
                        .text()
                        .to_string()
                        .split(|c: char| !c.is_alphanumeric() && c != '_')
                        .any(|derive| derive == "Copy")
                });
            Some((name, StructInfo { fields, is_copy }))
        })
        .collect()
}

/// `self`, parameters, and `let` bindings of `function` whose type is a known struct.
fn targets(function: &ast::Fn, structs: &BTreeMap<String, StructInfo>) -> Vec<Target> {
    let Some(body) = function.body() else {
        return Vec::new();
    };
    let body = body.syntax().text_range();
    let self_type = self_type(function);
    let known = |type_name: String| structs.contains_key(&type_name).then_some(type_name);

    let mut targets = Vec::new();
    let params = function.param_list();

    if let Some(self_param) = params.as_ref().and_then(|params| params.self_param())
        && self_param.ty().is_none()
        && let Some(type_name) = self_type.clone().and_then(known)
    {
        let ownership = if self_param.amp_token().is_some() {
            Ownership::Borrowed
        } else {
            Ownership::Owned
        };
        targets.push(Target {
            name: "self".to_owned(),
            type_name,
            ownership,
            scope: body,
        });
    }

    for param in params.iter().flat_map(|params| params.params()) {
        if let Some(name) = param.pat().and_then(|pat| plain_binding_name(&pat))
            && let Some((type_name, ownership)) = param
                .ty()
                .and_then(|ty| struct_type(&ty, self_type.as_deref()))
            && let Some(type_name) = known(type_name)
        {
            targets.push(Target {
                name,
                type_name,
                ownership,
                scope: body,
            });
        }
    }

    for stmt in function
        .syntax()
        .descendants()
        .filter_map(ast::LetStmt::cast)
    {
        if stmt.syntax().ancestors().find_map(ast::Fn::cast).as_ref() != Some(function) {
            continue;
        }
        let Some(name) = stmt.pat().and_then(|pat| plain_binding_name(&pat)) else {
            continue;
        };
        let typed = stmt
            .ty()
            .and_then(|ty| struct_type(&ty, self_type.as_deref()))
            .or_else(|| match stmt.initializer()? {
                ast::Expr::RecordExpr(record) => Some((
                    path_type_name(&record.path()?, self_type.as_deref())?,
                    Ownership::Owned,
                )),
                _ => None,
            });
        if let Some((type_name, ownership)) = typed
            && let Some(type_name) = known(type_name)
            && let Some(block) = stmt.syntax().parent()
        {
            targets.push(Target {
                name,
                type_name,
                ownership,
                scope: TextRange::new(stmt.syntax().text_range().end(), block.text_range().end()),
            });
        }
    }

    targets
}

/// The type of `Self`, if `function` is in an `impl` block.
fn self_type(function: &ast::Fn) -> Option<String> {
    let items = ast::AssocItemList::cast(function.syntax().parent()?)?;
    impl_type_name(&ast::Impl::cast(items.syntax().parent()?)?)
}

/// The name of a pattern like `foo` or `mut foo`, but not `ref foo` or `Foo { .. }`.
fn plain_binding_name(pat: &ast::Pat) -> Option<String> {
    let ast::Pat::IdentPat(pat) = pat else {
        return None;
    };
    if pat.ref_token().is_some() || pat.pat().is_some() {
        return None;
    }
    Some(pat.name()?.to_string())
}

/// `Foo` is owned, `&Foo` and `&mut Foo` are borrowed.
fn struct_type(ty: &ast::Type, self_type: Option<&str>) -> Option<(String, Ownership)> {
    match ty {
        ast::Type::PathType(path) => {
            Some((path_type_name(&path.path()?, self_type)?, Ownership::Owned))
        }
        ast::Type::RefType(reference) => match reference.ty()? {
            ast::Type::PathType(path) => Some((
                path_type_name(&path.path()?, self_type)?,
                Ownership::Borrowed,
            )),
            _ => None,
        },
        _ => None,
    }
}

/// The last segment of `path`, with `Self` resolved to `self_type`.
fn path_type_name(path: &ast::Path, self_type: Option<&str>) -> Option<String> {
    let name = path.segment()?.name_ref()?.to_string();
    if name == "Self" {
        self_type.map(ToOwned::to_owned)
    } else {
        Some(name)
    }
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

fn lint_target(
    config: &UseDestructuringConfig,
    source: &str,
    function: &ast::Fn,
    structs: &BTreeMap<String, StructInfo>,
    target: &Target,
) -> Option<LintWarning> {
    let Target {
        name,
        type_name,
        ownership,
        scope,
    } = target;
    let StructInfo { fields, is_copy } = structs.get(type_name)?;
    // Dropping a `Copy` value is harmless, so treat it like a borrow.
    let (min_fields, max_missing_fields) = match ownership {
        Ownership::Owned if !is_copy => (config.owned.min_fields, config.owned.max_missing_fields),
        Ownership::Owned | Ownership::Borrowed => (config.min_fields, 0),
    };
    if fields.len() < min_fields {
        return None;
    }

    let (accessed_fields, first_access) = field_reads(function, target, fields)?;

    // Macro arguments aren't parsed, so we can't tell how the value is used there.
    // A struct literal without `..` already makes the compiler point out new fields.
    if mentioned_in_macro(function, name, *scope)
        || builds_exhaustively(function, type_name, self_type(function).as_deref())
    {
        return None;
    }

    let missing_fields: Vec<&String> = fields
        .iter()
        .filter(|field| !accessed_fields.contains(*field))
        .collect();
    if accessed_fields.is_empty() || max_missing_fields < missing_fields.len() {
        return None;
    }

    // Destructuring must not shadow other bindings, and the target must not be shadowed.
    let has_name_clash = function
        .syntax()
        .descendants()
        .filter_map(ast::IdentPat::cast)
        .filter_map(|pat| Some((pat.name()?.to_string(), pat.syntax().text_range())))
        .any(|(binding, range)| {
            (binding == *name && scope.contains_range(range)) || accessed_fields.contains(&binding)
        });
    if has_name_clash {
        return None;
    }

    let (line, col) = offset_to_line_col(source, usize::from(first_access));
    let pattern = destructuring_pattern(type_name, fields, &accessed_fields, name);
    Some(LintWarning {
        line,
        col,
        message: warning_message(type_name, fields, &missing_fields, &pattern),
    })
}

/// The fields of `target` that are read, and where the first read is.
///
/// Returns `None` if `target` is used as a whole, or a field is assigned to.
fn field_reads(
    function: &ast::Fn,
    target: &Target,
    fields: &[String],
) -> Option<(BTreeSet<String>, TextSize)> {
    let Target { name, scope, .. } = target;
    let mut accessed_fields = BTreeSet::new();
    let mut first_access = None;

    let uses = function
        .body()?
        .syntax()
        .descendants()
        .filter_map(ast::PathExpr::cast)
        .filter(|path| {
            scope.contains_range(path.syntax().text_range()) && path.to_string() == *name
        });
    for path in uses {
        let field = path
            .syntax()
            .parent()
            .and_then(ast::FieldExpr::cast)
            .filter(|field| {
                field
                    .expr()
                    .is_some_and(|expr| expr.syntax() == path.syntax())
            })?;
        let field_name = field.name_ref()?.to_string();
        if !fields.contains(&field_name) || is_assignment_target(&field) {
            return None;
        }
        first_access.get_or_insert_with(|| field.syntax().text_range().start());
        accessed_fields.insert(field_name);
    }
    Some((accessed_fields, first_access?))
}

fn warning_message(
    type_name: &str,
    fields: &[String],
    missing_fields: &[&String],
    pattern: &str,
) -> String {
    if missing_fields.is_empty() {
        format!(
            "all {} fields of `{type_name}` are accessed individually; consider destructuring: `{pattern}`",
            fields.len()
        )
    } else {
        let names = missing_fields
            .iter()
            .map(|field| format!("`{field}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let (noun, verb) = if missing_fields.len() == 1 {
            ("field", "is")
        } else {
            ("fields", "are")
        };
        format!(
            "{noun} {names} of `{type_name}` {verb} never used; consider destructuring to make that explicit: `{pattern}`"
        )
    }
}

/// Does `function` build a `type_name` with a struct literal that lists every field (no `..`)?
fn builds_exhaustively(function: &ast::Fn, type_name: &str, self_type: Option<&str>) -> bool {
    function
        .syntax()
        .descendants()
        .filter_map(ast::RecordExpr::cast)
        .any(|record| {
            record
                .path()
                .and_then(|path| path_type_name(&path, self_type))
                .is_some_and(|name| name == type_name)
                && record
                    .record_expr_field_list()
                    .is_some_and(|fields| fields.dotdot_token().is_none())
        })
}

/// Is `name` (or `{name}` in a format string) used in a macro call within `scope`?
fn mentioned_in_macro(function: &ast::Fn, name: &str, scope: TextRange) -> bool {
    let inline_arg = |text: &str| {
        text.split('{')
            .skip(1)
            .any(|arg| arg.split(['}', ':', '.']).next() == Some(name))
    };
    function
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .filter(|call| scope.contains_range(call.syntax().text_range()))
        .filter_map(|call| call.token_tree())
        .flat_map(|tokens| {
            tokens
                .syntax()
                .descendants_with_tokens()
                .filter_map(|element| element.into_token())
        })
        .any(|token| token.text() == name || inline_arg(token.text()))
}

/// `let Foo { a, b: _ } = foo;` or `let Foo(field_0, _) = foo;`
fn destructuring_pattern(
    type_name: &str,
    fields: &[String],
    accessed_fields: &BTreeSet<String>,
    value: &str,
) -> String {
    let is_tuple = fields
        .first()
        .is_some_and(|field| field.chars().all(char::is_numeric));
    let parts: Vec<String> = fields
        .iter()
        .map(|field| match (is_tuple, accessed_fields.contains(field)) {
            (true, true) => format!("field_{field}"),
            (true, false) => "_".to_owned(),
            (false, true) => field.clone(),
            (false, false) => format!("{field}: _"),
        })
        .collect();
    if is_tuple {
        format!("let {type_name}({}) = {value};", parts.join(", "))
    } else {
        format!("let {type_name} {{ {} }} = {value};", parts.join(", "))
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use unindent::unindent;

    fn lint(source: &str) -> Vec<String> {
        lint_use_destructuring(source, &UseDestructuringConfig::default())
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

        let with_min_fields = |min_fields| UseDestructuringConfig {
            min_fields,
            ..Default::default()
        };
        assert!(lint_use_destructuring(&source, &with_min_fields(4)).is_empty());
        assert_eq!(
            lint_use_destructuring(&source, &with_min_fields(3)).len(),
            1
        );
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

    #[test]
    fn owned_values_with_a_missing_field() {
        let warnings = lint(&unindent(
            "
            struct Point { x: i32, y: i32, z: i32 }
            struct Triple(i32, i32, i32);
            fn owned(point: Point) -> i32 { point.x + point.y }
            fn borrowed(point: &Point) -> i32 { point.x + point.y }
            fn two_missing(point: Point) -> i32 { point.x }
            fn tuple(triple: Triple) -> i32 { triple.0 + triple.2 }
            impl Point { fn into_xy(self) -> (i32, i32) { (self.x, self.y) } }
            ",
        ));
        assert_eq!(
            warnings,
            vec![
                "field `z` of `Point` is never used; consider destructuring to make that explicit: `let Point { x, y, z: _ } = point;`",
                "field `1` of `Triple` is never used; consider destructuring to make that explicit: `let Triple(field_0, _, field_2) = triple;`",
                "field `z` of `Point` is never used; consider destructuring to make that explicit: `let Point { x, y, z: _ } = self;`",
            ]
        );
    }

    #[test]
    fn lints_parameters_and_let_bindings() {
        let warnings = lint(&unindent(
            "
            struct Point { x: i32, y: i32, z: i32 }
            impl Point { fn dot(&self, other: &Self) -> i32 { other.x + other.y + other.z } }
            fn owned_let() -> i32 {
                let point: Point = make_point();
                point.x + point.y + point.z
            }
            fn annotated(points: &[Point]) -> i32 {
                let point: &Point = &points[0];
                point.x + point.y + point.z
            }
            ",
        ));
        assert_eq!(warnings.len(), 3, "{warnings:#?}");
        assert!(warnings[0].contains("= other;"));
    }

    #[test]
    fn skips_macros_shadowing_and_unknown_types() {
        let warnings = lint(&unindent(
            "
            struct Point { x: i32, y: i32, z: i32 }
            fn in_macro(point: Point) -> i32 { println!(\"{}\", point.z); point.x + point.y }
            fn in_format_string(point: Point) -> i32 { println!(\"{point:?}\"); point.x + point.y }
            fn shadowed(point: Point) -> i32 { let point = 0; point }
            fn unknown(other: Other) -> i32 { other.x + other.y + other.z }
            ",
        ));
        assert!(warnings.is_empty(), "{warnings:#?}");
    }

    #[test]
    fn skips_copy_drops_and_exhaustive_struct_literals() {
        let warnings = lint(&unindent(
            "
            #[derive(Clone, Copy)]
            struct Point { x: i32, y: i32, z: i32 }
            struct Id { kind: u8, app: String, rec: String }
            fn copy(point: Point) -> i32 { point.x + point.y }
            impl Id {
                fn with_rec(self, rec: String) -> Self { Self { kind: self.kind, app: self.app, rec } }
                fn sum(&self) -> Id { Id { kind: self.kind, app: self.app.clone(), rec: self.rec.clone() } }
            }
            ",
        ));
        assert!(warnings.is_empty(), "{warnings:#?}");
    }
}
