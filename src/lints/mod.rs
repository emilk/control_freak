pub mod max_struct_fields;
pub mod max_visible_bindings;
pub mod negated_bool_names;
pub mod use_destructuring;

pub use max_struct_fields::lint_max_struct_fields;
pub use max_visible_bindings::lint_max_visible_bindings;
pub use negated_bool_names::lint_negated_bool_names;
pub use use_destructuring::{lint_use_destructuring, lint_use_destructuring_with_min_fields};

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
