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

## Configuration

`control_freak` looks for `control_freak.toml` in the supplied file or directory,
then in each parent directory. The closest file is used. All settings are optional:

```toml
[lints.max_mut_bindings]
max_bindings = 3

[lints.max_struct_fields]
max_fields = 8

[lints.max_visible_bindings]
max_bindings = 10

[lints.negated_bool_names]
enabled = false

[lints.use_destructuring]
enabled = true
min_fields = 4
```

All lints are enabled by default. See each lint below for its settings.

## Formatters
Formatters automatically rewrite your code.

### Flip negated `if`
Rewrites `if !condition { A } else { B }` into `if condition { B } else { A }`.
Also flips `if a != b` into `if a == b` with swapped branches.

Does not flip `if`/`else if` chains or `if` without an `else` branch.

Exception: `!x.is_empty()` is considered idiomatic and is not flipped.

## Lints
Lints report warnings but do not modify code.

### Max mut bindings
Like [max visible bindings](#max-visible-bindings), but only counts names declared with `mut` (e.g. `let mut x`, `mut x: i32`, and `mut self`). Defaults to 4. `&mut` references and `ref mut` bindings are not counted, and a `mut` binding shadowed by an immutable one no longer counts. Lots of mutable state makes a function hard to follow.

### Max struct fields
Warns when a struct has more than `max_fields` fields (12 by default). Big structs are hard to understand; consider grouping related fields into smaller structs.

### Max visible bindings
Warns when more than `max_bindings` names (16 by default) are visible at the same point in a function. This counts parameters (including `self`), `let` bindings, closure parameters, and bindings from `if let`, `while let`, `for`, and `match` arms. A shadowed name counts once. Too many live variables are hard to keep track of; consider splitting the function.

### Negated boolean names
Warns when a `bool` field, function/method return type, parameter, constant, or static
uses a negation word in its name. Prefer positive names (e.g. `is_enabled` over `is_disabled`).

Detected negation words:
`cannot`, `cant`, `disable`, `disabled`, `disallow`, `disallowed`,
`disconnect`, `disconnected`, `dont`, `illegal`, `never`, `no`, `not`,
`unavailable`, `unreachable`, `unusable`, `without`.

### Use destructuring
Warns when a method accesses every field of `self` individually for a struct with
at least `min_fields` fields (three by default). Destructuring makes the compiler point out methods that
need updating when a field is added.

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
