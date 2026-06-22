# Review Checklist

Lenses the adversarial review panel applies before a change is pushed, in
addition to "is the logic correct?" These exist because automated review
(Copilot) repeatedly surfaced real issues in dimensions ad-hoc panels
under-weighted. Apply the lenses relevant to the change; cite the ones that
don't apply rather than skipping silently.

## 1. Cross-state matrix (stateful features, esp. GUI)

For each action/handler, enumerate behavior against the *pre-existing state*, not
just the happy path:

- An editing session / modal is open.
- The target is the item **selected elsewhere** (e.g. the calculator's chosen
  material) — does deleting or **renaming** it leave a dangling reference?
- A prior error or success message is showing.
- It is the **last** item (empty-collection fallback).

> Repeat traps from PR #8: delete-the-edited-material, delete/rename-the-selected
> material (→ stale `form.material` → `MaterialNotFound`).

## 2. Invariant symmetry

If one path sets X and clears Y, the **mirror** path must too. Centralize
mutually-exclusive UI state in one helper (e.g. `set_mat_error` clears status;
`set_mat_status` clears error). Prefer **invariant tests** that assert the
guarantee after *every* `update` over hoping each handler remembered — see
`springmaker/src/app.rs::editor_message_sequence_preserves_invariants` (INV1:
the selected material always exists; INV2: error and status never both set).
This single technique catches most of the class.

## 3. Test hermeticity

Does any test touch the filesystem, network, env, clock, or global state, or
assume ordering/indices that depend on external state? Build fixtures
explicitly — never a `Default` that performs IO (use `App::from_store(...)`, not
`App::default()` which loads the OS overlay). No `user_materials()[0]`
index assumptions; derive identities deterministically.

## 4. User-facing text

Every string a user can see — error messages, headings, labels — must be
accurate in **all** states and use domain vocabulary, not internal identifiers.

> PR #8: "Design status" heading shown for startup load-warnings; `youngs_modulus`
> leaking into a GUI error instead of "Young's modulus (GPa)".

## 5. Public-API contract

For each new `pub fn`: state the precondition and enforce it — **fail fast at
runtime** for library code (we're a library; `SavedDesign::solve_with_material`
returns `SpringError` on a material/name mismatch), or document an intentional
relaxation. Can a caller pass inconsistent arguments?

## 6. Verify findings empirically — don't cargo-cult

Give every reviewer (human or tool) serious weight, but **validate each finding
against primary sources before acting**. A green multi-platform build refutes a
"this won't compile" claim; a passing oracle test refutes a "wrong constant"
claim. Applying a fix for a non-bug can *introduce* one (PR #8: a suggested
`&material.name` change would have broken `String`/`&String` comparison). Record
the false-positive determination instead of changing working, verified code.

## Pre-push gates (run locally, mirroring CI)

Beyond `cargo test` + `cargo clippy -D warnings` + `cargo fmt --check`:

- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features` (the
  Lint/Documentation job — catches `rustdoc::private_intra_doc_links` etc.)
- `typos` over the whole tree (not just changed files)
- `cargo mutants --in-diff <pr.diff> --package springcore` — 0 missed / 0 timeouts

## Fix the class, not the instance

When a finding is found, the fix brief asks "where else does this class occur?"
(delete-stale-selection → also rename-stale-selection). Loop until the class is
dry, rather than one round per instance.
