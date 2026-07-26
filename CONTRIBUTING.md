# Contributing to zj-agents

Thanks for helping out. This is a Rust virtual workspace (`resolver = "2"`,
edition 2021, toolchain pinned in `rust-toolchain.toml`) that builds two
stock-Zellij WASM plugins plus a pure core library.

## Development setup

```bash
git clone https://github.com/kaankoken/zj-agents
cd zj-agents
rustup target add wasm32-wasip1
cargo test --workspace
cargo build --workspace --release --target wasm32-wasip1
```

Install into your Zellij plugin dir (Nushell):

```bash
nu scripts/install.nu
```

## Before you open a PR

Run the same gates CI runs:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --target wasm32-wasip1
```

## House rules

- Prefer pure logic in `zj-agents-core` with unit tests; keep engine/sidebar adapters thin.
- Do not invent agent detection patterns without real, redacted fixtures under
  `crates/zj-agents-core/tests/fixtures/`.
- Do not add OpenCode/Antigravity (or other agents) without reviewed fixtures.
- No daemon, filesystem watcher, ACK/bye protocol, or extra config keys beyond
  `manifest_dir` / `notify` / `notify_command`.
- Minimal comments — explanatory only when non-obvious.
- Update `CHANGELOG.md` under `[Unreleased]` for user-facing changes.

## Workflow

Branch → PR against `master` → status checks green (`verify` on ubuntu + macOS) → merge.

`master` is protected via a GitHub ruleset (see `.github/rulesets/branch-master.json`):
no direct pushes, linear history, squash/rebase only, required CI contexts
`verify (ubuntu-latest)` and `verify (macos-14)`. Release tags `v*` are protected
against deletion/rewrite (`.github/rulesets/tag-release.json`).

Ruleset JSON is the source of truth for humans; apply or update on GitHub with
(requires admin on the repo, after the first successful `verify` run so check
names exist):

```bash
# create (once)
gh api --method POST repos/kaankoken/zj-agents/rulesets \
  --input .github/rulesets/branch-master.json
gh api --method POST repos/kaankoken/zj-agents/rulesets \
  --input .github/rulesets/tag-release.json

# list / update later: gh api repos/kaankoken/zj-agents/rulesets
```

## Releasing

Maintainers only:

1. Move `[Unreleased]` notes into a versioned section in `CHANGELOG.md`.
2. Tag `vX.Y.Z` and push the tag.
3. GitHub Actions builds both `.wasm` artifacts and attaches them to the release.
