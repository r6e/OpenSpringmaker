# Contributing

Contributions are welcome. OpenSpringmaker is built around traceable engineering formulas, strict TDD, and a small Rust workspace split between the solver library and desktop calculator.

## Getting started

```sh
git clone https://github.com/r6e/openspringmaker.git
cd openspringmaker
cargo build --workspace
cargo test --workspace --all-features
```

The initial scaffold may land before every command is available. Once the Cargo workspace exists, run all checks from the repository root.

## Development workflow

1. Create a focused branch from `main`.
2. Write a failing test that captures the behavior or formula being added.
3. Run the relevant test and verify it fails for the expected reason.
4. Implement the minimal code to pass the test.
5. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace --all-features`.
6. Commit with a conventional commit message.
7. Open a PR against `main`.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```plaintext
feat(core): add SI length quantity
fix(core): reject out-of-range material diameters
test(core): add Wahl stress golden fixture
docs: document unit-native material coefficients
ci: add cargo audit workflow
```

Use scopes that match the planned workspace when they fit:

- `core` for the `springcore` calculation library.
- `gui` for the `springmaker` iced application.
- `docs` for documentation-only changes.
- `ci` for GitHub Actions and automation.

## Code style

- Run `cargo fmt --all -- --check` before opening a PR.
- Run `cargo clippy --workspace --all-targets --all-features -- -D warnings` and fix warnings instead of suppressing them without justification.
- Keep `springcore` free of GUI dependencies. The `springmaker` crate must use `springcore` through its public API.
- Every public symbol needs a doc comment once it becomes part of the public API.
- No `unsafe` code in the workspace unless a future design document explicitly accepts it.
- In comments that reference other functions or logic, cite the symbol name rather than a literal line number. Line numbers rot silently; symbol names survive refactors.

## Engineering citations

Every formula and engineering constant must carry an inline source citation at its definition site. Include the source and equation, table, or section number when available.

Preferred citation hierarchy for compression round-wire springs:

- EN 13906-1 for cylindrical helical compression springs.
- SMI Handbook of Spring Design for spring-engineering practice.
- Shigley's Mechanical Engineering Design, Chapter 10, for formulas and worked examples.
- Wahl, Mechanical Springs, for spring stress correction factors.
- ASTM specifications for material records.
- Zimmerli endurance data only for materials where the cited source applies.

Do not add references to any commercial product or vendor in persisted files, including code, comments, docs, data files, commit messages, and tests.

## Units and materials

`springcore` stores quantities internally in canonical SI units. Convert at the UI, persistence, or data boundary only.

Material minimum tensile strength coefficients are unit-native. Evaluate coefficients in their documented native diameter and strength units, then convert only the scalar result to SI. Do not convert coefficients themselves.

Every numeric material value in `springcore/data/materials.toml` needs a citation field. If fatigue data is absent for a material, report that fatigue data is unavailable instead of borrowing constants from a different material family.

## Testing

- TDD is required: write the test first, watch it fail, then implement.
- Unit tests live alongside code in `#[cfg(test)]` modules when the behavior is local.
- Integration tests live in each crate's `tests/` directory.
- Golden fixtures from published worked examples are the accuracy contract for `springcore`.
- Property tests should cover scenario round-trips and unit conversion invariants.
- Solver tests should cover convergence, bad brackets, non-convergence, infeasible optimization, and active constraints.
- Target 80%+ coverage for `springcore` once coverage tooling is enabled.

Run the full workspace test suite before pushing:

```sh
cargo test --workspace --all-features
```

## Pull requests

- Keep PRs focused on one logical change.
- Update docs in the same PR that changes behavior, public APIs, formulas, material data, or contributor workflow.
- Include tests for touched behavior.
- Fill out the PR template's engineering considerations when changing formulas, units, materials, fatigue/buckling logic, optimization, persistence, file I/O, GUI input validation, or dependencies.
- PRs must pass CI before merge.
- Squash merge to `main`.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) and the ADRs under `docs/adr/` once they are scaffolded. The planned workspace has two crates:

- `springcore`: pure Rust calculation library for units, materials, formulas, solver scenarios, optimization, fatigue, and persistence.
- `springmaker`: iced desktop calculator and plotting UI that depends on `springcore`.

Follow existing module boundaries when adding code. If a change crosses a boundary, explain the trade-off in the PR description.

## Reporting issues

Open a GitHub issue with:

- What you expected to happen.
- What actually happened.
- Steps to reproduce.
- Spring inputs, selected material, unit system, scenario, or TOML design file when relevant.
- Engineering reference or worked example when reporting a calculation mismatch.
