# siquant

The design and staged plan are in `docs/plan.md`; follow it and update it when a decision changes.

- Every `.rs` file starts with `// Copyright (c) 2026 Stephen Roe`, then `//!` crate or module docs.
- Before finishing a change run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.
- Unit, prefix and quantity-type definitions live in `resources/*.yaml`, embedded with `include_str!`. Never generate prefix x unit tables; look prefixes up at parse time.
- Cell text is deliberately strict: SI units only, `.` between terms, `^` exponents, no spaces or `/`. See the plan before widening it.
- To check a Raku reference value, run the Raku suite with `prove6`, not `raku`.
- Do not hard-wrap Markdown.
