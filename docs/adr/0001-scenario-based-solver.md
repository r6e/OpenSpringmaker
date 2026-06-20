# ADR 0001: Scenario-based solver

**Status:** Accepted

## Context

Helical compression spring design involves four primary geometric unknowns: wire
diameter `d`, mean coil diameter `D`, number of active coils `Na`, and free length `Lf`.
A typical design problem holds three of these fixed (or related by a constraint) and
solves for the fourth. The solver must also support a min-weight optimization that
searches over feasible combinations subject to stress, deflection, and stability limits.

Two broad implementation approaches exist:

1. **Generic constraint solver** — model the problem as a system of equations and let a
   general-purpose solver (e.g. Newton–Raphson, interior-point) find solutions.
2. **Scenario-based solver** — enumerate the four "one unknown, three known" scenarios
   explicitly, implement a closed-form inverse for each where it exists, and fall back to
   a one-dimensional numeric root-finding kernel only when no closed form is available.

## Decision

Implement four closed-form solution scenarios corresponding to the four possible "solve
for one unknown given the others" combinations. For scenarios without a closed-form
inverse, use a bracketed numeric kernel (bisection or Brent's method) on the
one-dimensional residual. Layer a min-weight optimizer on top that iterates over
feasible scenario solutions.

Reject the generic constraint solver approach for this design domain.

## Consequences

**Benefits:**
- Each scenario has an isolated, deterministic code path that can be verified against
  published worked examples (Shigley Chapter 10, EN 13906-1 annexes).
- Unit tests can be written per-scenario with exact golden fixtures, giving high
  confidence in formula correctness.
- The numeric kernel is one-dimensional, making convergence guarantees straightforward
  (bracketed methods cannot fail to converge when a root exists in the bracket).
- No dependency on a general-purpose optimization or constraint-satisfaction library.

**Trade-offs:**
- New constraint types (e.g. pitch-constrained design) require a new scenario rather
  than being expressed as an additional equation in a generic solver.
- The scenario enumeration is explicit, not data-driven; a scenario-combinatorial
  explosion is possible if the constraint set grows significantly beyond v1.
