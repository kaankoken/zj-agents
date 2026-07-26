use std::collections::BTreeMap;

use serde::Deserialize;
use zj_agents_core::sanitize::sanitize_metadata;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalCandidate {
    pub pane_id: u32,
    pub tab_position: usize,
    pub tab_name: String,
    pub title: String,
}

#[derive(Clone, Debug, Default)]
pub struct Inventory {
    event_rows: BTreeMap<u32, TerminalCandidate>,
}

impl Inventory {
    pub fn apply_event_manifest(&mut self, rows: Vec<TerminalCandidate>) {
        self.event_rows = rows.into_iter().map(|r| (r.pane_id, r)).collect();
    }

    pub fn merge_cli_candidates(&mut self, candidates: Vec<TerminalCandidate>) {
        for candidate in candidates {
            self.event_rows
                .entry(candidate.pane_id)
                .or_insert(candidate);
        }
    }

    pub fn remove(&mut self, pane_id: u32) {
        self.event_rows.remove(&pane_id);
    }

    pub fn get(&self, pane_id: u32) -> Option<&TerminalCandidate> {
        self.event_rows.get(&pane_id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &TerminalCandidate> {
        self.event_rows.values()
    }

    pub fn pane_ids(&self) -> Vec<u32> {
        self.event_rows.keys().copied().collect()
    }

    pub fn update_title(&mut self, pane_id: u32, title: String) {
        if let Some(row) = self.event_rows.get_mut(&pane_id) {
            row.title = sanitize_metadata(&title, 60);
        }
    }

    pub fn update_tab(&mut self, pane_id: u32, tab_position: usize, tab_name: String) {
        if let Some(row) = self.event_rows.get_mut(&pane_id) {
            row.tab_position = tab_position;
            row.tab_name = sanitize_metadata(&tab_name, 60);
        }
    }
}

#[derive(Deserialize)]
struct PaneListRow {
    id: u32,
    is_plugin: bool,
    exited: bool,
    #[serde(default)]
    tab_position: usize,
    #[serde(default)]
    tab_name: String,
    #[serde(default)]
    title: String,
}

#[derive(Debug)]
pub struct PaneListError;

pub fn parse_pane_list_json(stdout: &[u8]) -> Result<Vec<TerminalCandidate>, PaneListError> {
    let text = std::str::from_utf8(stdout).map_err(|_| PaneListError)?;
    let rows: Vec<PaneListRow> = serde_json::from_str(text).map_err(|_| PaneListError)?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.is_plugin && !r.exited)
        .map(|r| TerminalCandidate {
            pane_id: r.id,
            tab_position: r.tab_position,
            tab_name: sanitize_metadata(&r.tab_name, 60),
            title: sanitize_metadata(&r.title, 60),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"[
      {
        "id": 1,
        "is_plugin": false,
        "exited": false,
        "tab_position": 0,
        "tab_name": "main",
        "title": "shell",
        "pane_command": "ignored",
        "pane_cwd": "/tmp",
        "future_field": true
      },
      {
        "id": 1,
        "is_plugin": true,
        "exited": false,
        "tab_position": 0,
        "tab_name": "main",
        "title": "plugin"
      },
      {
        "id": 2,
        "is_plugin": false,
        "exited": true,
        "tab_position": 0,
        "tab_name": "main",
        "title": "dead"
      }
    ]"#;

    #[test]
    fn parses_only_live_terminals() {
        let rows = parse_pane_list_json(FIXTURE.as_bytes()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            TerminalCandidate {
                pane_id: 1,
                tab_position: 0,
                tab_name: "main".into(),
                title: "shell".into(),
            }
        );
    }

    #[test]
    fn event_rows_win_over_cli() {
        let mut inv = Inventory::default();
        inv.apply_event_manifest(vec![TerminalCandidate {
            pane_id: 1,
            tab_position: 0,
            tab_name: "event".into(),
            title: "new".into(),
        }]);
        inv.merge_cli_candidates(vec![TerminalCandidate {
            pane_id: 1,
            tab_position: 9,
            tab_name: "cli".into(),
            title: "old".into(),
        }]);
        assert_eq!(inv.get(1).unwrap().tab_name, "event");
        inv.merge_cli_candidates(vec![TerminalCandidate {
            pane_id: 2,
            tab_position: 1,
            tab_name: "cli".into(),
            title: "extra".into(),
        }]);
        assert!(inv.get(2).is_some());
    }

    #[test]
    fn remove_drops_membership() {
        let mut inv = Inventory::default();
        inv.apply_event_manifest(vec![TerminalCandidate {
            pane_id: 3,
            tab_position: 0,
            tab_name: "t".into(),
            title: "x".into(),
        }]);
        inv.remove(3);
        assert!(inv.get(3).is_none());
    }
}
