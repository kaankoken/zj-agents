# zj-agents

[![CI](https://github.com/kaankoken/zj-agents/actions/workflows/verify.yml/badge.svg)](https://github.com/kaankoken/zj-agents/actions/workflows/verify.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Semantic coding-agent awareness for **stock Zellij ≥ 0.44.3**.

Two WASM plugins share a pure Rust core:

| Artifact | Role |
|---|---|
| `zj-agents-engine.wasm` | Background engine: discovery, classification, notifications, snapshots |
| `zj-agents-sidebar.wasm` | Hideable floating sidebar UI |

No Zellij fork is required. Zellij has no plugin package manager — distribution
is **release WASM + config**, the same model as [room](https://github.com/rvcas/room),
[harpoon](https://github.com/Nacho114/harpoon), and the [official plugin tutorial](https://zellij.dev/tutorials/developing-a-rust-plugin/).

## Install

`zellij` must resolve on the Zellij **server’s** inherited `PATH`
(used for `zellij action list-panes --all --json`).

### 1) Get the WASM plugins

**From a GitHub Release (recommended for users):**

```bash
mkdir -p ~/.config/zellij/plugins
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-engine.wasm \
  "https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-engine.wasm"
curl -fsSL -o ~/.config/zellij/plugins/zj-agents-sidebar.wasm \
  "https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-sidebar.wasm"
```

**Nushell:**

```nu
nu scripts/install.nu --from-release
# or a specific tag:
# nu scripts/install.nu --from-release --tag v0.1.0
```

**From source (developers):**

```bash
git clone https://github.com/kaankoken/zj-agents
cd zj-agents
rustup target add wasm32-wasip1
nu scripts/install.nu          # builds + copies both .wasm files
# or: cargo build --workspace --release --target wasm32-wasip1
```

Nushell does **not** expand bash braces — use `scripts/install.nu` or list both files.

Optional integrity check after a release install:

```bash
# download SHA256SUMS from the same release, then:
# sha256sum -c SHA256SUMS   # Linux
# shasum -a 256 -c SHA256SUMS  # macOS
```

### 2) Register named plugins in Zellij config

Add aliases, load the engine at session start, and bind the sidebar
([plugin aliases](https://zellij.dev/documentation/plugin-aliases.html),
[loading](https://zellij.dev/documentation/plugin-loading.html)):

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

HTTPS aliases (no local copy) also work if you prefer Zellij to fetch releases:

```kdl
plugins {
    zj-agents-engine location="https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-engine.wasm"
    zj-agents-sidebar location="https://github.com/kaankoken/zj-agents/releases/latest/download/zj-agents-sidebar.wasm"
}
```

Local `file:` copies are usually better for offline use and stable permissions.

### 3) Start a new session

1. Start Zellij (engine loads with `load_plugins`).
2. Grant the **engine** permission batch once.
3. Open the sidebar (`Ctrl+a` `a` if you used that binding) and grant its batch.
4. Wait until “Connecting…” clears (≤ ~30s).
5. Run a supported agent in a pane (`claude`, `codex`, `grok`, `pi`, `omp`).

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

Fixture-backed only: Claude, Codex, Grok, Pi, OMP. Patterns come from reviewed
redacted fixtures under `crates/zj-agents-core/tests/fixtures/`.

## Privacy

Raw pane contents never leave the engine. Snapshots carry sanitized display
metadata and derived state only.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo build --workspace --release --target wasm32-wasip1
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## Releases

Push a tag `vX.Y.Z`. GitHub Actions:

1. Generates release notes with **git-cliff** from conventional commits (`cliff.toml`).
2. Builds both plugins and attaches:
   - `zj-agents-engine.wasm`
   - `zj-agents-sidebar.wasm`
   - `SHA256SUMS`

Regenerate the in-repo file anytime with `git-cliff -o CHANGELOG.md`. Optional:
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
