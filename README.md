# Control Freak
An opinionated Rust source code formatter and linter.

## Install
```sh
cargo install --path .
```

## Usage
```sh
# Format and lint a single file
control_freak src/main.rs

# Format and lint an entire directory (respects .gitignore)
control_freak src/

# Check without modifying any files
control_freak --dry-run src/
```

Exits with code 1 if any lint warnings are found.

## Formatters
Formatters automatically rewrite your code.

### Flip negated `if`
Rewrites `if !condition { A } else { B }` into `if condition { B } else { A }`.
Also flips `if a != b` into `if a == b` with swapped branches.

Does not flip `if`/`else if` chains or `if` without an `else` branch.

Exception: `!x.is_empty()` is considered idiomatic and is not flipped.

## Lints
Lints report warnings but do not modify code.

### Negated boolean names
Warns when a `bool` field, function/method return type, parameter, constant, or static
uses a negation word in its name. Prefer positive names (e.g. `is_enabled` over `is_disabled`).

Detected negation words:
`cannot`, `cant`, `disable`, `disabled`, `disallow`, `disallowed`,
`disconnect`, `disconnected`, `dont`, `illegal`, `never`, `no`, `not`,
`unavailable`, `unreachable`, `unusable`, `without`.

## Suppressing control-freak
Add a `// NOLINT` comment to suppress both formatters and lints
on a given line. It can go on the line itself, or on the line above:

```rust
// NOLINT
if !value {
    // ...
} else {
    // ...
}

struct Config {
    is_disabled: bool, // NOLINT
}
```
