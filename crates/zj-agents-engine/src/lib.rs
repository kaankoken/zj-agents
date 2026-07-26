pub mod engine;
pub mod inventory;
pub mod overrides;

pub use engine::{
    Effect, Engine, EngineConfig, HostError, HostKind, HostRequests, PermissionState,
    TimerObservations, TimerPlan,
};
pub use inventory::{parse_pane_list_json, Inventory, TerminalCandidate};
pub use overrides::{
    adopt_overrides, parse_manifest_frame, resolve_manifest_dir, ReloadController,
    READ_MANIFESTS_SH,
};

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
