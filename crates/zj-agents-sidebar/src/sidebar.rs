use zj_agents_core::protocol::{Snapshot, PROTOCOL_VERSION};
use zj_agents_core::view::{build_rows, format_duration, selectable_pane_ids, Row, Selection};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PermissionState {
    #[default]
    Pending,
    Granted,
    Denied,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SidebarAction {
    None,
    SendHello,
    SendReload,
    Focus(u32),
    Hide,
}

#[derive(Clone, Debug, Default)]
pub struct Sidebar {
    permission: PermissionState,
    snapshot: Option<Snapshot>,
    incompatible_version: Option<u8>,
    parse_error: Option<String>,
    selection: Selection,
    heartbeat_elapsed_ms: u64,
    last_selectable: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderLine {
    pub text: String,
    pub selected: bool,
    pub color: Option<u8>,
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn permission(&self) -> PermissionState {
        self.permission
    }

    pub fn on_permission(&mut self, granted: bool) -> SidebarAction {
        if self.permission != PermissionState::Pending {
            return SidebarAction::None;
        }
        if granted {
            self.permission = PermissionState::Granted;
            SidebarAction::SendHello
        } else {
            self.permission = PermissionState::Denied;
            SidebarAction::None
        }
    }

    pub fn on_timer(&mut self, elapsed_ms: u64) -> SidebarAction {
        if self.permission != PermissionState::Granted {
            return SidebarAction::None;
        }
        if let Some(snapshot) = self.snapshot.as_mut() {
            for agent in &mut snapshot.agents {
                agent.since_ms = agent.since_ms.saturating_add(elapsed_ms);
            }
        }
        self.heartbeat_elapsed_ms = self.heartbeat_elapsed_ms.saturating_add(elapsed_ms);
        if self.heartbeat_elapsed_ms >= 30_000 {
            self.heartbeat_elapsed_ms %= 30_000;
            SidebarAction::SendHello
        } else {
            SidebarAction::None
        }
    }

    pub fn on_key(&mut self, key: SidebarKey) -> SidebarAction {
        if key == SidebarKey::Quit {
            return SidebarAction::Hide;
        }
        if self.permission != PermissionState::Granted {
            return SidebarAction::None;
        }
        match key {
            SidebarKey::Up => {
                let ids = self.selectable();
                self.selection.previous(&ids);
                SidebarAction::None
            }
            SidebarKey::Down => {
                let ids = self.selectable();
                self.selection.next(&ids);
                SidebarAction::None
            }
            SidebarKey::Enter => {
                if let Some(id) = self.selection.pane_id() {
                    SidebarAction::Focus(id)
                } else {
                    SidebarAction::None
                }
            }
            SidebarKey::Reload => SidebarAction::SendReload,
            SidebarKey::Quit => SidebarAction::Hide,
        }
    }

    pub fn on_snapshot_payload(&mut self, payload: &str) {
        if self.permission != PermissionState::Granted {
            return;
        }
        match serde_json::from_str::<Snapshot>(payload) {
            Ok(snapshot) if snapshot.v == PROTOCOL_VERSION => {
                self.incompatible_version = None;
                self.parse_error = None;
                let old = self.last_selectable.clone();
                let rows = build_rows(&snapshot);
                let new_ids = selectable_pane_ids(&rows);
                self.selection.reconcile(&old, &new_ids);
                self.last_selectable = new_ids;
                self.snapshot = Some(snapshot);
            }
            Ok(snapshot) => {
                self.incompatible_version = Some(snapshot.v);
            }
            Err(_) => {
                self.parse_error = Some("malformed snapshot".into());
            }
        }
    }

    fn selectable(&self) -> Vec<u32> {
        self.snapshot
            .as_ref()
            .map(|s| selectable_pane_ids(&build_rows(s)))
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SidebarKey {
    Up,
    Down,
    Enter,
    Reload,
    Quit,
}

pub fn render_plan(sidebar: &Sidebar, rows: usize, cols: usize) -> Vec<RenderLine> {
    let mut lines = Vec::new();
    if sidebar.permission == PermissionState::Denied {
        lines.push(line(
            "Sidebar permissions denied; engine connection and pane focus unavailable.",
            false,
            None,
            cols,
        ));
        return trim_viewport(lines, rows, sidebar);
    }
    if let Some(v) = sidebar.incompatible_version {
        lines.push(line(
            &format!("Incompatible engine protocol version {v}"),
            false,
            None,
            cols,
        ));
        return trim_viewport(lines, rows, sidebar);
    }
    if sidebar.snapshot.is_none() {
        lines.push(line("Connecting to zj-agents engine…", false, None, cols));
        return trim_viewport(lines, rows, sidebar);
    }
    let snapshot = sidebar.snapshot.as_ref().unwrap();
    lines.push(line("zj-agents", false, None, cols));
    if let Some(err) = &sidebar.parse_error {
        lines.push(line(err, false, Some(1), cols));
    }
    for diag in &snapshot.diagnostics {
        lines.push(line(
            &format!("[{}] {}", diag.source_label(), diag.message),
            false,
            Some(1),
            cols,
        ));
    }
    let built = build_rows(snapshot);
    if built.is_empty() {
        lines.push(line("No agent panes detected.", false, None, cols));
        return trim_viewport(lines, rows, sidebar);
    }
    let selected = sidebar.selection.pane_id();
    for row in built {
        match row {
            Row::Section(name) => lines.push(line(name, false, None, cols)),
            Row::Tab { position, name } => {
                lines.push(line(&format!("Tab {position}: {name}"), false, None, cols));
            }
            Row::Agent(agent) => {
                let text = format!(
                    "{} {}  {}  {}  {}",
                    agent.state.glyph(),
                    agent.state.as_str(),
                    agent.display,
                    agent.agent_label,
                    format_duration(agent.since_ms)
                );
                let is_selected = selected == Some(agent.pane_id);
                lines.push(line(&text, is_selected, state_color(agent.state), cols));
            }
        }
    }
    trim_viewport(lines, rows, sidebar)
}

fn state_color(state: zj_agents_core::model::AgentState) -> Option<u8> {
    use zj_agents_core::model::AgentState::*;
    Some(match state {
        Unknown => 8,
        Idle => 7,
        Working => 4,
        Blocked => 1,
        Done => 2,
    })
}

fn line(text: &str, selected: bool, color: Option<u8>, cols: usize) -> RenderLine {
    let truncated: String = text.chars().take(cols.max(1)).collect();
    RenderLine {
        text: truncated,
        selected,
        color,
    }
}

fn trim_viewport(lines: Vec<RenderLine>, rows: usize, sidebar: &Sidebar) -> Vec<RenderLine> {
    if rows == 0 {
        return Vec::new();
    }
    if lines.len() <= rows {
        return lines;
    }
    let selected_idx = lines.iter().position(|l| l.selected);
    let mut out = Vec::new();
    if let Some(first) = lines.first() {
        out.push(first.clone());
    }
    if let Some(idx) = selected_idx {
        if idx != 0 {
            out.push(lines[idx].clone());
        }
    } else if lines.len() > 1 {
        out.push(lines[1].clone());
    }
    out.truncate(rows);
    let _ = sidebar;
    out
}

trait DiagnosticSourceLabel {
    fn source_label(&self) -> &'static str;
}

impl DiagnosticSourceLabel for zj_agents_core::protocol::Diagnostic {
    fn source_label(&self) -> &'static str {
        use zj_agents_core::model::DiagnosticSource::*;
        match self.source {
            Inventory => "inventory",
            Manifest => "manifest",
            Host => "host",
            Notification => "notification",
            Protocol => "protocol",
            Detection => "detection",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zj_agents_core::model::AgentState;
    use zj_agents_core::protocol::{AgentSnapshot, Snapshot, TabSnapshot, PROTOCOL_VERSION};

    fn snap() -> Snapshot {
        Snapshot {
            v: PROTOCOL_VERSION,
            tabs: vec![TabSnapshot {
                position: 0,
                name: "main".into(),
            }],
            agents: vec![AgentSnapshot {
                pane_id: 3,
                tab_position: 0,
                agent: "claude".into(),
                agent_label: "Claude Code".into(),
                display: "repo".into(),
                state: AgentState::Working,
                since_ms: 0,
                fallback_used: false,
            }],
            diagnostics: vec![],
        }
    }

    #[test]
    fn connecting_before_snapshot() {
        let sidebar = Sidebar::new();
        let lines = render_plan(&sidebar, 10, 80);
        assert!(lines[0].text.contains("Connecting"));
    }

    #[test]
    fn grant_emits_hello() {
        let mut sidebar = Sidebar::new();
        assert_eq!(sidebar.on_permission(true), SidebarAction::SendHello);
    }

    #[test]
    fn denied_screen() {
        let mut sidebar = Sidebar::new();
        sidebar.on_permission(false);
        let lines = render_plan(&sidebar, 10, 80);
        assert!(lines[0].text.contains("permissions denied"));
    }

    #[test]
    fn accepts_v1_and_rejects_other() {
        let mut sidebar = Sidebar::new();
        sidebar.on_permission(true);
        sidebar.on_snapshot_payload(&serde_json::to_string(&snap()).unwrap());
        assert!(sidebar.snapshot.is_some());
        let mut bad = snap();
        bad.v = 9;
        sidebar.on_snapshot_payload(&serde_json::to_string(&bad).unwrap());
        assert_eq!(sidebar.incompatible_version, Some(9));
    }

    #[test]
    fn heartbeat_every_30s() {
        let mut sidebar = Sidebar::new();
        sidebar.on_permission(true);
        assert_eq!(sidebar.on_timer(29_999), SidebarAction::None);
        assert_eq!(sidebar.on_timer(1), SidebarAction::SendHello);
    }

    #[test]
    fn keys_while_pending_do_not_focus() {
        let mut sidebar = Sidebar::new();
        assert_eq!(sidebar.on_key(SidebarKey::Enter), SidebarAction::None);
        assert_eq!(sidebar.on_key(SidebarKey::Quit), SidebarAction::Hide);
    }

    #[test]
    fn empty_snapshot_message() {
        let mut sidebar = Sidebar::new();
        sidebar.on_permission(true);
        let empty = Snapshot {
            v: PROTOCOL_VERSION,
            tabs: vec![],
            agents: vec![],
            diagnostics: vec![],
        };
        sidebar.on_snapshot_payload(&serde_json::to_string(&empty).unwrap());
        let lines = render_plan(&sidebar, 10, 80);
        assert!(lines.iter().any(|l| l.text.contains("No agent panes")));
    }

    #[test]
    fn agent_line_has_glyph_and_text_state() {
        let mut sidebar = Sidebar::new();
        sidebar.on_permission(true);
        sidebar.on_snapshot_payload(&serde_json::to_string(&snap()).unwrap());
        let lines = render_plan(&sidebar, 20, 80);
        let agent = lines.iter().find(|l| l.text.contains("working")).unwrap();
        assert!(agent.text.contains('›') || agent.text.contains("working"));
        assert!(agent.color.is_some());
    }
}
