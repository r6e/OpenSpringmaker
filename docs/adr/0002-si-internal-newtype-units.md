# ADR 0002: SI-internal newtype units

**Status:** Accepted

## Context

`springcore` must work with many physical quantities — lengths, forces, stresses,
spring rates, masses, frequencies. Mixing units silently (passing a force in newtons
where a force in pounds is expected, or passing a stress where a length is expected)
is a well-documented source of engineering calculation errors.

Two main approaches exist in the Rust ecosystem:

1. **`uom` (units of measure)** — a comprehensive compile-time dimensional analysis
   library. Quantities carry their dimension and unit in their type; arithmetic
   propagates dimensions automatically.
2. **Newtype wrappers** — define one `struct` per physical dimension (e.g.
   `struct Length(f64)`), storing the value in a canonical unit (SI base unit).
   Cross-dimension confusion becomes a compile error; within-dimension unit confusion
   is prevented by convention (always SI internally, convert at boundaries).

## Decision

Use newtype wrappers over SI base units for all physical quantities in `springcore`.
The canonical internal unit for each dimension is:

- Length: metres (m)
- Force: newtons (N)
- Stress / pressure: pascals (Pa)
- Spring rate: newtons per metre (N/m)
- Mass: kilograms (kg)
- Frequency: hertz (Hz)
- Dimensionless ratios: plain `f64`

Unit conversion happens only at three boundary types: input parsing, UI display, and
TOML file persistence.

Reject `uom` for the initial implementation.

## Consequences

**Benefits:**
- Zero additional dependencies for unit safety.
- Arithmetic on same-dimension quantities is direct; no ceremony for common operations.
- Compile-time prevention of dimension confusion (passing `Force` where `Length` is
  expected is a type error).
- Easier to read and maintain for contributors unfamiliar with `uom`'s trait machinery.

**Trade-offs:**
- Within-dimension unit confusion (e.g. storing millimetres in a `Length` field that
  expects metres) is a runtime bug, not a compile error. Discipline and boundary
  conversion tests are the mitigation.
- `uom` supports automatic dimension propagation (multiplying `Force` by `Length` yields
  `Torque`); newtypes do not. Derived quantity arithmetic must be written explicitly.
- If the codebase grows to require automatic dimension tracking, migrating from newtypes
  to `uom` would be a breaking internal refactor.
