#[cfg(target_family = "wasm")]
mod plugin {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use zellij_tile::prelude::*;
    use zj_agents_core::sanitize::sanitize_metadata;
    use zj_agents_engine::elapsed_ms;
    use zj_agents_engine::engine::{Effect, Engine, EngineConfig, HostError, TimerObservations};
    use zj_agents_engine::inventory::TerminalCandidate;

    #[derive(Default)]
    struct State {
        engine: Option<Engine>,
    }

    register_plugin!(State);

    fn dispatch_effects(effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::RequestInventory { argv, context } => {
                    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                    run_command(&refs, context);
                }
                Effect::ReadOverrides { argv, env, context } => {
                    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                    run_command_with_env_variables_and_cwd(&refs, env, PathBuf::from("."), context);
                }
                Effect::DetectHost { argv, context } => {
                    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                    run_command(&refs, context);
                }
                Effect::Notify { argv, context } => {
                    let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
                    run_command(&refs, context);
                }
                Effect::SendSnapshot { plugin_id, payload } => {
                    pipe_message_to_plugin(
                        MessageToPlugin::new("zj-agents:snapshot")
                            .with_destination_plugin_id(plugin_id)
                            .with_payload(payload),
                    );
                }
            }
        }
    }

    impl ZellijPlugin for State {
        fn load(&mut self, configuration: BTreeMap<String, String>) {
            self.engine = Some(Engine::new(EngineConfig::parse(configuration)));
            subscribe(&[
                EventType::PaneUpdate,
                EventType::PaneClosed,
                EventType::TabUpdate,
                EventType::CwdChanged,
                EventType::CommandChanged,
                EventType::Timer,
                EventType::RunCommandResult,
                EventType::PermissionRequestResult,
            ]);
            request_permission(&[
                PermissionType::ReadApplicationState,
                PermissionType::ReadPaneContents,
                PermissionType::RunCommands,
                PermissionType::MessageAndLaunchOtherPlugins,
            ]);
            set_timeout(1.0);
        }

        fn update(&mut self, event: Event) -> bool {
            let engine = self.engine.as_mut().expect("load initializes engine");
            match event {
                Event::PermissionRequestResult(status) => {
                    let granted = matches!(status, PermissionStatus::Granted);
                    dispatch_effects(engine.on_permission(granted));
                }
                Event::Timer(elapsed_seconds) => {
                    set_timeout(1.0);
                    if !engine.permission_is_granted() {
                        return false;
                    }
                    let converted = elapsed_ms(elapsed_seconds);
                    let Some(mut plan) = engine.begin_timer(converted) else {
                        return false;
                    };
                    let bootstrap = std::mem::take(&mut plan.effects);
                    dispatch_effects(bootstrap);
                    let reconciled_commands = reconcile_commands(&plan.reconcile_pane_ids);
                    let focused_pane_id = match get_focused_pane_info() {
                        Ok((_tab, PaneId::Terminal(id))) => Some(id),
                        _ => None,
                    };
                    let mut viewports = BTreeMap::new();
                    for pane_id in &plan.viewport_pane_ids {
                        match get_pane_scrollback(PaneId::Terminal(*pane_id), false) {
                            Ok(contents) => {
                                viewports.insert(*pane_id, Ok(contents.viewport));
                            }
                            Err(_) => {
                                viewports.insert(*pane_id, Err(HostError::Failed));
                            }
                        }
                    }
                    let effects = engine.finish_timer(
                        plan,
                        TimerObservations {
                            focused_pane_id,
                            reconciled_commands,
                            viewports,
                        },
                    );
                    dispatch_effects(effects);
                }
                Event::PaneClosed(pane_id) => {
                    if let PaneId::Terminal(id) = pane_id {
                        engine.on_pane_closed(id);
                    }
                }
                Event::CommandChanged(pane_id, argv, is_foreground, _) => {
                    if let PaneId::Terminal(id) = pane_id {
                        engine.on_command_changed(id, &argv, is_foreground);
                    }
                }
                Event::PaneUpdate(manifest) => {
                    engine.on_event_inventory(pane_manifest_to_candidates(&manifest));
                }
                Event::TabUpdate(tabs) => {
                    let map = tabs.into_iter().map(|t| (t.position, t.name)).collect();
                    engine.on_tab_update(map);
                }
                Event::CwdChanged(pane_id, path, _) => {
                    if let PaneId::Terminal(id) = pane_id {
                        let basename = path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_owned();
                        engine.on_cwd_changed(id, basename);
                    }
                }
                Event::RunCommandResult(exit_code, stdout, _stderr, context) => {
                    dispatch_effects(engine.on_host_result(&context, exit_code, &stdout));
                }
                _ => {}
            }
            false
        }

        fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
            let engine = self.engine.as_mut().expect("load initializes engine");
            if !engine.permission_is_granted() {
                return false;
            }
            let payload = pipe_message.payload.as_deref().unwrap_or("");
            match pipe_message.name.as_str() {
                "zj-agents:hello" => {
                    if let PipeSource::Plugin(id) = pipe_message.source {
                        dispatch_effects(engine.on_hello(id, payload));
                    } else {
                        engine.on_unknown_pipe();
                    }
                }
                "zj-agents:reload" => {
                    dispatch_effects(engine.on_reload(payload));
                }
                _ => engine.on_unknown_pipe(),
            }
            false
        }

        fn render(&mut self, _rows: usize, _cols: usize) {}
    }

    fn reconcile_commands(ids: &[u32]) -> BTreeMap<u32, Result<Vec<String>, HostError>> {
        let mut out = BTreeMap::new();
        for pane_id in ids {
            match get_pane_running_command(PaneId::Terminal(*pane_id)) {
                Ok(cmd) => {
                    out.insert(*pane_id, Ok(cmd));
                }
                Err(_) => {
                    out.insert(*pane_id, Err(HostError::Failed));
                }
            }
        }
        out
    }

    fn pane_manifest_to_candidates(manifest: &PaneManifest) -> Vec<TerminalCandidate> {
        let mut rows = Vec::new();
        for (tab_position, panes) in &manifest.panes {
            for pane in panes {
                if pane.is_plugin || pane.exited {
                    continue;
                }
                rows.push(TerminalCandidate {
                    pane_id: pane.id,
                    tab_position: *tab_position,
                    tab_name: String::new(),
                    title: sanitize_metadata(&pane.title, 60),
                });
            }
        }
        rows
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {}

#[cfg(target_family = "wasm")]
fn main() {}
