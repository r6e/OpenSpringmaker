# ADR 0006: Allowable stress simplification

**Status:** Accepted

## Context

Allowable torsional and bending stress for spring wire depends on:

- The wire material and its processing (pre-set vs. not pre-set).
- The wire diameter (smaller diameters tolerate higher stress as a fraction of tensile
  strength, due to the surface-to-volume ratio and the nature of the cold-drawing
  process).
- The application type (static vs. fatigue loading).

Precise design-stress curves for common wire materials are published by the Spring
Manufacturers Institute (SMI) and appear in references such as the SMI Handbook of
Spring Design. These curves express allowable stress as a percentage of minimum tensile
strength (MTS), as a function of wire diameter. They are tabulated or plotted, not
simple closed-form expressions.

For v1 `springcore`, implementing full diameter-dependent design-stress curves requires:
- Digitising or encoding the full SMI tables per material.
- Interpolation logic across the diameter range.
- Separate curves for pre-set and not-pre-set conditions.

This is a substantial scope addition for a first release.

## Decision

For v1, represent allowable stress as three scalar percentages of MTS per material
record:

- `allowable_pct_torsion` — allowable torsional stress as % of MTS (static, not pre-set).
- `allowable_pct_bending` — allowable bending stress as % of MTS (for torsion springs).
- `allowable_pct_set` — allowable torsional stress as % of MTS (pre-set / set-removed).

These scalars are single-point approximations of the SMI diameter-dependent
design-stress curves, chosen conservatively at the midpoint of the applicable diameter
range for each material. Each value is documented in the material record with its source
and the diameter range it approximates.

These percentages gate two outputs:

1. **Status warnings** — a spring whose computed stress exceeds `allowable_pct_torsion`
   of its MTS receives a stress-overage warning in the design report.
2. **Min-weight feasibility boundary** — the optimizer treats designs exceeding the
   allowable percentage as infeasible and excludes them from the weight-minimizing
   search.

Diameter-dependent design-stress curves are a known planned improvement for a later
cycle.

## Consequences

**Benefits:**
- Simple to implement, test, and understand.
- Avoids encoding and interpolating large tabular datasets in v1.
- Scalar values are easy to override per-material in `materials.toml` as understanding
  of the design domain improves.

**Trade-offs:**
- The simplification is a known deviation from the full SMI curves. Springs near the
  allowable-stress boundary may receive incorrect status (either over- or under-flagged)
  depending on their actual wire diameter relative to the approximation point.
- The Min Weight feasibility boundary uses the same simplified threshold, so the
  optimizer may exclude designs that would be feasible under the full diameter-dependent
  curve, or vice versa.
- This simplification must be documented in user-facing output so that engineers
  designing near stress limits are aware of it.
