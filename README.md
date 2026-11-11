# siquant

SI units, prefixes and quantities with uncertainty, defined by YAML resource files. A second-generation, much simpler Rust successor to the Raku `Physics::Unit` and `Physics::Measure` modules.

Status: Stage 0, the scaffold. Nothing is implemented yet. The staged plan is in [docs/plan.md](docs/plan.md).

## Development

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Licence

MIT. See [LICENSE](LICENSE).
