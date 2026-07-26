# Security Policy

## Supported Versions

`zj-agents` is pre-1.0; only the latest release receives security fixes.

| Version | Supported |
| ------- | --------- |
| latest  | ✅        |
| < latest| ❌        |

## Reporting a Vulnerability

Please **do not** open a public issue for security problems.

Report privately via GitHub Security Advisories:
<https://github.com/kaankoken/zj-agents/security/advisories/new>

You'll get an acknowledgement within a few days. Once a fix is available and
released, the advisory is published with credit (unless you prefer to stay
anonymous).

## Scope notes

- The engine plugin may request permissions to read pane contents, run host
  commands (`zellij action list-panes`, notifications), and message other plugins.
- Treat override manifests and `notify_command` templates as trusted configuration.
- Raw viewport text stays engine-local by design; please report any path that
  leaks viewport, tokens, or secrets into logs, snapshots, or notifications.
