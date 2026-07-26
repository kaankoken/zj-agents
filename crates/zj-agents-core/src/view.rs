use std::collections::BTreeMap;

use crate::model::AgentState;
use crate::protocol::{AgentSnapshot, Snapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Row {
    Section(&'static str),
    Tab { position: usize, name: String },
    Agent(AgentRow),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentRow {
    pub pane_id: u32,
    pub tab_position: usize,
    pub tab_name: String,
    pub agent: String,
    pub agent_label: String,
    pub display: String,
    pub state: AgentState,
    pub since_ms: u64,
    pub fallback_used: bool,
    pub in_attention: bool,
}

impl AgentRow {
    pub fn pane_id(&self) -> u32 {
        self.pane_id
    }

    pub fn line_text(&self) -> String {
        format!(
            "{} {}  {}  {}  {}",
            self.state.glyph(),
            self.state.as_str(),
            self.display,
            self.agent_label,
            format_duration(self.since_ms)
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Selection {
    pane_id: Option<u32>,
}

impl Selection {
    pub fn new() -> Self {
        Self { pane_id: None }
    }

    pub fn pane_id(&self) -> Option<u32> {
        self.pane_id
    }

    pub fn set(&mut self, pane_id: Option<u32>) {
        self.pane_id = pane_id;
    }

    pub fn reconcile(&mut self, old_ids: &[u32], new_ids: &[u32]) {
        if new_ids.is_empty() {
            self.pane_id = None;
            return;
        }
        if let Some(current) = self.pane_id {
            if new_ids.contains(&current) {
                return;
            }
            if let Some(old_idx) = old_ids.iter().position(|id| *id == current) {
                let nearest = old_idx.min(new_ids.len() - 1);
                self.pane_id = Some(new_ids[nearest]);
                return;
            }
        }
        self.pane_id = Some(new_ids[0]);
    }

    pub fn previous(&mut self, selectable: &[u32]) {
        if selectable.is_empty() {
            self.pane_id = None;
            return;
        }
        let idx = self
            .pane_id
            .and_then(|id| selectable.iter().position(|x| *x == id))
            .unwrap_or(0);
        let next = if idx == 0 {
            selectable.len() - 1
        } else {
            idx - 1
        };
        self.pane_id = Some(selectable[next]);
    }

    pub fn next(&mut self, selectable: &[u32]) {
        if selectable.is_empty() {
            self.pane_id = None;
            return;
        }
        let idx = self
            .pane_id
            .and_then(|id| selectable.iter().position(|x| *x == id))
            .unwrap_or(usize::MAX);
        let next = if idx == usize::MAX || idx + 1 >= selectable.len() {
            0
        } else {
            idx + 1
        };
        self.pane_id = Some(selectable[next]);
    }
}

pub fn build_rows(snapshot: &Snapshot) -> Vec<Row> {
    let tab_names: BTreeMap<usize, String> = snapshot
        .tabs
        .iter()
        .map(|t| (t.position, t.name.clone()))
        .collect();

    let mut attention: Vec<AgentRow> = Vec::new();
    let mut rest: Vec<AgentRow> = Vec::new();

    for agent in &snapshot.agents {
        let tab_name = tab_names
            .get(&agent.tab_position)
            .cloned()
            .unwrap_or_default();
        let row = agent_row(agent, tab_name);
        match agent.state {
            AgentState::Blocked | AgentState::Done => {
                let mut r = row;
                r.in_attention = true;
                attention.push(r);
            }
            _ => rest.push(row),
        }
    }

    attention.sort_by(|a, b| {
        attention_rank(a.state)
            .cmp(&attention_rank(b.state))
            .then(a.tab_position.cmp(&b.tab_position))
            .then(a.pane_id.cmp(&b.pane_id))
    });
    rest.sort_by(|a, b| {
        a.tab_position
            .cmp(&b.tab_position)
            .then(a.tab_name.cmp(&b.tab_name))
            .then(a.pane_id.cmp(&b.pane_id))
    });

    let mut rows = Vec::new();
    if !attention.is_empty() {
        rows.push(Row::Section("Attention"));
        for agent in attention {
            rows.push(Row::Agent(agent));
        }
    }

    let mut current_tab: Option<(usize, String)> = None;
    for agent in rest {
        let key = (agent.tab_position, agent.tab_name.clone());
        if current_tab.as_ref() != Some(&key) {
            rows.push(Row::Tab {
                position: key.0,
                name: key.1.clone(),
            });
            current_tab = Some(key);
        }
        rows.push(Row::Agent(agent));
    }
    rows
}

pub fn selectable_pane_ids(rows: &[Row]) -> Vec<u32> {
    rows.iter()
        .filter_map(|row| match row {
            Row::Agent(a) => Some(a.pane_id),
            _ => None,
        })
        .collect()
}

fn agent_row(agent: &AgentSnapshot, tab_name: String) -> AgentRow {
    AgentRow {
        pane_id: agent.pane_id,
        tab_position: agent.tab_position,
        tab_name,
        agent: agent.agent.clone(),
        agent_label: agent.agent_label.clone(),
        display: agent.display.clone(),
        state: agent.state,
        since_ms: agent.since_ms,
        fallback_used: agent.fallback_used,
        in_attention: false,
    }
}

fn attention_rank(state: AgentState) -> u8 {
    match state {
        AgentState::Blocked => 0,
        AgentState::Done => 1,
        _ => 2,
    }
}

pub fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins > 0 {
        format!("{mins}m{secs:02}s")
    } else {
        format!("{secs}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{AgentSnapshot, Snapshot, TabSnapshot, PROTOCOL_VERSION};
    use std::collections::BTreeSet;

    fn snapshot_fixture() -> Snapshot {
        Snapshot {
            v: PROTOCOL_VERSION,
            tabs: vec![
                TabSnapshot {
                    position: 0,
                    name: "main".into(),
                },
                TabSnapshot {
                    position: 1,
                    name: "work".into(),
                },
            ],
            agents: vec![
                agent(1, 0, AgentState::Working),
                agent(2, 0, AgentState::Blocked),
                agent(3, 1, AgentState::Done),
                agent(4, 1, AgentState::Idle),
            ],
            diagnostics: vec![],
        }
    }

    fn agent(pane_id: u32, tab: usize, state: AgentState) -> AgentSnapshot {
        AgentSnapshot {
            pane_id,
            tab_position: tab,
            agent: "claude".into(),
            agent_label: "Claude".into(),
            display: format!("d{pane_id}"),
            state,
            since_ms: 0,
            fallback_used: false,
        }
    }

    #[test]
    fn attention_rows_are_not_duplicated_in_tabs() {
        let rows = build_rows(&snapshot_fixture());
        let ids = rows
            .iter()
            .filter_map(|row| match row {
                Row::Agent(a) => Some(a.pane_id()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let unique = ids.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn attention_order_blocked_then_done() {
        let rows = build_rows(&snapshot_fixture());
        let attention: Vec<_> = rows
            .iter()
            .filter_map(|r| match r {
                Row::Agent(a) if a.in_attention => Some(a.state),
                _ => None,
            })
            .collect();
        assert_eq!(attention, vec![AgentState::Blocked, AgentState::Done]);
    }

    #[test]
    fn selection_moves_to_nearest_when_removed() {
        let mut selection = Selection::new();
        selection.set(Some(2));
        let old = vec![1, 2, 3];
        let new = vec![1, 3];
        selection.reconcile(&old, &new);
        assert_eq!(selection.pane_id(), Some(3));
    }

    #[test]
    fn selection_wraps() {
        let mut selection = Selection::new();
        selection.set(Some(1));
        let ids = vec![1, 2, 3];
        selection.previous(&ids);
        assert_eq!(selection.pane_id(), Some(3));
        selection.next(&ids);
        assert_eq!(selection.pane_id(), Some(1));
    }
}
