# OpenSpringmaker

A desktop calculator for helical compression spring design. Enter wire diameter,
spring index, active coils, and material; OpenSpringmaker computes stress, deflection,
natural frequency, buckling stability, and fatigue life, and solves for unknown
dimensions under four closed-form scenario constraints.

The workspace is split into two crates:

- **`springcore`** — pure Rust engineering calculation library (units, materials,
  formulas, solver scenarios, optimization, fatigue, persistence).
- **`springmaker`** — iced desktop GUI that depends on `springcore` through its public API.

## Build

Requires a stable Rust toolchain (MSRV 1.80).

```sh
cargo build --workspace
```

## Run

```sh
cargo run -p springmaker
```

## Test

```sh
cargo test --workspace
```

## License

Licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this project shall be dual-licensed as above, without any
additional terms or conditions.
