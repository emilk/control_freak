pub mod dyn_any_params;
pub mod max_struct_fields;
pub mod max_visible_bindings;
pub mod negated_bool_names;
pub mod unnamed_error_fields;
pub mod use_destructuring;
pub mod zip_chain_methods;

pub use dyn_any_params::lint_dyn_any_params;
pub use max_struct_fields::lint_max_struct_fields;
pub use max_visible_bindings::{lint_max_mut_bindings, lint_max_visible_bindings};
pub use negated_bool_names::lint_negated_bool_names;
pub use unnamed_error_fields::lint_unnamed_error_fields;
pub use use_destructuring::lint_use_destructuring;
pub use zip_chain_methods::lint_zip_chain_methods;

use crate::RuleInfo;

/// All lints, sorted by name.
pub const LINTS: &[RuleInfo] = &[
    RuleInfo {
        name: "dyn_any_params",
        summary: "Parameters of type `&dyn Any`, which a `&Box<dyn Any>` also fits",
    },
    RuleInfo {
        name: "max_mut_bindings",
        summary: "Too many mutable bindings visible at once in a function",
    },
    RuleInfo {
        name: "max_struct_fields",
        summary: "Structs with too many fields",
    },
    RuleInfo {
        name: "max_visible_bindings",
        summary: "Too many bindings visible at once in a function",
    },
    RuleInfo {
        name: "negated_bool_names",
        summary: "Boolean names with negation words, like `is_disabled`",
    },
    RuleInfo {
        name: "unnamed_error_fields",
        summary: "`#[error]` on tuple variants with 2+ fields, like `#[error(\"{0} {1}\")]`",
    },
    RuleInfo {
        name: "use_destructuring",
        summary: "Methods that access every field of `self` one by one",
    },
    RuleInfo {
        name: "zip_chain_methods",
        summary: "Method calls `a.zip(b)` and `a.chain(b)`, instead of `std::iter::zip(a, b)`",
    },
];

#[derive(Debug, PartialEq, Eq)]
pub struct LintWarning {
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl std::fmt::Display for LintWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

/// Convert a byte offset into a 1-based `(line, column)` pair.
pub(crate) fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (index, character) in source.char_indices() {
        if index == offset {
            return (line, col);
        }
        if character == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}
