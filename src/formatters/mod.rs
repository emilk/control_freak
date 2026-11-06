pub mod flip_negated_if;
pub mod vertical_spacing;

pub use flip_negated_if::flip_negated_ifs;
pub use vertical_spacing::add_vertical_spacing;

use crate::RuleInfo;

/// All formatters, sorted by name.
pub const FORMATTERS: &[RuleInfo] = &[
    RuleInfo {
        name: "flip_negated_if",
        summary: "Rewrite `if !a { A } else { B }` as `if a { B } else { A }`",
    },
    RuleInfo {
        name: "vertical_spacing",
        summary: "Add a blank line before items, and before documented fields and variants",
    },
];
