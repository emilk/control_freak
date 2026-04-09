pub mod negated_bool_names;

pub use negated_bool_names::lint_negated_bool_names;

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
