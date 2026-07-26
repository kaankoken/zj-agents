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
- Prefer [Conventional Commits](https://www.conventionalcommits.org/)
  (`feat:`, `fix:`, `ci:`, `chore:`, …). The changelog is generated from them
  with [git-cliff](https://git-cliff.org/) (`cliff.toml`, same setup as tessera).

## Changelog

`CHANGELOG.md` is **generated**, not hand-edited for routine entries:

```bash
# full file (from all tags + unreleased)
git-cliff -o CHANGELOG.md

# preview unreleased only
git-cliff --unreleased
```

On tag push (`vX.Y.Z`), release CI runs `git-cliff --latest --strip header` and
uses that as the GitHub Release body. You do **not** need to paste notes into
`CHANGELOG.md` before tagging; regenerate the file when you want the repo copy
updated (e.g. after a release, or in a prep PR).

## Workflow

Branch → PR against `master` → status checks green (`verify` on ubuntu + macOS) → merge.

`master` is protected via a GitHub ruleset (see `.github/rulesets/branch-master.json`):
no direct pushes, linear history, squash/rebase only, required CI contexts
`verify (ubuntu-latest)` and `verify (macos-14)`. Release tags `v*` are protected
against deletion/rewrite (`.github/rulesets/tag-release.json`).

Ruleset JSON under `.github/rulesets/` matches what is applied on GitHub
(copied from tessera via `gh api`, with status checks set to this repo’s
`verify` matrix). DeployKey may bypass; Dependabot integration bypass was
omitted (GitHub rejects it for this personal repo).

```bash
# list
gh api repos/kaankoken/zj-agents/rulesets

# update after editing JSON (use ruleset id from list)
# gh api --method PUT repos/kaankoken/zj-agents/rulesets/<id> --input .github/rulesets/branch-master.json
```

## Releasing

Maintainers only:

1. Ensure recent commits use conventional prefixes so git-cliff groups them.
2. Tag `vX.Y.Z` on the commit to ship and push the tag (`git push origin vX.Y.Z`).
3. GitHub Actions: git-cliff generates release notes; both `.wasm` artifacts +
   `SHA256SUMS` are uploaded to the GitHub Release.
4. Optionally refresh the in-repo changelog: `git-cliff -o CHANGELOG.md` and open a PR.
