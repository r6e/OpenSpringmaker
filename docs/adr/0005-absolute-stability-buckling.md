# ADR 0005: Absolute stability buckling criterion

**Status:** Accepted

## Context

Helical compression springs can buckle under load if the free length exceeds a critical
ratio relative to the mean coil diameter. Two criteria appear in standard references:

1. **Absolute stability criterion** (Shigley Eq. 10-10) — a spring is absolutely stable
   (will not buckle under any end condition) when:

   ```
   Lf / D < α_cr
   ```

   where `α_cr` is a dimensionless critical ratio derived from the spring's elastic
   constants (approximately 2.63 for steel with fixed-parallel ends). This criterion is
   end-condition-independent and conservative.

2. **Deflection-ratio curve** (Shigley Eq. 10-11 and associated Figure 10-13) — a spring
   is stable at a specific deflection `δ` when the operating point falls below a curve
   that depends on the end condition parameter `α` and the deflection ratio `δ/Lf`. This
   criterion is more precise but requires an edition-dependent constant (`α_cr` varies
   between Shigley editions) and the curve is tabulated, not closed-form.

## Decision

Implement the absolute stability criterion (Shigley Eq. 10-10) for v1. Defer the
deflection-ratio curve (Eq. 10-11) to a later cycle.

The critical ratio `α_cr ≈ 2.63` applies to steel springs with fixed-parallel end
conditions and is cited from Shigley's Mechanical Engineering Design, Chapter 10.

## Consequences

**Benefits:**
- The absolute criterion has a simple closed-form expression and no edition-dependent
  constants — the same formula works across Shigley editions.
- It is conservative: a spring that passes the absolute criterion is safe under all end
  conditions and at all deflection levels.
- It is unambiguous to test with golden fixtures from published worked examples.

**Trade-offs:**
- The criterion is conservative: springs that would be stable under a specific end
  condition and deflection ratio may be flagged as potentially unstable.
- The deflection-ratio curve gives a less conservative (and more accurate) stability
  check for the actual operating end conditions, which is deferred to a later cycle.
- Users designing springs near the stability boundary may get a conservative warning
  that a more precise analysis would not produce.
