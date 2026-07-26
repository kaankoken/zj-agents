use std::collections::BTreeMap;
use std::path::Path;

use crate::model::DiagnosticSource;
use crate::protocol::Diagnostic;
use crate::sanitize::{escape_notify_markup, sanitize_metadata};
use crate::state::AttentionKind;

pub const NOTIFICATION_TITLE: &str = "zj-agents";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostOs {
    Linux,
    Darwin,
}

#[derive(Clone, Debug, Default)]
pub struct DiagnosticSlots {
    slots: BTreeMap<DiagnosticSource, String>,
}

impl DiagnosticSlots {
    pub fn set(&mut self, source: DiagnosticSource, message: impl AsRef<str>) {
        let cleaned = sanitize_metadata(message.as_ref(), 120);
        if cleaned.is_empty() {
            self.slots.remove(&source);
        } else {
            self.slots.insert(source, cleaned);
        }
    }

    pub fn clear(&mut self, source: DiagnosticSource) {
        self.slots.remove(&source);
    }

    pub fn snapshot(&self) -> Vec<Diagnostic> {
        self.slots
            .iter()
            .map(|(source, message)| Diagnostic::new(*source, message.clone()))
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct NotifyTemplate {
    argv: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotifyTemplateError {
    InvalidJson,
    Empty,
    MissingPlaceholder,
    RepeatedPlaceholder,
    PlaceholderNotWholeElement,
    MissingDoubleDash,
    ShellExecutable,
}

impl NotifyTemplate {
    pub fn parse(json: &str) -> Result<Self, NotifyTemplateError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|_| NotifyTemplateError::InvalidJson)?;
        let arr = value.as_array().ok_or(NotifyTemplateError::InvalidJson)?;
        if arr.is_empty() {
            return Err(NotifyTemplateError::Empty);
        }
        let mut argv = Vec::with_capacity(arr.len());
        for item in arr {
            let s = item.as_str().ok_or(NotifyTemplateError::InvalidJson)?;
            argv.push(s.to_owned());
        }
        validate_template(&argv)?;
        Ok(Self { argv })
    }

    pub fn from_argv(argv: Vec<String>) -> Result<Self, NotifyTemplateError> {
        validate_template(&argv)?;
        Ok(Self { argv })
    }

    pub fn instantiate(&self, body: &str) -> Vec<String> {
        let title = NOTIFICATION_TITLE;
        let body_value = if executable_is_notify_send(&self.argv[0]) {
            escape_notify_markup(body)
        } else {
            body.to_owned()
        };
        self.argv
            .iter()
            .map(|part| match part.as_str() {
                "{title}" => title.to_owned(),
                "{body}" => body_value.clone(),
                other => other.to_owned(),
            })
            .collect()
    }
}

fn validate_template(argv: &[String]) -> Result<(), NotifyTemplateError> {
    if argv.is_empty() {
        return Err(NotifyTemplateError::Empty);
    }
    let basename = Path::new(&argv[0])
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(argv[0].as_str());
    if matches!(basename, "sh" | "bash" | "dash" | "zsh" | "fish") {
        return Err(NotifyTemplateError::ShellExecutable);
    }
    let title_count = argv.iter().filter(|p| p.as_str() == "{title}").count();
    let body_count = argv.iter().filter(|p| p.as_str() == "{body}").count();
    if title_count == 0 || body_count == 0 {
        return Err(NotifyTemplateError::MissingPlaceholder);
    }
    if title_count > 1 || body_count > 1 {
        return Err(NotifyTemplateError::RepeatedPlaceholder);
    }
    if argv.iter().any(|p| {
        (p.contains("{title}") && p.as_str() != "{title}")
            || (p.contains("{body}") && p.as_str() != "{body}")
    }) {
        return Err(NotifyTemplateError::PlaceholderNotWholeElement);
    }
    let title_idx = argv.iter().position(|p| p.as_str() == "{title}").unwrap();
    let body_idx = argv.iter().position(|p| p.as_str() == "{body}").unwrap();
    let first_placeholder = title_idx.min(body_idx);
    if !argv[..first_placeholder].iter().any(|p| p.as_str() == "--") {
        return Err(NotifyTemplateError::MissingDoubleDash);
    }
    Ok(())
}

fn executable_is_notify_send(executable: &str) -> bool {
    Path::new(executable).file_name().and_then(|s| s.to_str()) == Some("notify-send")
}

pub fn default_notify_argv(host: HostOs) -> Vec<String> {
    match host {
        HostOs::Linux => vec![
            "notify-send".into(),
            "--".into(),
            "{title}".into(),
            "{body}".into(),
        ],
        HostOs::Darwin => vec![
            "osascript".into(),
            "-e".into(),
            "on run argv".into(),
            "-e".into(),
            "display notification (item 2 of argv) with title (item 1 of argv)".into(),
            "-e".into(),
            "end run".into(),
            "--".into(),
            "{title}".into(),
            "{body}".into(),
        ],
    }
}

pub fn parse_host_os(stdout: &str) -> Option<HostOs> {
    match stdout.trim() {
        "Linux" => Some(HostOs::Linux),
        "Darwin" => Some(HostOs::Darwin),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingAttention {
    pub pane_id: u32,
    pub state: AttentionKind,
    pub generation: u64,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct AttentionQueue {
    pending: BTreeMap<u32, PendingAttention>,
    remaining_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionEvent {
    pub pane_id: u32,
    pub state: AttentionKind,
    pub generation: u64,
    pub label: String,
}

impl AttentionQueue {
    pub fn enqueue(&mut self, event: PendingAttention) {
        let is_first = self.pending.is_empty();
        self.pending.insert(event.pane_id, event);
        if is_first {
            self.remaining_ms = Some(2_000);
        }
    }

    pub fn on_focused(&mut self, pane_id: u32) {
        self.pending.remove(&pane_id);
        if self.pending.is_empty() {
            self.remaining_ms = None;
        }
    }

    pub fn invalidate_if_stale(&mut self, pane_id: u32, generation: u64, state: AttentionKind) {
        if let Some(entry) = self.pending.get(&pane_id) {
            if entry.generation != generation || entry.state != state {
                self.pending.remove(&pane_id);
            }
        }
        if self.pending.is_empty() {
            self.remaining_ms = None;
        }
    }

    pub fn advance(
        &mut self,
        elapsed_ms: u64,
        still_valid: impl Fn(&PendingAttention) -> bool,
    ) -> Option<Vec<AttentionEvent>> {
        let remaining = self.remaining_ms.as_mut()?;
        *remaining = remaining.saturating_sub(elapsed_ms);
        if *remaining > 0 {
            return None;
        }
        let pending = std::mem::take(&mut self.pending);
        let mut emitted = Vec::new();
        for (_, entry) in pending {
            if still_valid(&entry) {
                emitted.push(AttentionEvent {
                    pane_id: entry.pane_id,
                    state: entry.state,
                    generation: entry.generation,
                    label: entry.label,
                });
            }
        }
        self.remaining_ms = None;
        Some(emitted)
    }

    pub fn format_body(events: &[AttentionEvent]) -> String {
        match events {
            [] => String::new(),
            [one] => match one.state {
                AttentionKind::Blocked => format!("{} blocked", one.label),
                AttentionKind::Done => format!("{} finished", one.label),
            },
            many => {
                let all_blocked = many.iter().all(|e| e.state == AttentionKind::Blocked);
                let all_done = many.iter().all(|e| e.state == AttentionKind::Done);
                let n = many.len();
                if all_blocked {
                    format!("{n} agents blocked")
                } else if all_done {
                    format!("{n} agents finished")
                } else {
                    format!("{n} agents need attention")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Diagnostic;

    #[test]
    fn diagnostics_are_current_slots_in_protocol_order() {
        let mut slots = DiagnosticSlots::default();
        slots.set(DiagnosticSource::Protocol, "bad message");
        slots.set(DiagnosticSource::Inventory, "list failed");
        slots.set(DiagnosticSource::Protocol, "unsupported v");
        assert_eq!(
            slots.snapshot(),
            vec![
                Diagnostic::new(DiagnosticSource::Inventory, "list failed"),
                Diagnostic::new(DiagnosticSource::Protocol, "unsupported v"),
            ]
        );
        slots.clear(DiagnosticSource::Inventory);
        assert_eq!(slots.snapshot().len(), 1);
    }

    #[test]
    fn macos_default_keeps_untrusted_values_after_double_dash() {
        let argv = default_notify_argv(HostOs::Darwin);
        assert_eq!(argv[7], "--");
        assert_eq!(argv[8], "{title}");
        assert_eq!(argv[9], "{body}");
    }

    #[test]
    fn executable_basename_rejects_shells() {
        assert!(NotifyTemplate::parse(r#"["/bin/sh","--","{title}","{body}"]"#).is_err());
    }

    #[test]
    fn notify_send_escapes_markup_in_the_instantiated_body() {
        for executable in ["notify-send", "/usr/bin/notify-send"] {
            let template =
                NotifyTemplate::parse(&format!(r#"["{executable}","--","{{title}}","{{body}}"]"#))
                    .unwrap();
            assert_eq!(
                template.instantiate("repo&<ui> blocked"),
                vec![
                    executable.to_owned(),
                    "--".to_owned(),
                    "zj-agents".to_owned(),
                    "repo&amp;&lt;ui&gt; blocked".to_owned(),
                ]
            );
        }
    }

    #[test]
    fn linux_default_argv() {
        assert_eq!(
            default_notify_argv(HostOs::Linux),
            vec!["notify-send", "--", "{title}", "{body}"]
        );
    }

    #[test]
    fn blocked_working_blocked_emits_only_final_generation() {
        let mut q = AttentionQueue::default();
        q.enqueue(PendingAttention {
            pane_id: 1,
            state: AttentionKind::Blocked,
            generation: 1,
            label: "a".into(),
        });
        q.invalidate_if_stale(1, 2, AttentionKind::Done);
        q.enqueue(PendingAttention {
            pane_id: 1,
            state: AttentionKind::Blocked,
            generation: 3,
            label: "a".into(),
        });
        let events = q
            .advance(2_000, |e| {
                e.generation == 3 && e.state == AttentionKind::Blocked
            })
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].generation, 3);
        assert_eq!(AttentionQueue::format_body(&events), "a blocked");
    }

    #[test]
    fn focused_pending_is_dropped_even_if_unfocused_before_deadline() {
        let mut q = AttentionQueue::default();
        q.enqueue(PendingAttention {
            pane_id: 2,
            state: AttentionKind::Done,
            generation: 1,
            label: "b".into(),
        });
        q.on_focused(2);
        let events = q.advance(2_000, |_| true);
        assert!(events.is_none() || events.unwrap().is_empty());
    }

    #[test]
    fn mixed_batch_body() {
        let events = vec![
            AttentionEvent {
                pane_id: 1,
                state: AttentionKind::Blocked,
                generation: 1,
                label: "a".into(),
            },
            AttentionEvent {
                pane_id: 2,
                state: AttentionKind::Done,
                generation: 1,
                label: "b".into(),
            },
        ];
        assert_eq!(
            AttentionQueue::format_body(&events),
            "2 agents need attention"
        );
    }

    #[test]
    fn parse_host_os_exact() {
        assert_eq!(parse_host_os("Linux\n"), Some(HostOs::Linux));
        assert_eq!(parse_host_os("Darwin"), Some(HostOs::Darwin));
        assert_eq!(parse_host_os("Windows"), None);
    }
}
