# zj-agents

[![CI](https://github.com/kaankoken/zj-agents/actions/workflows/verify.yml/badge.svg)](https://github.com/kaankoken/zj-agents/actions/workflows/verify.yml)
[![Release](https://img.shields.io/github/v/release/kaankoken/zj-agents)](https://github.com/kaankoken/zj-agents/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Semantic coding-agent awareness for **stock Zellij ≥ 0.44.3**.

Two WASM plugins share a pure Rust core:

| Artifact | Role |
|---|---|
| `zj-agents-engine.wasm` | Background engine: discovery, classification, notifications, snapshots |
| `zj-agents-sidebar.wasm` | Hideable floating sidebar UI |

No Zellij fork is required. Install **from a GitHub Release** (recommended), then
register named plugin aliases in your Zellij config — the same distribution model
as [room](https://github.com/rvcas/room), [harpoon](https://github.com/Nacho114/harpoon),
and the [official plugin tutorial](https://zellij.dev/tutorials/developing-a-rust-plugin/).

## Install (from release)

`zellij` must resolve on the Zellij **server’s** inherited `PATH`
(used for `zellij action list-panes --all --json`).

### 1) Download plugins from GitHub Releases

Assets are published on every `v*` tag:
[latest release](https://github.com/kaankoken/zj-agents/releases/latest)
(`zj-agents-engine.wasm`, `zj-agents-sidebar.wasm`, `SHA256SUMS`).

**One-liner (bash/zsh) — latest:**

```bash
mkdir -p ~/.config/zellij/plugins
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-engine.wasm \
  "https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-engine.wasm"
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-sidebar.wasm \
  "https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-sidebar.wasm"
```

**Pin a version** (example `v0.1.0`):

```bash
VER=v0.1.0
mkdir -p ~/.config/zellij/plugins
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-engine.wasm \
  "https://github.com/kaankoken/zj-agents/releases/download/${VER}/zj-agents-engine.wasm"
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-sidebar.wasm \
  "https://github.com/kaankoken/zj-agents/releases/download/${VER}/zj-agents-sidebar.wasm"
```

**Nushell helper** (from a clone, or copy `scripts/install.nu`):

```nu
# latest release assets → ~/.config/zellij/plugins/
nu scripts/install.nu --from-release

# pinned tag
nu scripts/install.nu --from-release --tag v0.1.0
```

**Optional integrity check:**

```bash
VER=v0.1.0   # or use …/latest/download/SHA256SUMS
cd ~/.config/zellij/plugins
curl -fsSL -O "https://github.com/kaankoken/zj-agents/releases/download/${VER}/SHA256SUMS"
# Linux: sha256sum -c SHA256SUMS
# macOS: shasum -a 256 -c SHA256SUMS
```

### 2) Register named plugins in Zellij config

```kdl
plugins {
    // …built-ins…
    zj-agents-engine location="file:~/.config/zellij/plugins/zj-agents-engine.wasm"
    zj-agents-sidebar location="file:~/.config/zellij/plugins/zj-agents-sidebar.wasm"
}

load_plugins {
    zj-agents-engine {
        notify true
        // notify_command "[\"notify-send\",\"--\",\"{title}\",\"{body}\"]"
        // manifest_dir "~/…/custom-agent-detection"
    }
}
```

Keybind example (prefer **Ctrl** over Alt on Turkish Q layouts):

```kdl
// e.g. under a leader mode, or shared_except "locked"
bind "a" {
    LaunchOrFocusPlugin "zj-agents-sidebar" {
        floating true
        move_to_focused_tab true
    }
}
```

**Zero-copy option:** point aliases at release HTTPS URLs (Zellij downloads/caches):

```kdl
plugins {
    zj-agents-engine location="https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-engine.wasm"
    zj-agents-sidebar location="https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-sidebar.wasm"
}
```

Local `file:` copies are better for offline use and stable permission grants.

### 3) Start a new session

1. Start Zellij (engine loads with `load_plugins`).
2. Grant the **engine** permission batch once.
3. Open the sidebar and grant its batch.
4. Wait until “Connecting…” clears (≤ ~30s).
5. Run a supported agent in a pane (`claude`, `codex`, `grok`, `pi`, `omp`, `agy`).

## Configuration keys (engine only)

| Key | Default | Meaning |
|---|---|---|
| `manifest_dir` | `$HOME/.config/zellij/zj-agents/agent-detection` | Override directory (`absolute` or `~/...`) |
| `notify` | `true` | Master notification switch |
| `notify_command` | absent | Optional JSON argv template; otherwise host default |

## Permissions

Each plugin requests its **complete** permission batch once after subscribing.

### Engine

- `ReadApplicationState`
- `ReadPaneContents`
- `RunCommands`
- `MessageAndLaunchOtherPlugins`

### Sidebar

- `ChangeApplicationState`
- `MessageAndLaunchOtherPlugins`

Batch denial leaves the engine inert; the sidebar shows a local denial screen.

## Bundled agents

Fixture-backed only: Claude, Codex, Grok, Pi, OMP, Antigravity (`agy`). Patterns come from reviewed
redacted fixtures under `crates/zj-agents-core/tests/fixtures/`.

## Privacy

Raw pane contents never leave the engine. Snapshots carry sanitized display
metadata and derived state only.

## Build from source (developers)

```bash
git clone https://github.com/kaankoken/zj-agents
cd zj-agents
rustup target add wasm32-wasip1
nu scripts/install.nu    # cargo release build + copy into ~/.config/zellij/plugins/
# or: cargo build --workspace --release --target wasm32-wasip1
```

Nushell does **not** expand bash `{engine,sidebar}` braces — list both files or use `scripts/install.nu`.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Releases

Push a tag `vX.Y.Z`. GitHub Actions:

1. Generates release notes with **git-cliff** from conventional commits (`cliff.toml`).
2. Builds both plugins and attaches:
   - `zj-agents-engine.wasm`
   - `zj-agents-sidebar.wasm`
   - `SHA256SUMS`

Regenerate the in-repo changelog anytime with `git-cliff -o CHANGELOG.md`. Optional:
list the project on [awesome-zellij](https://github.com/zellij-org/awesome-zellij).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). By participating you agree to the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Report vulnerabilities privately — see [SECURITY.md](SECURITY.md).

## License

Licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option. Unless you explicitly state otherwise, any contribution
intentionally submitted for inclusion in this work shall be dual-licensed as
above, without any additional terms or conditions.
