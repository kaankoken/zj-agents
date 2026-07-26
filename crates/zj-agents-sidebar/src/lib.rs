pub mod sidebar;

pub use sidebar::{render_plan, PermissionState, RenderLine, Sidebar, SidebarAction, SidebarKey};

pub fn elapsed_ms(elapsed_seconds: f64) -> u64 {
    if !elapsed_seconds.is_finite() || elapsed_seconds <= 0.0 {
        return 0;
    }
    let milliseconds = elapsed_seconds * 1_000.0;
    if !milliseconds.is_finite() || milliseconds >= u64::MAX as f64 {
        u64::MAX
    } else {
        milliseconds.round() as u64
    }
}
