#[derive(Debug, Clone, Copy)]
pub struct AppConfig {
    pub gravity_ms: u64,
    pub frame_ms: u64,
    pub das_ms: u64,
    pub arr_ms: u64,
    pub dcd_ms: u64,
    pub soft_drop_factor: u32,
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
        }
    }
}
