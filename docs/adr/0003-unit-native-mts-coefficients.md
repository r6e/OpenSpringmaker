# ADR 0003: Unit-native MTS coefficients

**Status:** Accepted

## Context

Minimum tensile strength (MTS) for spring wire is given by a power-law equation of the
form:

```
Sut = A / d^m
```

where `d` is the wire diameter and `A` and `m` are material-specific coefficients.

Shigley's Mechanical Engineering Design, Table 10-4, tabulates these coefficients with
wire diameter in millimetres and strength in megapascals (or in inches and ksi for the
US customary version). The coefficients are not dimensionless; they carry implicit units
that match the table's unit system.

A naive implementation might attempt to convert `A` into SI units (Pa·m^m) before
storing it. This introduces an error: the power-law coefficient `A` transforms as
`A_SI = A_native * (unit_length_factor)^m * unit_stress_factor`, which requires exact
knowledge of `m` at conversion time and is easy to get wrong when `m` varies per
material.

## Decision

Store MTS coefficients in their source (native) units as documented in the reference
table (e.g. `A` in MPa·mm^m, `d` range in mm). When computing minimum tensile
strength:

1. Convert the input wire diameter to the coefficient's native diameter unit.
2. Evaluate the power-law equation entirely in the coefficient's native unit system.
3. Convert only the scalar result (the MTS value) to SI (pascals).

Never convert the coefficients themselves. Each material record in
`springcore/data/materials.toml` records the native units of its coefficients alongside
the coefficient values and the source citation.

## Consequences

**Benefits:**
- Coefficient values in source files match published tables exactly, making citation
  verification straightforward.
- No risk of coefficient mis-transformation; only a single scalar value (the result) is
  converted.
- Shigley Table 10-4 notes unit-dependence explicitly; this design honours that note
  rather than hiding it.

**Trade-offs:**
- Material records must explicitly store the native unit system for each coefficient
  set; the schema is slightly more complex than a pure-SI store.
- Boundary conversion (diameter to native unit, result to SI) must be present and
  tested at every MTS evaluation site.
