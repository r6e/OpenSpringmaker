# ADR 0004: Two-crate workspace

**Status:** Accepted

## Context

The project requires both an engineering calculation library and a desktop GUI. These
two concerns have different dependency profiles, test strategies, and stability
contracts:

- Calculation logic depends only on arithmetic, material data, and serialization. It
  should be testable in a headless environment, have deterministic output, and be
  independently publishable.
- GUI logic depends on a windowing/rendering framework (iced), graphics (wgpu, plotters),
  and OS integration. It changes at a different cadence and has no stable public API
  contract.

Placing both in a single crate would mean the calculation library transitively depends
on GUI crates, making headless testing harder, slowing compile times, and coupling the
library's versioning to UI framework upgrades.

## Decision

Structure the project as a Cargo workspace with two members:

- **`springcore`** (`springcore/`) — the engineering calculation library. No GUI
  dependencies. Exposes a stable, documented public API. All formula logic, unit types,
  material data, solver scenarios, optimization, fatigue, and persistence live here.

- **`springmaker`** (`springmaker/`) — the iced desktop application. Depends on
  `springcore` through its public API only. UI state, widgets, and plotting live here.

`springmaker` must not reach into `springcore` internals; only `pub` items from
`springcore` are permitted. `springcore` must not depend on `springmaker`.

## Consequences

**Benefits:**
- `springcore` can be tested in a headless CI environment with no native window system.
- `springcore` is independently publishable to crates.io when ready.
- GUI framework upgrades do not affect `springcore`'s compilation or tests.
- The public API boundary is enforced by the Rust module system and Cargo dependency
  direction.

**Trade-offs:**
- Two `Cargo.toml` files to maintain.
- Cross-crate refactors require updating both the API definition in `springcore` and the
  call sites in `springmaker`.
