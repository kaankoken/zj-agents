#[cfg(target_family = "wasm")]
mod plugin {
    use std::collections::BTreeMap;

    use zellij_tile::prelude::*;
    use zj_agents_core::protocol::{Hello, Reload};
    use zj_agents_sidebar::elapsed_ms;
    use zj_agents_sidebar::sidebar::{render_plan, Sidebar, SidebarAction, SidebarKey};
    use zj_agents_sidebar::PermissionState;

    #[derive(Default)]
    struct State {
        sidebar: Option<Sidebar>,
    }

    register_plugin!(State);

    fn dispatch_action(action: SidebarAction) {
        match action {
            SidebarAction::None => {}
            SidebarAction::SendHello => {
                pipe_message_to_plugin(
                    MessageToPlugin::new("zj-agents:hello")
                        .with_payload(serde_json::to_string(&Hello::v1()).unwrap_or_default()),
                );
            }
            SidebarAction::SendReload => {
                pipe_message_to_plugin(
                    MessageToPlugin::new("zj-agents:reload")
                        .with_payload(serde_json::to_string(&Reload::v1()).unwrap_or_default()),
                );
            }
            SidebarAction::Focus(id) => {
                focus_pane_with_id(PaneId::Terminal(id), false, false);
                hide_self();
            }
            SidebarAction::Hide => {
                hide_self();
            }
        }
    }

    impl ZellijPlugin for State {
        fn load(&mut self, _configuration: BTreeMap<String, String>) {
            self.sidebar = Some(Sidebar::new());
            subscribe(&[
                EventType::Key,
                EventType::Timer,
                EventType::PermissionRequestResult,
            ]);
            request_permission(&[
                PermissionType::ChangeApplicationState,
                PermissionType::MessageAndLaunchOtherPlugins,
            ]);
            set_timeout(1.0);
        }

        fn update(&mut self, event: Event) -> bool {
            let sidebar = self.sidebar.as_mut().expect("load initializes sidebar");
            match event {
                Event::PermissionRequestResult(status) => {
                    let granted = matches!(status, PermissionStatus::Granted);
                    dispatch_action(sidebar.on_permission(granted));
                    true
                }
                Event::Timer(elapsed_seconds) => {
                    set_timeout(1.0);
                    if sidebar.permission() != PermissionState::Granted {
                        return false;
                    }
                    dispatch_action(sidebar.on_timer(elapsed_ms(elapsed_seconds)));
                    true
                }
                Event::Key(key) => {
                    if let Some(mapped) = map_key(&key) {
                        let action = sidebar.on_key(mapped);
                        let should_render =
                            !matches!(action, SidebarAction::Hide | SidebarAction::Focus(_));
                        dispatch_action(action);
                        return should_render;
                    }
                    false
                }
                _ => false,
            }
        }

        fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
            let sidebar = self.sidebar.as_mut().expect("load initializes sidebar");
            if sidebar.permission() != PermissionState::Granted {
                return false;
            }
            if pipe_message.name == "zj-agents:snapshot" {
                let payload = pipe_message.payload.as_deref().unwrap_or("");
                sidebar.on_snapshot_payload(payload);
                return true;
            }
            false
        }

        fn render(&mut self, rows: usize, cols: usize) {
            let sidebar = self.sidebar.as_ref().expect("load initializes sidebar");
            let lines = render_plan(sidebar, rows, cols);
            for (y, line) in lines.into_iter().enumerate() {
                let text = if line.selected {
                    Text::new(&line.text).selected()
                } else {
                    Text::new(&line.text)
                };
                print_text_with_coordinates(text, 0, y, Some(cols), None);
            }
        }
    }

    fn map_key(key: &KeyWithModifier) -> Option<SidebarKey> {
        use BareKey::*;
        if !key.has_no_modifiers() {
            return None;
        }
        match key.bare_key {
            Up | Char('k') => Some(SidebarKey::Up),
            Down | Char('j') => Some(SidebarKey::Down),
            Enter => Some(SidebarKey::Enter),
            Char('r') => Some(SidebarKey::Reload),
            Char('q') | Esc => Some(SidebarKey::Quit),
            _ => None,
        }
    }
}

#[cfg(not(target_family = "wasm"))]
fn main() {}

#[cfg(target_family = "wasm")]
fn main() {}
