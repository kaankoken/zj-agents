## What & why

<!-- Short description of the change and the motivation. -->

## Checklist

- [ ] `cargo fmt --all --check` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` passes
- [ ] `cargo test --workspace` passes
- [ ] `cargo build --workspace --release --target wasm32-wasip1` produces both `.wasm` artifacts
- [ ] `CHANGELOG.md` updated under `[Unreleased]` (if user-facing)
- [ ] New agent manifests include reviewed redacted fixtures (if adding detection)
