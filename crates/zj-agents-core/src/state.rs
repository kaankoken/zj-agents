use crate::model::{AgentState, Observation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttentionKind {
    Blocked,
    Done,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StateChange {
    pub changed: bool,
    pub generation: u64,
    pub attention: Option<AttentionKind>,
}

#[derive(Clone, Debug)]
pub struct PaneState {
    state: AgentState,
    baseline_pending: bool,
    completion_pending: bool,
    consecutive_read_failures: u8,
    transition_generation: u64,
    since_ms: u64,
}

impl PaneState {
    pub fn new() -> Self {
        Self {
            state: AgentState::Unknown,
            baseline_pending: true,
            completion_pending: false,
            consecutive_read_failures: 0,
            transition_generation: 0,
            since_ms: 0,
        }
    }

    #[cfg(test)]
    pub fn baseline(state: AgentState) -> Self {
        Self {
            state,
            baseline_pending: false,
            completion_pending: false,
            consecutive_read_failures: 0,
            transition_generation: 1,
            since_ms: 0,
        }
    }

    pub fn state(&self) -> AgentState {
        self.state
    }

    pub fn baseline_pending(&self) -> bool {
        self.baseline_pending
    }

    pub fn generation(&self) -> u64 {
        self.transition_generation
    }

    pub fn since_ms(&self) -> u64 {
        self.since_ms
    }

    pub fn set_baseline_pending(&mut self) {
        self.baseline_pending = true;
        self.completion_pending = false;
        self.consecutive_read_failures = 0;
    }

    pub fn advance(&mut self, elapsed_ms: u64) {
        self.since_ms = self.since_ms.saturating_add(elapsed_ms);
    }

    pub fn observe(&mut self, observation: Observation, focused: bool) -> StateChange {
        self.clear_done_on_focus(focused);
        self.consecutive_read_failures = 0;
        self.apply_observation(observation, focused)
    }

    pub fn read_failed(&mut self, focused: bool) -> StateChange {
        self.clear_done_on_focus(focused);
        self.completion_pending = false;
        self.consecutive_read_failures = self.consecutive_read_failures.saturating_add(1);
        if self.consecutive_read_failures >= 3 {
            let previous = self.state;
            self.state = AgentState::Unknown;
            self.baseline_pending = true;
            self.completion_pending = false;
            if previous != AgentState::Unknown {
                return self.record_change(None);
            }
            return StateChange {
                changed: false,
                generation: self.transition_generation,
                attention: None,
            };
        }
        StateChange {
            changed: false,
            generation: self.transition_generation,
            attention: None,
        }
    }

    fn clear_done_on_focus(&mut self, focused: bool) {
        if focused && self.state == AgentState::Done {
            self.state = AgentState::Idle;
            self.completion_pending = false;
            self.transition_generation = self.transition_generation.saturating_add(1);
            self.since_ms = 0;
        }
    }

    fn apply_observation(&mut self, observation: Observation, focused: bool) -> StateChange {
        if self.baseline_pending {
            let next = AgentState::from(observation);
            self.baseline_pending = false;
            self.completion_pending = false;
            if next != self.state {
                return self.record_change_to(next, None);
            }
            return StateChange {
                changed: false,
                generation: self.transition_generation,
                attention: None,
            };
        }

        match (self.state, observation) {
            (AgentState::Unknown, Observation::Unknown) => self.stable(),
            (AgentState::Unknown, Observation::Idle) => {
                self.record_change_to(AgentState::Idle, None)
            }
            (AgentState::Unknown, Observation::Working) => {
                self.record_change_to(AgentState::Working, None)
            }
            (AgentState::Unknown, Observation::Blocked) => self.attention_blocked(focused),

            (AgentState::Idle, Observation::Working) => {
                self.record_change_to(AgentState::Working, None)
            }
            (AgentState::Idle, Observation::Blocked) => self.attention_blocked(focused),
            (AgentState::Idle, Observation::Idle) => self.stable(),
            (AgentState::Idle, Observation::Unknown) => {
                self.record_change_to(AgentState::Unknown, None)
            }

            (AgentState::Working, Observation::Working) => {
                self.completion_pending = false;
                self.stable()
            }
            (AgentState::Working, Observation::Idle) if focused => {
                self.completion_pending = false;
                self.record_change_to(AgentState::Idle, None)
            }
            (AgentState::Working, Observation::Idle) if !self.completion_pending => {
                self.completion_pending = true;
                self.stable()
            }
            (AgentState::Working, Observation::Idle) => {
                self.completion_pending = false;
                self.record_change_to(AgentState::Done, Some(AttentionKind::Done))
            }
            (AgentState::Working, Observation::Blocked) => self.attention_blocked(focused),
            (AgentState::Working, Observation::Unknown) => {
                self.completion_pending = false;
                self.record_change_to(AgentState::Unknown, None)
            }

            (AgentState::Blocked, Observation::Working) => {
                self.record_change_to(AgentState::Working, None)
            }
            (AgentState::Blocked, Observation::Idle) => {
                self.record_change_to(AgentState::Idle, None)
            }
            (AgentState::Blocked, Observation::Blocked) => self.stable(),
            (AgentState::Blocked, Observation::Unknown) => {
                self.record_change_to(AgentState::Unknown, None)
            }

            (AgentState::Done, Observation::Idle | Observation::Unknown) if !focused => {
                self.stable()
            }
            (AgentState::Done, Observation::Working) => {
                self.record_change_to(AgentState::Working, None)
            }
            (AgentState::Done, Observation::Blocked) => self.attention_blocked(focused),
            (AgentState::Done, Observation::Idle | Observation::Unknown) => {
                // focused Done already cleared before observe; keep Idle path safe
                self.record_change_to(AgentState::from(observation), None)
            }
        }
    }

    fn attention_blocked(&mut self, focused: bool) -> StateChange {
        self.completion_pending = false;
        let attention = if focused {
            None
        } else {
            Some(AttentionKind::Blocked)
        };
        if self.state != AgentState::Blocked {
            self.record_change_to(AgentState::Blocked, attention)
        } else {
            self.stable()
        }
    }

    fn stable(&self) -> StateChange {
        StateChange {
            changed: false,
            generation: self.transition_generation,
            attention: None,
        }
    }

    fn record_change_to(
        &mut self,
        next: AgentState,
        attention: Option<AttentionKind>,
    ) -> StateChange {
        self.state = next;
        self.record_change(attention)
    }

    fn record_change(&mut self, attention: Option<AttentionKind>) -> StateChange {
        self.transition_generation = self.transition_generation.saturating_add(1);
        self.since_ms = 0;
        StateChange {
            changed: true,
            generation: self.transition_generation,
            attention,
        }
    }
}

impl Default for PaneState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_observation_is_a_notification_free_baseline() {
        let mut pane = PaneState::new();
        let change = pane.observe(Observation::Blocked, false);
        assert_eq!(pane.state(), AgentState::Blocked);
        assert_eq!(change.attention, None);
        assert!(!pane.baseline_pending());
    }

    #[test]
    fn focused_working_to_idle_bypasses_done() {
        let mut pane = PaneState::baseline(AgentState::Working);
        pane.observe(Observation::Idle, true);
        assert_eq!(pane.state(), AgentState::Idle);
    }

    #[test]
    fn two_unfocused_idle_observations_derive_done() {
        let mut pane = PaneState::baseline(AgentState::Working);
        assert_eq!(pane.observe(Observation::Idle, false).attention, None);
        let change = pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Done);
        assert_eq!(change.attention, Some(AttentionKind::Done));
    }

    #[test]
    fn successful_unknown_is_not_operational_recovery() {
        let mut pane = PaneState::baseline(AgentState::Idle);
        pane.observe(Observation::Unknown, false);
        assert_eq!(pane.state(), AgentState::Unknown);
        assert!(!pane.baseline_pending());
        assert_eq!(
            pane.observe(Observation::Blocked, false).attention,
            Some(AttentionKind::Blocked)
        );
    }

    #[test]
    fn third_read_failure_enters_baselined_operational_unknown() {
        let mut pane = PaneState::baseline(AgentState::Done);
        pane.read_failed(false);
        pane.read_failed(false);
        pane.read_failed(false);
        assert_eq!(pane.state(), AgentState::Unknown);
        assert!(pane.baseline_pending());
        assert_eq!(pane.observe(Observation::Blocked, false).attention, None);
    }

    #[test]
    fn focus_clears_done_even_when_the_read_fails() {
        let mut pane = PaneState::baseline(AgentState::Done);
        pane.read_failed(true);
        assert_eq!(pane.state(), AgentState::Idle);
    }

    #[test]
    fn sticky_done_survives_idle_and_unknown() {
        let mut pane = PaneState::baseline(AgentState::Working);
        pane.observe(Observation::Idle, false);
        pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Done);
        pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Done);
        pane.observe(Observation::Unknown, false);
        assert_eq!(pane.state(), AgentState::Done);
    }

    #[test]
    fn working_supersedes_sticky_done() {
        let mut pane = PaneState::baseline(AgentState::Working);
        pane.observe(Observation::Idle, false);
        pane.observe(Observation::Idle, false);
        pane.observe(Observation::Working, false);
        assert_eq!(pane.state(), AgentState::Working);
    }

    #[test]
    fn focused_blocked_suppresses_attention() {
        let mut pane = PaneState::baseline(AgentState::Idle);
        let change = pane.observe(Observation::Blocked, true);
        assert_eq!(pane.state(), AgentState::Blocked);
        assert_eq!(change.attention, None);
    }

    #[test]
    fn generation_increments_on_state_change() {
        let mut pane = PaneState::baseline(AgentState::Idle);
        let before = pane.generation();
        let change = pane.observe(Observation::Working, false);
        assert!(change.changed);
        assert_eq!(change.generation, before + 1);
        assert_eq!(pane.since_ms(), 0);
    }

    #[test]
    fn advance_only_while_unchanged() {
        let mut pane = PaneState::baseline(AgentState::Idle);
        pane.advance(500);
        assert_eq!(pane.since_ms(), 500);
        pane.observe(Observation::Working, false);
        assert_eq!(pane.since_ms(), 0);
    }

    #[test]
    fn non_idle_cancels_completion_pending() {
        let mut pane = PaneState::baseline(AgentState::Working);
        pane.observe(Observation::Idle, false);
        pane.observe(Observation::Working, false);
        let change = pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Working);
        assert_eq!(change.attention, None);
    }

    #[test]
    fn transition_table_unknown_paths() {
        let mut pane = PaneState::baseline(AgentState::Unknown);
        assert!(!pane.observe(Observation::Unknown, false).changed);
        pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Idle);
        pane.observe(Observation::Unknown, false);
        assert_eq!(pane.state(), AgentState::Unknown);
        pane.observe(Observation::Working, false);
        assert_eq!(pane.state(), AgentState::Working);
    }

    #[test]
    fn blocked_to_working_and_idle() {
        let mut pane = PaneState::baseline(AgentState::Blocked);
        pane.observe(Observation::Working, false);
        assert_eq!(pane.state(), AgentState::Working);
        let mut pane = PaneState::baseline(AgentState::Blocked);
        pane.observe(Observation::Idle, false);
        assert_eq!(pane.state(), AgentState::Idle);
    }
}
