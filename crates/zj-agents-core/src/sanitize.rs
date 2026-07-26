pub fn sanitize_metadata(raw: &str, limit: usize) -> String {
    let stripped = strip_ansi_escapes::strip(raw.as_bytes());
    let text = String::from_utf8_lossy(&stripped);
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_control() {
            continue;
        }
        if is_bidi_or_zero_width(ch) {
            continue;
        }
        out.push(ch);
    }
    let collapsed: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(limit).collect()
}

pub fn sanitize_label(raw: &str, limit: usize, pane_id: u32) -> String {
    let cleaned = sanitize_metadata(raw, limit);
    if cleaned.is_empty() {
        format!("pane {pane_id}")
    } else {
        cleaned
    }
}

pub fn escape_notify_markup(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn choose_display(
    cwd_basename: Option<&str>,
    pane_title: Option<&str>,
    pane_id: u32,
) -> String {
    if let Some(cwd) = cwd_basename {
        let cleaned = sanitize_metadata(cwd, 60);
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    if let Some(title) = pane_title {
        let cleaned = sanitize_metadata(title, 60);
        if !cleaned.is_empty() {
            return cleaned;
        }
    }
    format!("pane {pane_id}")
}

fn is_bidi_or_zero_width(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentState, DiagnosticSource};
    use crate::protocol::{AgentSnapshot, Diagnostic, Snapshot, TabSnapshot, PROTOCOL_VERSION};

    #[test]
    fn sanitizer_removes_terminal_and_unicode_controls() {
        let raw = "\u{1b}]0;secret\u{7}\u{202e}repo\u{200b}\n\tname";
        assert_eq!(sanitize_label(raw, 60, 9), "reponame");
    }

    #[test]
    fn empty_label_falls_back_to_pane_id() {
        assert_eq!(sanitize_label("\u{1b}[31m\u{1b}[0m", 60, 9), "pane 9");
    }

    #[test]
    fn truncation_counts_unicode_scalars() {
        assert_eq!(sanitize_label("aé日z", 3, 9), "aé日");
    }

    #[test]
    fn notify_markup_is_literal() {
        assert_eq!(escape_notify_markup("a&<b>"), "a&amp;&lt;b&gt;");
    }

    #[test]
    fn manifest_labels_and_tab_names_use_the_same_control_stripping() {
        assert_eq!(
            sanitize_metadata("\u{1b}]0;secret\u{7}Claude\u{200b}", 60),
            "Claude"
        );
        assert_eq!(sanitize_metadata("\u{202e}work\u{2069}", 60), "work");
    }

    #[test]
    fn choose_display_prefers_cwd_then_title() {
        assert_eq!(choose_display(Some("repo"), Some("title"), 3), "repo");
        assert_eq!(choose_display(None, Some("title"), 3), "title");
        assert_eq!(choose_display(None, None, 3), "pane 3");
    }

    #[test]
    fn snapshot_only_carries_sanitized_metadata() {
        let hostile_label = sanitize_label("\u{1b}[31mClaude\u{200b}", 60, 1);
        let hostile_tab = sanitize_metadata("\u{202e}work", 60);
        let snapshot = Snapshot {
            v: PROTOCOL_VERSION,
            tabs: vec![TabSnapshot {
                position: 0,
                name: hostile_tab.clone(),
            }],
            agents: vec![AgentSnapshot {
                pane_id: 1,
                tab_position: 0,
                agent: "claude".into(),
                agent_label: hostile_label.clone(),
                display: choose_display(Some("\u{1b}repo"), None, 1),
                state: AgentState::Idle,
                since_ms: 0,
                fallback_used: false,
            }],
            diagnostics: vec![Diagnostic::new(
                DiagnosticSource::Manifest,
                sanitize_metadata("bad", 60),
            )],
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains('\u{1b}'));
        assert!(!json.contains('\u{202e}'));
        assert_eq!(snapshot.agents[0].agent_label, "Claude");
        assert_eq!(snapshot.tabs[0].name, "work");
    }
}
