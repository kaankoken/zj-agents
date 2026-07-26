use std::collections::{BTreeMap, BTreeSet};

use zj_agents_core::manifest::{bundled_manifests, classify, detect, CompiledManifest, Detection};
use zj_agents_core::model::{AgentState, DiagnosticSource};
use zj_agents_core::notification::{
    default_notify_argv, parse_host_os, AttentionQueue, DiagnosticSlots, NotifyTemplate,
    PendingAttention,
};
use zj_agents_core::protocol::{
    AgentSnapshot, Diagnostic, Hello, Reload, Snapshot, TabSnapshot, PROTOCOL_VERSION,
};
use zj_agents_core::sanitize::{choose_display, sanitize_label, sanitize_metadata};
use zj_agents_core::state::{AttentionKind, PaneState};

use crate::inventory::{parse_pane_list_json, Inventory, TerminalCandidate};
use crate::overrides::{
    adopt_overrides, parse_manifest_frame, resolve_manifest_dir, ReloadController,
    READ_MANIFESTS_SH,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PermissionState {
    #[default]
    Pending,
    Granted,
    Denied,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum HostKind {
    PaneInventory,
    ManifestRead,
    HostDetect,
    Notify,
}

impl HostKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PaneInventory => "pane-inventory",
            Self::ManifestRead => "manifest-read",
            Self::HostDetect => "host-detect",
            Self::Notify => "notify",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pane-inventory" => Some(Self::PaneInventory),
            "manifest-read" => Some(Self::ManifestRead),
            "host-detect" => Some(Self::HostDetect),
            "notify" => Some(Self::Notify),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HostRequests {
    next_id: u64,
    active: BTreeMap<HostKind, String>,
    notify_active: BTreeSet<String>,
}

impl HostRequests {
    pub fn start(&mut self, kind: HostKind) -> Option<BTreeMap<String, String>> {
        if kind == HostKind::Notify {
            let id = self.alloc();
            self.notify_active.insert(id.clone());
            return Some(context(kind, &id));
        }
        if self.active.contains_key(&kind) {
            return None;
        }
        let id = self.alloc();
        self.active.insert(kind, id.clone());
        Some(context(kind, &id))
    }

    pub fn resolve(&mut self, ctx: &BTreeMap<String, String>) -> Option<HostKind> {
        let kind = HostKind::parse(ctx.get("kind")?)?;
        let request_id = ctx.get("request_id")?;
        if kind == HostKind::Notify {
            if self.notify_active.remove(request_id) {
                return Some(HostKind::Notify);
            }
            return None;
        }
        match self.active.get(&kind) {
            Some(active) if active == request_id => {
                self.active.remove(&kind);
                Some(kind)
            }
            _ => None,
        }
    }

    fn alloc(&mut self) -> String {
        self.next_id = self.next_id.saturating_add(1);
        self.next_id.to_string()
    }
}

fn context(kind: HostKind, request_id: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    map.insert("kind".into(), kind.as_str().into());
    map.insert("request_id".into(), request_id.into());
    map
}

#[derive(Clone, Debug)]
pub enum Effect {
    RequestInventory {
        argv: Vec<String>,
        context: BTreeMap<String, String>,
    },
    ReadOverrides {
        argv: Vec<String>,
        env: BTreeMap<String, String>,
        context: BTreeMap<String, String>,
    },
    DetectHost {
        argv: Vec<String>,
        context: BTreeMap<String, String>,
    },
    Notify {
        argv: Vec<String>,
        context: BTreeMap<String, String>,
    },
    SendSnapshot {
        plugin_id: u32,
        payload: String,
    },
}

#[derive(Clone, Debug)]
pub struct TimerPlan {
    pub elapsed_ms: u64,
    pub effects: Vec<Effect>,
    pub reconcile_pane_ids: Vec<u32>,
    pub viewport_pane_ids: Vec<u32>,
}

#[derive(Clone, Debug)]
pub enum HostError {
    Failed,
}

#[derive(Clone, Debug)]
pub struct TimerObservations {
    pub focused_pane_id: Option<u32>,
    pub reconciled_commands: BTreeMap<u32, Result<Vec<String>, HostError>>,
    pub viewports: BTreeMap<u32, Result<Vec<String>, HostError>>,
}

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub manifest_dir: Option<String>,
    pub notify: bool,
    pub notify_command: Option<NotifyTemplate>,
    pub notify_disabled_reason: Option<&'static str>,
}

impl EngineConfig {
    pub fn parse(configuration: BTreeMap<String, String>) -> Self {
        let mut notify = true;
        let mut notify_command = None;
        let mut notify_disabled_reason = None;
        let mut manifest_dir = None;

        if let Some(v) = configuration.get("manifest_dir") {
            if !v.is_empty() {
                manifest_dir = Some(v.clone());
            }
        }
        if let Some(v) = configuration.get("notify") {
            match v.as_str() {
                "true" => notify = true,
                "false" => notify = false,
                _ => {
                    notify = false;
                    notify_disabled_reason = Some("invalid notify");
                }
            }
        }
        if let Some(raw) = configuration.get("notify_command") {
            match NotifyTemplate::parse(raw) {
                Ok(t) => notify_command = Some(t),
                Err(_) => {
                    notify = false;
                    notify_disabled_reason = Some("invalid notify_command");
                }
            }
        }

        Self {
            manifest_dir,
            notify,
            notify_command,
            notify_disabled_reason,
        }
    }
}

#[derive(Clone, Debug)]
struct TrackedPane {
    agent: String,
    agent_label: String,
    display: String,
    tab_position: usize,
    state: PaneState,
    fallback_used: bool,
    cwd_basename: Option<String>,
    title: Option<String>,
}

pub struct Engine {
    permission: PermissionState,
    config: EngineConfig,
    manifests: Vec<CompiledManifest>,
    inventory: Inventory,
    panes: BTreeMap<u32, TrackedPane>,
    tabs: BTreeMap<usize, String>,
    sidebars: BTreeSet<u32>,
    diagnostics: DiagnosticSlots,
    attention: AttentionQueue,
    host_requests: HostRequests,
    reload: ReloadController,
    inventory_bootstrapped: bool,
    inventory_retry_ms: u64,
    reconcile_elapsed_ms: u64,
    last_sent_semantic: Option<Snapshot>,
    notify_template: Option<NotifyTemplate>,
    home: Option<String>,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Self {
        let mut diagnostics = DiagnosticSlots::default();
        if let Some(reason) = config.notify_disabled_reason {
            diagnostics.set(DiagnosticSource::Notification, reason);
        }
        let manifests = bundled_manifests().unwrap_or_default();
        Self {
            permission: PermissionState::Pending,
            config,
            manifests,
            inventory: Inventory::default(),
            panes: BTreeMap::new(),
            tabs: BTreeMap::new(),
            sidebars: BTreeSet::new(),
            diagnostics,
            attention: AttentionQueue::default(),
            host_requests: HostRequests::default(),
            reload: ReloadController::default(),
            inventory_bootstrapped: false,
            inventory_retry_ms: 0,
            reconcile_elapsed_ms: 0,
            last_sent_semantic: None,
            notify_template: None,
            home: std::env::var("HOME").ok(),
        }
    }

    pub fn permission_is_granted(&self) -> bool {
        self.permission == PermissionState::Granted
    }

    pub fn on_permission(&mut self, granted: bool) -> Vec<Effect> {
        if self.permission != PermissionState::Pending {
            return Vec::new();
        }
        if granted {
            self.permission = PermissionState::Granted;
            let mut effects = Vec::new();
            effects.extend(self.start_inventory_request());
            effects.extend(self.start_manifest_read());
            if self.config.notify && self.config.notify_command.is_none() {
                effects.extend(self.start_host_detect());
            } else if self.config.notify {
                self.notify_template = self.config.notify_command.clone();
            }
            effects
        } else {
            self.permission = PermissionState::Denied;
            Vec::new()
        }
    }

    pub fn begin_timer(&mut self, elapsed_ms: u64) -> Option<TimerPlan> {
        if !self.permission_is_granted() {
            return None;
        }
        for pane in self.panes.values_mut() {
            pane.state.advance(elapsed_ms);
        }

        let mut effects = Vec::new();
        if !self.inventory_bootstrapped {
            self.inventory_retry_ms = self.inventory_retry_ms.saturating_add(elapsed_ms);
            if self.inventory_retry_ms >= 5_000 {
                self.inventory_retry_ms = 0;
                effects.extend(self.start_inventory_request());
            }
        }

        self.reconcile_elapsed_ms = self.reconcile_elapsed_ms.saturating_add(elapsed_ms);
        let mut reconcile_pane_ids = Vec::new();
        if self.reconcile_elapsed_ms >= 5_000 {
            self.reconcile_elapsed_ms %= 5_000;
            reconcile_pane_ids = self.inventory.pane_ids();
        }

        let viewport_pane_ids = self.panes.keys().copied().collect();
        Some(TimerPlan {
            elapsed_ms,
            effects,
            reconcile_pane_ids,
            viewport_pane_ids,
        })
    }

    pub fn finish_timer(
        &mut self,
        plan: TimerPlan,
        observations: TimerObservations,
    ) -> Vec<Effect> {
        if !self.permission_is_granted() {
            return Vec::new();
        }
        let mut effects = plan.effects;
        let focused = observations.focused_pane_id;

        for (pane_id, cmd) in observations.reconciled_commands {
            if let Ok(argv) = cmd {
                self.apply_command(pane_id, &argv, true);
            }
        }

        if let Some(fid) = focused {
            self.attention.on_focused(fid);
        }

        for (pane_id, viewport) in observations.viewports {
            let Some(tracked) = self.panes.get_mut(&pane_id) else {
                continue;
            };
            let is_focused = focused == Some(pane_id);
            match viewport {
                Ok(lines) => {
                    let manifest = self
                        .manifests
                        .iter()
                        .find(|m| m.name() == tracked.agent)
                        .cloned();
                    if let Some(manifest) = manifest {
                        let classification = classify(&manifest, &lines);
                        tracked.fallback_used = classification.fallback_used;
                        let change = tracked
                            .state
                            .observe(classification.observation, is_focused);
                        if let Some(kind) = change.attention {
                            self.attention.enqueue(PendingAttention {
                                pane_id,
                                state: kind,
                                generation: change.generation,
                                label: tracked.display.clone(),
                            });
                        }
                    }
                }
                Err(_) => {
                    let change = tracked.state.read_failed(is_focused);
                    if change.changed {
                        self.attention.invalidate_if_stale(
                            pane_id,
                            change.generation,
                            AttentionKind::Done,
                        );
                    }
                }
            }
        }

        if let Some(events) = self.attention.advance(plan.elapsed_ms, |pending| {
            self.panes
                .get(&pending.pane_id)
                .map(|p| {
                    let state_ok = match pending.state {
                        AttentionKind::Blocked => p.state.state() == AgentState::Blocked,
                        AttentionKind::Done => p.state.state() == AgentState::Done,
                    };
                    state_ok
                        && p.state.generation() == pending.generation
                        && focused != Some(pending.pane_id)
                })
                .unwrap_or(false)
        }) {
            if let Some(template) = self.notify_template.clone() {
                let body = AttentionQueue::format_body(&events);
                if !body.is_empty() {
                    if let Some(ctx) = self.host_requests.start(HostKind::Notify) {
                        let argv = template.instantiate(&body);
                        effects.push(Effect::Notify { argv, context: ctx });
                    }
                }
            }
        }

        effects.extend(self.emit_snapshot_if_dirty());
        effects
    }

    pub fn on_command_changed(&mut self, pane_id: u32, argv: &[String], is_foreground: bool) {
        if !self.permission_is_granted() {
            return;
        }
        if !is_foreground {
            self.demote(pane_id);
            return;
        }
        self.apply_command(pane_id, argv, true);
    }

    pub fn on_pane_closed(&mut self, pane_id: u32) {
        self.inventory.remove(pane_id);
        self.panes.remove(&pane_id);
        self.attention.on_focused(pane_id);
    }

    pub fn on_event_inventory(&mut self, rows: Vec<TerminalCandidate>) {
        if !self.permission_is_granted() {
            return;
        }
        let rows: Vec<TerminalCandidate> = rows
            .into_iter()
            .map(|row| TerminalCandidate {
                pane_id: row.pane_id,
                tab_position: row.tab_position,
                tab_name: sanitize_metadata(&row.tab_name, 60),
                title: sanitize_metadata(&row.title, 60),
            })
            .collect();
        self.inventory.apply_event_manifest(rows);
        self.sync_tracked_from_inventory();
    }

    pub fn on_tab_update(&mut self, tabs: BTreeMap<usize, String>) {
        self.tabs = tabs
            .into_iter()
            .map(|(p, n)| (p, sanitize_metadata(&n, 60)))
            .collect();
    }

    pub fn on_cwd_changed(&mut self, pane_id: u32, basename: String) {
        if let Some(pane) = self.panes.get_mut(&pane_id) {
            let clean = sanitize_metadata(&basename, 60);
            pane.cwd_basename = if clean.is_empty() { None } else { Some(clean) };
            pane.display =
                choose_display(pane.cwd_basename.as_deref(), pane.title.as_deref(), pane_id);
        }
    }

    pub fn on_host_result(
        &mut self,
        context: &BTreeMap<String, String>,
        exit_code: Option<i32>,
        stdout: &[u8],
    ) -> Vec<Effect> {
        let Some(kind) = self.host_requests.resolve(context) else {
            return Vec::new();
        };
        match kind {
            HostKind::PaneInventory => self.handle_inventory_result(exit_code, stdout),
            HostKind::ManifestRead => self.handle_manifest_result(exit_code, stdout),
            HostKind::HostDetect => {
                self.handle_host_detect(exit_code, stdout);
                Vec::new()
            }
            HostKind::Notify => {
                if exit_code != Some(0) {
                    self.diagnostics.set(
                        DiagnosticSource::Notification,
                        "notification dispatch failed",
                    );
                } else {
                    self.diagnostics.clear(DiagnosticSource::Notification);
                }
                Vec::new()
            }
        }
    }

    pub fn on_hello(&mut self, plugin_id: u32, payload: &str) -> Vec<Effect> {
        if !self.permission_is_granted() {
            return Vec::new();
        }
        match serde_json::from_str::<Hello>(payload) {
            Ok(hello) if hello.v == PROTOCOL_VERSION => {
                self.diagnostics.clear(DiagnosticSource::Protocol);
                self.sidebars.insert(plugin_id);
                let payload = self.current_snapshot_json();
                vec![Effect::SendSnapshot { plugin_id, payload }]
            }
            Ok(_) => {
                self.diagnostics
                    .set(DiagnosticSource::Protocol, "unsupported protocol version");
                Vec::new()
            }
            Err(_) => {
                self.diagnostics
                    .set(DiagnosticSource::Protocol, "malformed hello");
                Vec::new()
            }
        }
    }

    pub fn on_reload(&mut self, payload: &str) -> Vec<Effect> {
        if !self.permission_is_granted() {
            return Vec::new();
        }
        match serde_json::from_str::<Reload>(payload) {
            Ok(reload) if reload.v == PROTOCOL_VERSION => {
                self.diagnostics.clear(DiagnosticSource::Protocol);
                if self.reload.request_start() {
                    self.start_manifest_read()
                } else {
                    Vec::new()
                }
            }
            Ok(_) => {
                self.diagnostics
                    .set(DiagnosticSource::Protocol, "unsupported protocol version");
                Vec::new()
            }
            Err(_) => {
                self.diagnostics
                    .set(DiagnosticSource::Protocol, "malformed reload");
                Vec::new()
            }
        }
    }

    pub fn on_unknown_pipe(&mut self) {
        self.diagnostics
            .set(DiagnosticSource::Protocol, "unknown pipe message");
    }

    fn apply_command(&mut self, pane_id: u32, argv: &[String], allow_promote: bool) {
        let decision = match detect(&self.manifests, argv) {
            Detection::One(manifest) => {
                Some((manifest.name().to_owned(), manifest.label().to_owned()))
            }
            Detection::None => None,
            Detection::Ambiguous(names) => {
                let joined = names.join(",");
                self.demote(pane_id);
                self.diagnostics.set(
                    DiagnosticSource::Detection,
                    format!("ambiguous manifests: {joined}"),
                );
                return;
            }
        };
        match decision {
            Some((name, label)) => {
                if allow_promote {
                    self.promote_named(pane_id, &name, &label);
                }
                self.diagnostics.clear(DiagnosticSource::Detection);
            }
            None => {
                self.demote(pane_id);
            }
        }
    }

    fn promote_named(&mut self, pane_id: u32, name: &str, label: &str) {
        if self.panes.contains_key(&pane_id) {
            if let Some(p) = self.panes.get_mut(&pane_id) {
                if p.agent != name {
                    p.agent = name.to_owned();
                    p.agent_label = sanitize_label(label, 60, pane_id);
                    p.state.set_baseline_pending();
                }
            }
            return;
        }
        let inv = self.inventory.get(pane_id);
        let tab_position = inv.map(|c| c.tab_position).unwrap_or(0);
        let title = inv.map(|c| c.title.clone());
        let display = choose_display(None, title.as_deref(), pane_id);
        self.panes.insert(
            pane_id,
            TrackedPane {
                agent: name.to_owned(),
                agent_label: sanitize_label(label, 60, pane_id),
                display,
                tab_position,
                state: PaneState::new(),
                fallback_used: false,
                cwd_basename: None,
                title,
            },
        );
    }

    fn demote(&mut self, pane_id: u32) {
        self.panes.remove(&pane_id);
        self.attention.on_focused(pane_id);
    }

    fn start_inventory_request(&mut self) -> Vec<Effect> {
        let Some(context) = self.host_requests.start(HostKind::PaneInventory) else {
            return Vec::new();
        };
        vec![Effect::RequestInventory {
            argv: vec![
                "zellij".into(),
                "action".into(),
                "list-panes".into(),
                "--all".into(),
                "--json".into(),
            ],
            context,
        }]
    }

    fn start_manifest_read(&mut self) -> Vec<Effect> {
        let Some(context) = self.host_requests.start(HostKind::ManifestRead) else {
            return Vec::new();
        };
        let dir =
            match resolve_manifest_dir(self.config.manifest_dir.as_deref(), self.home.as_deref()) {
                Ok(p) => p,
                Err(_) => {
                    self.diagnostics
                        .set(DiagnosticSource::Manifest, "invalid manifest_dir");
                    return Vec::new();
                }
            };
        let mut env = BTreeMap::new();
        env.insert("ZJA_DIR".into(), dir.display().to_string());
        vec![Effect::ReadOverrides {
            argv: vec!["sh".into(), "-c".into(), READ_MANIFESTS_SH.trim().into()],
            env,
            context,
        }]
    }

    fn start_host_detect(&mut self) -> Vec<Effect> {
        let Some(context) = self.host_requests.start(HostKind::HostDetect) else {
            return Vec::new();
        };
        vec![Effect::DetectHost {
            argv: vec!["uname".into(), "-s".into()],
            context,
        }]
    }

    fn handle_inventory_result(&mut self, exit_code: Option<i32>, stdout: &[u8]) -> Vec<Effect> {
        if exit_code != Some(0) {
            self.diagnostics
                .set(DiagnosticSource::Inventory, "list-panes failed");
            return Vec::new();
        }
        match parse_pane_list_json(stdout) {
            Ok(rows) => {
                self.diagnostics.clear(DiagnosticSource::Inventory);
                self.inventory_bootstrapped = true;
                self.inventory.merge_cli_candidates(rows);
            }
            Err(_) => {
                self.diagnostics
                    .set(DiagnosticSource::Inventory, "list-panes invalid");
            }
        }
        Vec::new()
    }

    fn handle_manifest_result(&mut self, exit_code: Option<i32>, stdout: &[u8]) -> Vec<Effect> {
        let follow_up = self.reload.complete();
        let mut effects = Vec::new();
        if exit_code != Some(0) {
            self.diagnostics
                .set(DiagnosticSource::Manifest, "override read failed");
        } else {
            match parse_manifest_frame(stdout) {
                Ok(records) => match adopt_overrides(&records) {
                    Ok(set) => {
                        self.diagnostics.clear(DiagnosticSource::Manifest);
                        self.manifests = set;
                        for pane in self.panes.values_mut() {
                            pane.state.set_baseline_pending();
                        }
                        self.attention = AttentionQueue::default();
                    }
                    Err(_) => {
                        self.diagnostics
                            .set(DiagnosticSource::Manifest, "invalid override manifest");
                    }
                },
                Err(_) => {
                    self.diagnostics
                        .set(DiagnosticSource::Manifest, "invalid override framing");
                }
            }
        }
        if follow_up {
            effects.extend(self.start_manifest_read());
        }
        effects.extend(self.emit_snapshot_if_dirty());
        effects
    }

    fn sync_tracked_from_inventory(&mut self) {
        let pane_ids: Vec<u32> = self.panes.keys().copied().collect();
        for pane_id in pane_ids {
            let Some(candidate) = self.inventory.get(pane_id).cloned() else {
                continue;
            };
            let Some(pane) = self.panes.get_mut(&pane_id) else {
                continue;
            };
            pane.tab_position = candidate.tab_position;
            pane.title = if candidate.title.is_empty() {
                None
            } else {
                Some(candidate.title)
            };
            pane.display =
                choose_display(pane.cwd_basename.as_deref(), pane.title.as_deref(), pane_id);
        }
    }

    fn handle_host_detect(&mut self, exit_code: Option<i32>, stdout: &[u8]) {
        if exit_code != Some(0) {
            self.config.notify = false;
            self.diagnostics
                .set(DiagnosticSource::Host, "host detection failed");
            return;
        }
        let text = String::from_utf8_lossy(stdout);
        match parse_host_os(&text) {
            Some(os) => {
                self.diagnostics.clear(DiagnosticSource::Host);
                self.notify_template = NotifyTemplate::from_argv(default_notify_argv(os)).ok();
            }
            None => {
                self.config.notify = false;
                self.diagnostics
                    .set(DiagnosticSource::Host, "unsupported host");
            }
        }
    }

    fn build_snapshot(&self) -> Snapshot {
        let tabs = self
            .tabs
            .iter()
            .map(|(position, name)| TabSnapshot {
                position: *position,
                name: name.clone(),
            })
            .collect();
        let agents = self
            .panes
            .iter()
            .map(|(pane_id, pane)| AgentSnapshot {
                pane_id: *pane_id,
                tab_position: pane.tab_position,
                agent: pane.agent.clone(),
                agent_label: pane.agent_label.clone(),
                display: pane.display.clone(),
                state: pane.state.state(),
                since_ms: pane.state.since_ms(),
                fallback_used: pane.fallback_used,
            })
            .collect();
        Snapshot {
            v: PROTOCOL_VERSION,
            tabs,
            agents,
            diagnostics: self.diagnostics.snapshot(),
        }
    }

    fn semantic_key(snapshot: &Snapshot) -> Snapshot {
        let mut key = snapshot.clone();
        for agent in &mut key.agents {
            agent.since_ms = 0;
        }
        key
    }

    fn current_snapshot_json(&self) -> String {
        serde_json::to_string(&self.build_snapshot()).unwrap_or_else(|_| {
            serde_json::to_string(&Snapshot {
                v: PROTOCOL_VERSION,
                tabs: vec![],
                agents: vec![],
                diagnostics: vec![Diagnostic::new(
                    DiagnosticSource::Protocol,
                    "snapshot encode failed",
                )],
            })
            .unwrap()
        })
    }

    fn emit_snapshot_if_dirty(&mut self) -> Vec<Effect> {
        let snapshot = self.build_snapshot();
        let key = Self::semantic_key(&snapshot);
        if self.last_sent_semantic.as_ref() == Some(&key) {
            return Vec::new();
        }
        self.last_sent_semantic = Some(key);
        let payload = serde_json::to_string(&snapshot).unwrap_or_default();
        self.sidebars
            .iter()
            .map(|plugin_id| Effect::SendSnapshot {
                plugin_id: *plugin_id,
                payload: payload.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod host_request_tests {
    use super::*;

    #[test]
    fn ids_increase_and_resolve_once() {
        let mut hr = HostRequests::default();
        let a = hr.start(HostKind::PaneInventory).unwrap();
        assert_eq!(a.get("kind").unwrap(), "pane-inventory");
        assert_eq!(a.get("request_id").unwrap(), "1");
        assert!(hr.start(HostKind::PaneInventory).is_none());
        assert_eq!(hr.resolve(&a), Some(HostKind::PaneInventory));
        assert_eq!(hr.resolve(&a), None);
    }

    #[test]
    fn notify_may_overlap() {
        let mut hr = HostRequests::default();
        let a = hr.start(HostKind::Notify).unwrap();
        let b = hr.start(HostKind::Notify).unwrap();
        assert_ne!(a.get("request_id"), b.get("request_id"));
        assert_eq!(hr.resolve(&a), Some(HostKind::Notify));
        assert_eq!(hr.resolve(&b), Some(HostKind::Notify));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_denied_is_inert() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        assert!(engine.on_permission(false).is_empty());
        assert!(engine.begin_timer(1000).is_none());
        assert!(engine.on_hello(1, r#"{"v":1}"#).is_empty());
    }

    #[test]
    fn granted_starts_bootstrap_effects() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        let effects = engine.on_permission(true);
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::RequestInventory { .. })));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::ReadOverrides { .. })));
        assert!(effects
            .iter()
            .any(|e| matches!(e, Effect::DetectHost { .. })));
    }

    #[test]
    fn second_permission_ignored() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        engine.on_permission(true);
        assert!(engine.on_permission(false).is_empty());
        assert!(engine.permission_is_granted());
    }

    #[test]
    fn config_parses_only_three_keys() {
        let mut map = BTreeMap::new();
        map.insert("notify".into(), "false".into());
        map.insert("extra".into(), "x".into());
        let cfg = EngineConfig::parse(map);
        assert!(!cfg.notify);
    }

    #[test]
    fn hello_registers_sidebar_and_replies() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        engine.on_permission(true);
        let effects = engine.on_hello(7, r#"{"v":1}"#);
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::SendSnapshot { plugin_id, payload } => {
                assert_eq!(*plugin_id, 7);
                let snap: Snapshot = serde_json::from_str(payload).unwrap();
                assert_eq!(snap.v, 1);
            }
            _ => panic!("expected snapshot"),
        }
    }

    #[test]
    fn reconcile_due_every_five_seconds() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        engine.on_permission(true);
        engine
            .inventory
            .apply_event_manifest(vec![TerminalCandidate {
                pane_id: 4,
                tab_position: 0,
                tab_name: "t".into(),
                title: "x".into(),
            }]);
        let plan = engine.begin_timer(4_999).unwrap();
        assert!(plan.reconcile_pane_ids.is_empty());
        let plan = engine.begin_timer(1).unwrap();
        assert_eq!(plan.reconcile_pane_ids, vec![4]);
    }

    #[test]
    fn promotion_and_classification_baseline() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        engine.on_permission(true);
        engine
            .inventory
            .apply_event_manifest(vec![TerminalCandidate {
                pane_id: 9,
                tab_position: 0,
                tab_name: "main".into(),
                title: "claude".into(),
            }]);
        engine.on_command_changed(9, &["claude".into()], true);
        assert!(engine.panes.contains_key(&9));
        let plan = engine.begin_timer(1000).unwrap();
        let mut viewports = BTreeMap::new();
        viewports.insert(
            9,
            Ok(vec!["Do you want to proceed?".into(), "Allow?".into()]),
        );
        let effects = engine.finish_timer(
            plan,
            TimerObservations {
                focused_pane_id: None,
                reconciled_commands: BTreeMap::new(),
                viewports,
            },
        );
        let _ = effects;
        assert_eq!(
            engine.panes.get(&9).unwrap().state.state(),
            AgentState::Blocked
        );
    }

    #[test]
    fn inventory_failure_then_success_clears_slot() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        let effects = engine.on_permission(true);
        let ctx = effects
            .iter()
            .find_map(|e| match e {
                Effect::RequestInventory { context, .. } => Some(context.clone()),
                _ => None,
            })
            .expect("bootstrap inventory request");
        engine.on_host_result(&ctx, Some(1), b"");
        assert!(engine
            .diagnostics
            .snapshot()
            .iter()
            .any(|d| d.source == DiagnosticSource::Inventory));
        let ctx = engine.host_requests.start(HostKind::PaneInventory).unwrap();
        engine.on_host_result(&ctx, Some(0), b"[]");
        assert!(!engine
            .diagnostics
            .snapshot()
            .iter()
            .any(|d| d.source == DiagnosticSource::Inventory));
    }

    #[test]
    fn notify_title_constant() {
        assert_eq!(
            zj_agents_core::notification::NOTIFICATION_TITLE,
            "zj-agents"
        );
    }

    #[test]
    fn manifest_read_nonzero_exit_empty_stdout_preserves_manifests() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        let effects = engine.on_permission(true);
        let before_names: Vec<String> = engine.manifests.iter().map(|m| m.name().into()).collect();
        assert!(!before_names.is_empty());

        let ctx = effects
            .iter()
            .find_map(|e| match e {
                Effect::ReadOverrides { context, .. } => Some(context.clone()),
                _ => None,
            })
            .expect("bootstrap manifest-read request");

        engine.on_host_result(&ctx, Some(1), b"");
        let after_names: Vec<String> = engine.manifests.iter().map(|m| m.name().into()).collect();
        assert_eq!(before_names, after_names);
        assert!(engine
            .diagnostics
            .snapshot()
            .iter()
            .any(|d| d.source == DiagnosticSource::Manifest));

        let ctx = engine.host_requests.start(HostKind::ManifestRead).unwrap();
        engine.on_host_result(&ctx, None, b"");
        let after_none: Vec<String> = engine.manifests.iter().map(|m| m.name().into()).collect();
        assert_eq!(before_names, after_none);
        assert!(engine
            .diagnostics
            .snapshot()
            .iter()
            .any(|d| d.source == DiagnosticSource::Manifest));
    }

    #[test]
    fn pane_update_syncs_tracked_tab_and_sanitized_title() {
        let mut engine = Engine::new(EngineConfig::parse(BTreeMap::new()));
        engine.on_permission(true);
        engine.on_event_inventory(vec![TerminalCandidate {
            pane_id: 5,
            tab_position: 0,
            tab_name: "main".into(),
            title: "shell".into(),
        }]);
        engine.on_command_changed(5, &["claude".into()], true);
        assert_eq!(engine.panes.get(&5).unwrap().tab_position, 0);

        engine.on_event_inventory(vec![TerminalCandidate {
            pane_id: 5,
            tab_position: 2,
            tab_name: "work".into(),
            title: "\u{1b}[31mrepo\u{200b}".into(),
        }]);
        let pane = engine.panes.get(&5).unwrap();
        assert_eq!(pane.tab_position, 2);
        assert_eq!(pane.title.as_deref(), Some("repo"));
        assert_eq!(pane.display, "repo");
    }
}
