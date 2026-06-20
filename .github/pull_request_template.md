## Summary

<!-- 1-3 sentences on what changed and why. -->

## Linked issue

Closes #

## Test plan

<!--
OpenSpringmaker follows TDD. List the tests added or modified before the implementation changed.
-->

- [ ] New tests added or updated (unit / integration / property / golden fixture)
- [ ] `cargo test --workspace --all-features` passes locally

## Pre-flight checklist

- [ ] PR title is a Conventional Commit (for example, `feat(core): add unit quantities`)
- [ ] `cargo fmt --all -- --check` is clean
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings` is clean (mirrors CI)
- [ ] Documentation updated if behavior, public API, or contributor workflow changed
- [ ] New formulas/constants include an inline source citation with equation, table, or section
- [ ] New material data includes citations and preserves native-unit strength coefficients
- [ ] No commercial-product or vendor references were added to persisted files

## Engineering considerations

<!--
Required for changes touching: formulas, unit conversions, material data, fatigue/buckling logic,
optimization constraints, persistence, file I/O, GUI input validation, or dependencies.
Describe correctness evidence, edge cases, input validation, and security impact.
-->

## Notes for reviewers

<!-- Optional. Trade-offs, design decisions worth flagging, follow-ups deferred. -->
