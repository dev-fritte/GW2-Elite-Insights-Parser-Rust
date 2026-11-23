# Rust workspace

Experimental Rust core for the Elite Insights parser. The C# implementation stays the reference; every crate is
checked against its output (see `docs/RUST_MIGRATION_PLAN.md` and `docs/phase1/ORACLE.md`).

| Crate | Purpose | Status |
| --- | --- | --- |
| `evtc-ingest` | Zero-copy reader for EVTC logs | skeleton (phase 1.1) |

Checks (also run in `.github/workflows/rust-check.yml`):

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

The toolchain is pinned in `rust-toolchain.toml`. `unsafe` is forbidden workspace-wide for now; the plan allows it
only in the ingest crate and with a justification, which would be added per module when needed.

Licensed under the MIT License of the repository (`LICENSE`). Ported code references its C# origin in the doc comment.
