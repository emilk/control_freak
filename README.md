# Control Freak
An opinionated Rust source code formatter and linter.

## Formatters

### `flip_negated_if`
```rust
// Before:
if !is_ready {
    wait();
} else {
    run();
}

// After:
if is_ready {
    run();
} else {
    wait();
}
```

### `vertical_spacing`
```rust
// Before:
struct Point {
    x: f32,
    /// Up
    y: f32,
}
impl Point {}

// After:
struct Point {
    x: f32,

    /// Up
    y: f32,
}

impl Point {}
```

## Lints

### `dyn_any_params`
```rust
fn handle(event: &dyn Any) {} // ⚠️ parameter of type `&dyn Any`; …
```

### `max_mut_bindings`
```rust
// With `max_bindings = 2`:
fn update(&mut self, out: &mut Vec<u8>) {
    let mut count = 0; // ⚠️ 3 mutable bindings are visible in `update`
}
```

### `max_struct_fields`
```rust
// With `max_fields = 2`:
struct Point { x: f32, y: f32, z: f32 } // ⚠️ struct `Point` has 3 fields
```

### `max_visible_bindings`
```rust
// With `max_bindings = 2`:
fn area(width: f32, height: f32) -> f32 {
    let area = width * height; // ⚠️ 3 bindings are visible in `area`
    area
}
```

### `negated_bool_names`
```rust
struct Config {
    is_disabled: bool, // ⚠️ use a positive name, like `is_enabled`
}
```

### `use_destructuring`
```rust
impl Point {
    fn sum(&self) -> f32 {
        self.x + self.y + self.z // ⚠️ consider `let Point { x, y, z } = self;`
    }
}
```

### `zip_chain_methods`
```rust
let pairs = a.iter().zip(b); // ⚠️ prefer `std::iter::zip(a, b)` …
```
