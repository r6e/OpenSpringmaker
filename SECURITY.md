# Security Policy

## Scope

OpenSpringmaker is a user-local desktop calculator and Rust calculation library for helical spring design. The current scope is the first roadmap sub-project: a cylindrical compression round-wire solver, an iced GUI, curated material data, and single-design TOML save/load.

Security-relevant areas include:

- Loading and validating human-readable TOML design files.
- Loading the curated material data file with cited numeric values.
- Handling numeric input from the GUI and persistence layer without panics, infinities, NaN propagation, unbounded iteration, or excessive allocation.
- Dependency and GitHub Actions supply-chain integrity.
- Local file reads and writes for design persistence.

OpenSpringmaker does not currently run as a service, accept network input, execute user-provided scripts, or process binary archive formats.

## Reporting a vulnerability

If you discover a security vulnerability, report it privately:

1. Do not open a public GitHub issue.
2. Use GitHub private vulnerability reporting if enabled for the repository, or contact the maintainers directly.
3. Include a description of the vulnerability, steps to reproduce, affected versions or commits, and potential impact.

We aim to acknowledge reports within 48 hours and provide a fix or mitigation within 7 days for critical issues.

## Supported versions

Only the latest release on `main` is actively supported with security fixes.

## Security practices

- `unsafe` code is not part of the planned workspace. If it is ever proposed, it needs explicit design review and justification.
- Dependencies are scanned with Dependabot and `cargo audit` once the Cargo workspace and lockfile exist.
- GitHub Actions use pinned action SHAs where practical.
- CI runs formatting, clippy, tests, build, audit, CodeQL, and cargo-deny checks as the corresponding project files become available.
- TOML parsing must use structured parsers and typed validation, not ad hoc string parsing.
- File persistence must reject path traversal only if OpenSpringmaker later introduces project directories or import/export features that derive paths from file contents. The current single-file open/save flow uses user-selected paths.
- User-facing errors should be understandable, while debug details remain in logs if logging is added.

## Threat model

OpenSpringmaker currently runs with the privileges of the local user who launches it. It reads material data shipped with the application and design files the user chooses to open, then writes design files to paths the user chooses.

The current threat model includes:

- Malformed or adversarial TOML design files.
- Malformed material data in development builds or downstream packages.
- Numeric denial-of-service inputs such as NaN, infinity, impossible geometry, bad root-finder brackets, non-converging equations, and infeasible optimization constraints.
- Dependency compromise or vulnerable transitive crates.
- CI workflow changes that broaden permissions or run untrusted input in shell commands.

The current threat model does not include:

- A daemon or server ingesting files autonomously.
- Multi-tenant execution where one user's design file is processed in another user's filesystem context.
- Network-sourced material registries or update feeds.
- CAD/DXF export, 3D visualization asset pipelines, customer databases, or project databases. These are later roadmap items and must expand this document when implemented.

## Hardening notes

- **Design files are untrusted input.** Validate scenario names, units, dimensions, material references, and enum values before constructing a design. Reject inconsistent inputs with typed errors instead of panicking.
- **Numeric kernels need explicit bounds.** Root finding and optimization must use iteration caps, tolerances, bracket validation, and typed non-convergence errors.
- **Floating-point values need validation.** Reject NaN and infinity at input boundaries. Guard formulas against division by zero, negative physical quantities, and invalid spring index ranges.
- **Material coefficients are correctness-sensitive.** Minimum tensile strength coefficients are evaluated in native units and only the scalar result is converted to SI. Treat coefficient conversion bugs as safety-relevant calculation defects.
- **Fatigue data must not be borrowed across materials.** If a material lacks cited endurance data, report that fatigue data is unavailable.
- **No commercial-product references.** Persisted files must describe functionality directly and cite engineering literature only.

## GitHub Apps

No installation-scoped GitHub Apps are currently required by this repository's workflows.

If a future workflow adds a GitHub App or other long-lived automation credential, update this section in the same PR with:

- The App name and repository installation scope.
- The workflow and step that mint or use the token.
- Required repository permissions.
- Required repository variables and secrets.
- Blast radius if the credential is compromised.
- Rotation procedure and rotation triggers.
