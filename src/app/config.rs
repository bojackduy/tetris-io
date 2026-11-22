use crate::adapters::input::KeyBindings;

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
    pub sound_volume: u8,
    pub rotation_sound: bool,
    pub ghost: bool,
    pub grid_style: u8,
    pub key_bindings: KeyBindings,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            gravity_ms: 900,
            frame_ms: 16,
            das_ms: 133,
            arr_ms: 10,
            dcd_ms: 0,
            // Legacy field name; now stores independent soft-drop cells per second (0 = instant).
            // Matches Jstris 60Hz presets: Slow 20, Medium 60, Fast 120, Ultra 180, Instant 0.
            soft_drop_factor: 120,
            lock_delay_ms: 500,
            lock_reset_limit: 0,
            sound_volume: 60,
            rotation_sound: false,
            ghost: true,
            grid_style: 1,
            key_bindings: KeyBindings::default(),
        }
    }
}

impl AppConfig {
    pub fn apply_jstris_profile(&mut self) {
        let bindings = self.key_bindings;
        *self = Self::default();
        self.key_bindings = bindings;
    }

    pub fn soft_drop_label(&self) -> String {
        match self.soft_drop_factor {
            20 => "Slow (20 cells/s)".into(),
            60 => "Medium (60 cells/s)".into(),
            120 => "Fast (120 cells/s)".into(),
            180 => "Ultra (180 cells/s)".into(),
            0 => "Instant".into(),
            rate => format!("Custom ({rate} cells/s)"),
        }
    }
}
