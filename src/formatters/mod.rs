pub mod flip_negated_if;

pub use flip_negated_if::flip_negated_ifs;

use crate::RuleInfo;

/// All formatters.
pub const FORMATTERS: &[RuleInfo] = &[RuleInfo {
    name: "flip_negated_if",
    summary: "Rewrite `if !a { A } else { B }` as `if a { B } else { A }`",
}];
