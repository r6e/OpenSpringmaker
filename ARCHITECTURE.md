# Architecture

## Two-crate workspace

The workspace contains two crates with a strict dependency direction:

- **`springcore`** — the engineering calculation library. No GUI dependencies. Exports a
  stable public API. Contains units, materials data, spring formulas, solver scenarios,
  the min-weight optimizer, fatigue analysis, buckling stability, and TOML persistence.

- **`springmaker`** — the iced desktop application. Depends on `springcore` through its
  public API only. Handles input, display, and plotting. GUI state and business logic are
  strictly separated.

See [`docs/adr/0004-two-crate-workspace.md`](docs/adr/0004-two-crate-workspace.md) for
the rationale.

## SI-internal newtype units

All quantities inside `springcore` are stored in canonical SI base units (metres,
newtons, pascals, kilograms, seconds). Each physical dimension is represented by a
newtype struct (e.g. `Length(f64)`, `Force(f64)`, `Stress(f64)`). Unit conversion
happens only at crate boundaries — input parsing, UI display, and TOML persistence.

Using newtypes rather than a full unit-algebra crate keeps the dependency tree light,
avoids compile-time complexity, and gives compile-time dimensional safety for the most
common mistake (passing a force where a length is expected). The trade-off vs. `uom` is
documented in [`docs/adr/0002-si-internal-newtype-units.md`](docs/adr/0002-si-internal-newtype-units.md).

## Scenario-based solver

Spring design problems have four unknowns (wire diameter, mean coil diameter, active
coils, initial length) and typically one design constraint. `springcore` implements four
closed-form solution scenarios corresponding to the four possible "one unknown given the
others" combinations, plus a numeric root-finding kernel for cases without a closed-form
inverse, and a min-weight optimizer that searches over feasible combinations.

This approach was chosen over a generic constraint solver for determinism and
testability. See [`docs/adr/0001-scenario-based-solver.md`](docs/adr/0001-scenario-based-solver.md).

## Unit-native material coefficients

Minimum tensile strength coefficients for spring wire (e.g. Shigley Table 10-4) are
defined in their source units (ksi and inches). When evaluating minimum tensile
strength, the coefficient equation is evaluated in those native units and only the
scalar result is converted to SI. The coefficients themselves are never converted.

This avoids the error introduced by transforming power-law coefficients into different
unit systems. See [`docs/adr/0003-unit-native-mts-coefficients.md`](docs/adr/0003-unit-native-mts-coefficients.md).

## Materials data model

Material records are stored in `springcore/data/materials.toml`. Each record carries:
minimum tensile strength coefficients with their unit system, shear modulus, density,
endurance limits (when available with citations), and ASTM specification reference.
Materials without fatigue data report that fatigue data is unavailable rather than
borrowing constants from another family.

## ADR index

| # | Title |
|---|-------|
| [0001](docs/adr/0001-scenario-based-solver.md) | Scenario-based solver |
| [0002](docs/adr/0002-si-internal-newtype-units.md) | SI-internal newtype units |
| [0003](docs/adr/0003-unit-native-mts-coefficients.md) | Unit-native MTS coefficients |
| [0004](docs/adr/0004-two-crate-workspace.md) | Two-crate workspace |
| [0005](docs/adr/0005-absolute-stability-buckling.md) | Absolute stability buckling criterion |
| [0006](docs/adr/0006-allowable-stress-simplification.md) | Allowable stress simplification |
| [0007](docs/adr/0007-accept-transitive-lru-advisory.md) | Accept transitive `lru` advisory RUSTSEC-2026-0002 |
