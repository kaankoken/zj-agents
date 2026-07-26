# zj-agents

Semantic coding-agent awareness for **stock Zellij ≥ 0.44.3**.

Two WASM plugins share a pure Rust core:

| Artifact | Role |
|---|---|
| `zj-agents-engine.wasm` | Background engine: discovery, classification, notifications, snapshots |
| `zj-agents-sidebar.wasm` | Hideable floating sidebar UI |

No Zellij fork is required.

## Build and install

`zellij` must resolve on the Zellij **server’s** inherited `PATH` (used for `zellij action list-panes --all --json`).

```bash
rustup target add wasm32-wasip1
```

**Nushell (recommended on this machine):**

```nu
nu scripts/install.nu
```

**Bash/zsh:**

```bash
cargo build --workspace --release --target wasm32-wasip1
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/zj-agents-engine.wasm \
   target/wasm32-wasip1/release/zj-agents-sidebar.wasm \
   ~/.config/zellij/plugins/
```

Nushell does **not** expand `{engine,sidebar}` — list both files or use `scripts/install.nu`.

## Configuration (KDL)

Register **named plugin aliases**, load the engine at session start, launch the sidebar by alias (not raw `file:` paths in keybinds).

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
    }
}

keybinds {
    // example: leader chord (prefer Ctrl over Alt on Turkish Q layouts)
    tmux {
        bind "a" {
            LaunchOrFocusPlugin "zj-agents-sidebar" {
                floating true
                move_to_focused_tab true
            }
            SwitchToMode "normal"
        }
    }
}
```

### Configuration keys (engine only)

Exactly three keys:

| Key | Default | Meaning |
|---|---|---|
| `manifest_dir` | `$HOME/.config/zellij/zj-agents/agent-detection` | Override directory (`absolute` or `~/...`) |
| `notify` | `true` | Master notification switch |
| `notify_command` | absent | Optional JSON argv template; otherwise host default |

## Permissions

Each plugin requests its **complete** permission batch once after subscribing. Stock v0.44.3 treats the result as all-or-nothing.

### Engine batch

- `ReadApplicationState`
- `ReadPaneContents`
- `RunCommands`
- `MessageAndLaunchOtherPlugins`

### Sidebar batch

- `ChangeApplicationState`
- `MessageAndLaunchOtherPlugins`

**Batch denial:** the engine stays inert (no discovery, pipes, or notifications). The sidebar shows:  
`Sidebar permissions denied; engine connection and pane focus unavailable.`

Document these prompts before installing so operators know what they are granting.

## Bundled agents

Fixture-backed manifests only:

- Claude
- Codex
- Grok
- Pi
- OMP

Patterns come from reviewed redacted fixtures under `crates/zj-agents-core/tests/fixtures/`. Agents without real fixtures are not bundled.

Override TOML files live in `manifest_dir`. Press `r` in the sidebar (or pipe `zj-agents:reload`) to reload atomically: invalid candidates are rejected as a set without partial adoption or notification.

## Notifications

Defaults when `notify = true` and `notify_command` is unset:

- **Linux:** `notify-send -- {title} {body}`
- **macOS:** `osascript` with `{title}`/`{body}` after `--`

Hostile labels are sanitized; `notify-send` markup is escaped. Only new unfocused `Blocked` and `Done` transitions notify (coalesced).

## Privacy

Raw pane contents never leave the engine. Snapshots carry sanitized display metadata and derived state only. Do not log viewports, full commands, paths, titles, or fixture bodies.

## Sidebar keys

| Key | Action |
|---|---|
| `Up` / `k` | Previous row |
| `Down` / `j` | Next row |
| `Enter` | Focus selected pane and hide |
| `r` | Reload manifests |
| `q` / `Esc` | Hide |

## Smoke checklist

Against unmodified Zellij 0.44.3:

1. Engine starts with pre-existing panes; CLI inventory bootstrap works.
2. Deny engine batch once → engine inert.
3. Deny sidebar batch once → exact denial screen.
4. Grant both; promote/reconcile/demote/close agents.
5. Exercise Unknown, Blocked, Working, Idle, Done, focus suppression.
6. Hide/reopen/navigate/focus sidebar.
7. Sidebar before engine or engine restart → hello reconnect within 30s.
8. Malformed/version-mismatched pipes → diagnostics / incompatibility UI.
9. Valid multi-file reload accepted; invalid rejected atomically.
10. Notification argv safety for Linux/macOS.
11. `Blocked → Working → Blocked` coalescing.
12. No duplicate engine via broadcast/ID pipes.

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo build --workspace --release --target wasm32-wasip1
```
