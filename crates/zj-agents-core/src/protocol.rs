use serde::{Deserialize, Serialize};

use crate::model::{AgentState, DiagnosticSource};

pub const PROTOCOL_VERSION: u8 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub v: u8,
}

impl Hello {
    pub const fn v1() -> Self {
        Self {
            v: PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reload {
    pub v: u8,
}

impl Reload {
    pub const fn v1() -> Self {
        Self {
            v: PROTOCOL_VERSION,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub v: u8,
    pub tabs: Vec<TabSnapshot>,
    pub agents: Vec<AgentSnapshot>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TabSnapshot {
    pub position: usize,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentSnapshot {
    pub pane_id: u32,
    pub tab_position: usize,
    pub agent: String,
    pub agent_label: String,
    pub display: String,
    pub state: AgentState,
    pub since_ms: u64,
    pub fallback_used: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub source: DiagnosticSource,
    pub message: String,
}

impl Diagnostic {
    pub fn new(source: DiagnosticSource, message: impl Into<String>) -> Self {
        Self {
            source,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_uses_version_one_and_structured_diagnostics() {
        let snapshot = Snapshot {
            v: PROTOCOL_VERSION,
            tabs: vec![TabSnapshot {
                position: 0,
                name: "work".into(),
            }],
            agents: vec![AgentSnapshot {
                pane_id: 7,
                tab_position: 0,
                agent: "claude".into(),
                agent_label: "Claude Code".into(),
                display: "repo".into(),
                state: AgentState::Working,
                since_ms: 1_500,
                fallback_used: false,
            }],
            diagnostics: vec![Diagnostic {
                source: DiagnosticSource::Manifest,
                message: "invalid override manifest".into(),
            }],
        };

        let value = serde_json::to_value(snapshot).unwrap();
        assert_eq!(value["v"], 1);
        assert_eq!(value["agents"][0]["state"], "working");
        assert_eq!(value["diagnostics"][0]["source"], "manifest");
        assert!(value.get("diagnostic").is_none());
    }

    #[test]
    fn hello_and_reload_are_versioned() {
        assert_eq!(serde_json::to_string(&Hello::v1()).unwrap(), r#"{"v":1}"#);
        assert_eq!(serde_json::to_string(&Reload::v1()).unwrap(), r#"{"v":1}"#);
    }
}
