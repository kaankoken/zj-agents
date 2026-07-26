use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    #[default]
    Unknown,
    Idle,
    Working,
    Blocked,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    Unknown,
    Idle,
    Working,
    Blocked,
}

impl From<Observation> for AgentState {
    fn from(value: Observation) -> Self {
        match value {
            Observation::Unknown => Self::Unknown,
            Observation::Idle => Self::Idle,
            Observation::Working => Self::Working,
            Observation::Blocked => Self::Blocked,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSource {
    Inventory,
    Manifest,
    Host,
    Notification,
    Protocol,
    Detection,
}

impl AgentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Idle => "idle",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Done => "done",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            Self::Unknown => "?",
            Self::Idle => "·",
            Self::Working => "›",
            Self::Blocked => "!",
            Self::Done => "✓",
        }
    }
}
