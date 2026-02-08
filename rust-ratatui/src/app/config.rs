use crate::ui::input::KeyBindings;

#[derive(Debug, Clone, Copy)]
pub struct AppConfig {
    pub gravity_ms: u64,
    pub frame_ms: u64,
    pub das_ms: u64,
    pub arr_ms: u64,
    pub dcd_ms: u64,
    pub soft_drop_factor: u32,
    pub lock_delay_ms: u64,
    pub lock_reset_limit: u32,
    pub key_bindings: KeyBindings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gravity_ms: 1000,
            frame_ms: 16,
            das_ms: 167,
            arr_ms: 16,
            dcd_ms: 33,
            soft_drop_factor: 6,
            lock_delay_ms: 500,
            lock_reset_limit: 10,
            key_bindings: KeyBindings::default(),
        }
    }
}
