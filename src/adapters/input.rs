use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::domain::command::Command;

#[derive(Debug, Clone, Copy)]
pub struct KeyBindings {
    pub move_left: KeyCode,
    pub move_right: KeyCode,
    pub soft_drop: KeyCode,
    pub rotate_left: KeyCode,
    pub rotate_right: KeyCode,
    pub rotate_180: KeyCode,
    pub hold: KeyCode,
    pub hard_drop: KeyCode,
    pub quit: KeyCode,
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            move_left: KeyCode::Left,
            move_right: KeyCode::Right,
            soft_drop: KeyCode::Down,
            rotate_left: KeyCode::Char('z'),
            rotate_right: KeyCode::Up,
            rotate_180: KeyCode::Char('a'),
            hold: KeyCode::Char('c'),
            hard_drop: KeyCode::Char(' '),
            quit: KeyCode::Char('q'),
        }
    }
}

pub fn keycode_label(code: KeyCode) -> String {
    match code {
        KeyCode::Left => "Left Arrow".to_string(),
        KeyCode::Right => "Right Arrow".to_string(),
        KeyCode::Up => "Up Arrow".to_string(),
        KeyCode::Down => "Down Arrow".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "Page Up".to_string(),
        KeyCode::PageDown => "Page Down".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        other => format!("{:?}", other),
    }
}

pub fn keycode_to_storage(code: KeyCode) -> String {
    match code {
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => format!("Char:{}", c.to_ascii_lowercase()),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        other => format!("Other:{:?}", other),
    }
}

pub fn keycode_from_storage(value: &str) -> Option<KeyCode> {
    let raw = value.trim();
    let lower = raw.to_ascii_lowercase();
    match lower.as_str() {
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "space" => Some(KeyCode::Char(' ')),
        "enter" => Some(KeyCode::Enter),
        "tab" => Some(KeyCode::Tab),
        "backspace" => Some(KeyCode::Backspace),
        "delete" => Some(KeyCode::Delete),
        "insert" => Some(KeyCode::Insert),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "pageup" => Some(KeyCode::PageUp),
        "pagedown" => Some(KeyCode::PageDown),
        "esc" | "escape" => Some(KeyCode::Esc),
        _ => {
            if let Some(rest) = raw
                .strip_prefix("Char:")
                .or_else(|| raw.strip_prefix("char:"))
            {
                let mut chars = rest.chars();
                let c = chars.next()?;
                if chars.next().is_none() {
                    return Some(KeyCode::Char(c));
                }
            }
            if let Some(rest) = lower.strip_prefix('f')
                && let Ok(num) = rest.parse::<u8>()
            {
                return Some(KeyCode::F(num));
            }
            None
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InputConfig {
    pub das_ms: u64,
    pub arr_ms: u64,
    pub dcd_ms: u64,
}

/// Physical key state from OS - simplified, only tracks if key is down
#[derive(Debug, Default, Clone, Copy)]
struct PhysicalKey {
    is_down: bool,
    last_event: Option<Instant>,
}

impl PhysicalKey {
    fn press(&mut self, now: Instant) {
        self.is_down = true;
        self.last_event = Some(now);
    }

    fn release(&mut self) {
        self.is_down = false;
        // Keep last_event for debugging, but key is up
    }

    fn update_event_time(&mut self, now: Instant) {
        self.last_event = Some(now);
    }

    /// Auto-release if no events received for too long (safety net for missed Release events)
    fn auto_release_if_stale(&mut self, now: Instant, timeout_ms: u64) {
        if !self.is_down {
            return;
        }
        if let Some(last) = self.last_event
            && now.duration_since(last) >= Duration::from_millis(timeout_ms)
        {
            self.release();
        }
    }
}

/// Horizontal direction for DAS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
}

impl Dir {
    fn to_command(self) -> Command {
        match self {
            Dir::Left => Command::MoveLeft,
            Dir::Right => Command::MoveRight,
        }
    }
}

/// DAS (Delayed Auto-Shift) state machine for horizontal movement
/// This is a unified timer for left/right - only one direction is active at a time
#[derive(Debug, Default, Clone, Copy)]
struct DasState {
    /// Which direction DAS is currently active for (None = stopped)
    active_dir: Option<Dir>,
    /// When DAS charging began for current direction
    das_start: Option<Instant>,
    /// When last ARR repeat fired (after DAS charged)
    last_repeat_at: Option<Instant>,
    /// Whether DAS has fully charged (ready for ARR)
    das_charged: bool,
}

impl DasState {
    /// Start DAS for a new direction (on initial key press)
    fn start(&mut self, dir: Dir, now: Instant) {
        self.active_dir = Some(dir);
        self.das_start = Some(now);
        self.last_repeat_at = None;
        self.das_charged = false;
    }

    /// Start DAS with pre-charge (for DCD - direction cut delay)
    fn start_with_precharge(&mut self, dir: Dir, now: Instant, precharge_ms: u64) {
        self.active_dir = Some(dir);
        // Pretend we started earlier to pre-charge DAS
        self.das_start = Some(now - Duration::from_millis(precharge_ms));
        self.last_repeat_at = None;
        self.das_charged = false;
    }

    /// Stop DAS completely
    fn stop(&mut self) {
        self.active_dir = None;
        self.das_start = None;
        self.last_repeat_at = None;
        self.das_charged = false;
    }

    /// Check if DAS should produce a repeat command this frame
    fn poll_repeat(&mut self, now: Instant, das_ms: u64, arr_ms: u64) -> Option<Command> {
        let dir = self.active_dir?;
        let das_start = self.das_start?;

        // Check if DAS has charged
        if !self.das_charged {
            if now.duration_since(das_start) >= Duration::from_millis(das_ms) {
                self.das_charged = true;
                self.last_repeat_at = Some(now);
                return Some(dir.to_command());
            }
            return None;
        }

        // DAS is charged, check ARR timing
        if arr_ms == 0 {
            // ARR=0 means instant repeat every frame
            return Some(dir.to_command());
        }

        let last = self.last_repeat_at.unwrap_or(das_start);
        if now.duration_since(last) >= Duration::from_millis(arr_ms) {
            self.last_repeat_at = Some(now);
            return Some(dir.to_command());
        }

        None
    }
}

/// Virtual key state for legacy terminals (inferred from Press event gaps)
/// In legacy mode, OS only sends Press events - we infer Release from gaps
#[derive(Debug, Default, Clone, Copy)]
struct VirtualKey {
    /// Whether we believe this key is held (inferred)
    is_held: bool,
    /// When the last Press event arrived
    last_press: Option<Instant>,
}

impl VirtualKey {
    /// Process an incoming Press event
    /// Returns true if this is a NEW press (start of hold), false if OS repeat
    fn on_press(&mut self, now: Instant, gap_threshold_ms: u64) -> bool {
        let is_new_press = match self.last_press {
            None => true, // First ever press
            Some(last) => {
                // If gap > threshold, key was released and pressed again
                now.duration_since(last) > Duration::from_millis(gap_threshold_ms)
            }
        };

        self.last_press = Some(now);
        if is_new_press {
            self.is_held = true;
        }
        is_new_press
    }

    /// Check if key should be considered released (gap exceeded)
    /// Call this from repeat_commands to infer release
    fn check_release(&mut self, now: Instant, gap_threshold_ms: u64) -> bool {
        if !self.is_held {
            return false;
        }
        if let Some(last) = self.last_press
            && now.duration_since(last) > Duration::from_millis(gap_threshold_ms)
        {
            self.is_held = false;
            return true; // Just released
        }
        false
    }
}

/// Auto-calibration for OS keyboard repeat rate (legacy mode)
/// Measures the interval between consecutive Press events to determine
/// when a gap indicates a key release vs normal OS repeat
#[derive(Debug, Clone)]
struct LegacyCalibration {
    /// Measured intervals between consecutive Press events (same key)
    samples: Vec<u64>,
    /// Number of samples needed before calibration is complete
    samples_needed: usize,
    /// Computed gap threshold (2x average interval)
    gap_threshold_ms: u64,
    /// Whether calibration is complete
    calibrated: bool,
}

impl Default for LegacyCalibration {
    fn default() -> Self {
        Self {
            samples: Vec::with_capacity(8),
            samples_needed: 5,
            // Default threshold before calibration: 80ms
            // Typical OS repeat is ~30-50ms, so 80ms should catch most releases
            gap_threshold_ms: 80,
            calibrated: false,
        }
    }
}

impl LegacyCalibration {
    /// Record an interval between consecutive Press events for the same key
    fn record_interval(&mut self, interval_ms: u64) {
        if self.calibrated {
            return;
        }

        // Filter out outliers (likely actual releases, not repeats)
        // Only accept intervals that look like OS repeats (< 100ms)
        if interval_ms > 100 {
            return;
        }

        self.samples.push(interval_ms);

        if self.samples.len() >= self.samples_needed {
            self.finalize();
        }
    }

    fn finalize(&mut self) {
        if self.samples.is_empty() {
            return;
        }

        // Compute average interval
        let sum: u64 = self.samples.iter().sum();
        let avg = sum / self.samples.len() as u64;

        // Set threshold to ~2x average (with minimum floor)
        self.gap_threshold_ms = (avg * 2).max(60);
        self.calibrated = true;
    }

    fn threshold(&self) -> u64 {
        self.gap_threshold_ms
    }
}

/// Soft drop repeat state (similar to DAS but for vertical)
#[derive(Debug, Default, Clone, Copy)]
struct SoftDropState {
    /// Whether soft drop repeat is active
    active: bool,
    /// When soft drop started
    start: Option<Instant>,
    /// When last repeat fired
    last_repeat_at: Option<Instant>,
    /// Whether initial delay has passed
    charged: bool,
}

impl SoftDropState {
    fn start(&mut self, now: Instant) {
        self.active = true;
        self.start = Some(now);
        self.last_repeat_at = None;
        self.charged = false;
    }

    fn stop(&mut self) {
        self.active = false;
        self.start = None;
        self.last_repeat_at = None;
        self.charged = false;
    }

    /// Poll for soft drop repeat - uses same DAS/ARR timing
    fn poll_repeat(&mut self, now: Instant, das_ms: u64, arr_ms: u64) -> Option<Command> {
        if !self.active {
            return None;
        }
        let start = self.start?;

        if !self.charged {
            if now.duration_since(start) >= Duration::from_millis(das_ms) {
                self.charged = true;
                self.last_repeat_at = Some(now);
                return Some(Command::SoftDrop);
            }
            return None;
        }

        if arr_ms == 0 {
            return Some(Command::SoftDrop);
        }

        let last = self.last_repeat_at.unwrap_or(start);
        if now.duration_since(last) >= Duration::from_millis(arr_ms) {
            self.last_repeat_at = Some(now);
            return Some(Command::SoftDrop);
        }

        None
    }
}

pub struct InputState {
    cfg: InputConfig,
    bindings: KeyBindings,
    /// Physical key states (what the OS tells us) - used in enhanced mode
    left_key: PhysicalKey,
    right_key: PhysicalKey,
    down_key: PhysicalKey,
    /// DAS state machine for horizontal movement
    das: DasState,
    /// Soft drop repeat state
    soft_drop: SoftDropState,
    /// Movement lockout (e.g., after garbage)
    lockout_until: Option<Instant>,
    /// Whether terminal supports enhanced input (Release/Repeat events)
    enhanced_input: bool,

    // === Legacy mode state (for terminals without Release/Repeat events) ===
    /// Virtual key states - inferred from Press event timing
    legacy_left: VirtualKey,
    legacy_right: VirtualKey,
    legacy_down: VirtualKey,
    /// Auto-calibration for OS repeat interval detection
    legacy_calibration: LegacyCalibration,
}

impl InputState {
    pub fn new(cfg: InputConfig, bindings: KeyBindings) -> Self {
        Self {
            cfg,
            bindings,
            left_key: PhysicalKey::default(),
            right_key: PhysicalKey::default(),
            down_key: PhysicalKey::default(),
            das: DasState::default(),
            soft_drop: SoftDropState::default(),
            lockout_until: None,
            enhanced_input: false,
            // Legacy mode state
            legacy_left: VirtualKey::default(),
            legacy_right: VirtualKey::default(),
            legacy_down: VirtualKey::default(),
            legacy_calibration: LegacyCalibration::default(),
        }
    }

    pub fn handle_event(&mut self, event: Event, now: Instant, out: &mut Vec<Command>) {
        if let Event::Key(key) = event {
            self.handle_key(key, now, out);
        }
    }

    /// Called once per frame to generate repeat commands from DAS/ARR
    pub fn repeat_commands(&mut self, now: Instant, out: &mut Vec<Command>) {
        if self.enhanced_input {
            self.repeat_commands_enhanced(now, out);
        } else {
            self.repeat_commands_legacy(now, out);
        }
    }

    /// Enhanced mode: use physical key state from OS Release events
    fn repeat_commands_enhanced(&mut self, now: Instant, out: &mut Vec<Command>) {
        // Safety net: auto-release keys that haven't received events (missed Release)
        const STALE_TIMEOUT_MS: u64 = 500;
        self.left_key.auto_release_if_stale(now, STALE_TIMEOUT_MS);
        self.right_key.auto_release_if_stale(now, STALE_TIMEOUT_MS);
        self.down_key.auto_release_if_stale(now, STALE_TIMEOUT_MS);

        // SOCD resolution: ensure DAS active direction matches physical key state
        self.resolve_das_direction(now, out);

        // Soft drop resolution
        if !self.down_key.is_down && self.soft_drop.active {
            self.soft_drop.stop();
        }

        // Skip repeats during lockout
        if self.lockout_active(now) {
            return;
        }

        // Poll DAS for horizontal repeat
        if let Some(cmd) = self.das.poll_repeat(now, self.cfg.das_ms, self.cfg.arr_ms) {
            out.push(cmd);
        }

        // Poll soft drop for repeat
        if let Some(cmd) = self
            .soft_drop
            .poll_repeat(now, self.cfg.das_ms, self.cfg.arr_ms)
        {
            out.push(cmd);
        }
    }

    /// Legacy mode: infer key releases from timing gaps in Press events
    fn repeat_commands_legacy(&mut self, now: Instant, out: &mut Vec<Command>) {
        let threshold = self.legacy_calibration.threshold();

        // Check for inferred releases (gap exceeded threshold)
        let left_released = self.legacy_left.check_release(now, threshold);
        let right_released = self.legacy_right.check_release(now, threshold);
        let down_released = self.legacy_down.check_release(now, threshold);

        // SOCD resolution for legacy mode
        self.resolve_das_direction_legacy(now, left_released, right_released, out);

        // Soft drop resolution
        if down_released && self.soft_drop.active {
            self.soft_drop.stop();
        }

        // Skip repeats during lockout
        if self.lockout_active(now) {
            return;
        }

        // Poll DAS for horizontal repeat
        if let Some(cmd) = self.das.poll_repeat(now, self.cfg.das_ms, self.cfg.arr_ms) {
            out.push(cmd);
        }

        // Poll soft drop for repeat
        if let Some(cmd) = self
            .soft_drop
            .poll_repeat(now, self.cfg.das_ms, self.cfg.arr_ms)
        {
            out.push(cmd);
        }
    }

    /// SOCD resolution for legacy mode - uses virtual key state
    fn resolve_das_direction_legacy(
        &mut self,
        now: Instant,
        left_released: bool,
        right_released: bool,
        out: &mut Vec<Command>,
    ) {
        match self.das.active_dir {
            Some(Dir::Left) => {
                if left_released || !self.legacy_left.is_held {
                    // Left released - check if right is still held (SOCD fallback)
                    if self.legacy_right.is_held {
                        let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
                        self.das.start_with_precharge(Dir::Right, now, precharge);
                        out.push(Command::MoveRight);
                    } else {
                        self.das.stop();
                    }
                }
            }
            Some(Dir::Right) => {
                if right_released || !self.legacy_right.is_held {
                    // Right released - check if left is still held (SOCD fallback)
                    if self.legacy_left.is_held {
                        let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
                        self.das.start_with_precharge(Dir::Left, now, precharge);
                        out.push(Command::MoveLeft);
                    } else {
                        self.das.stop();
                    }
                }
            }
            None => {
                // No active direction - check if we should start one
                if self.legacy_left.is_held && !self.legacy_right.is_held {
                    self.das.start(Dir::Left, now);
                } else if self.legacy_right.is_held && !self.legacy_left.is_held {
                    self.das.start(Dir::Right, now);
                }
            }
        }
    }

    /// SOCD (Simultaneous Opposing Cardinal Directions) resolution
    /// Ensures DAS active direction matches what keys are physically held
    fn resolve_das_direction(&mut self, now: Instant, out: &mut Vec<Command>) {
        match self.das.active_dir {
            Some(Dir::Left) => {
                if !self.left_key.is_down {
                    // Left released - check if right is still held (SOCD fallback)
                    if self.right_key.is_down {
                        // Switch to right with DCD pre-charge
                        let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
                        self.das.start_with_precharge(Dir::Right, now, precharge);
                        out.push(Command::MoveRight); // Immediate move on direction switch
                    } else {
                        self.das.stop();
                    }
                }
            }
            Some(Dir::Right) => {
                if !self.right_key.is_down {
                    // Right released - check if left is still held (SOCD fallback)
                    if self.left_key.is_down {
                        // Switch to left with DCD pre-charge
                        let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
                        self.das.start_with_precharge(Dir::Left, now, precharge);
                        out.push(Command::MoveLeft); // Immediate move on direction switch
                    } else {
                        self.das.stop();
                    }
                }
            }
            None => {
                // No active direction - check if we should start one
                // (This handles edge case where key was pressed but DAS wasn't started)
                if self.left_key.is_down && !self.right_key.is_down {
                    self.das.start(Dir::Left, now);
                } else if self.right_key.is_down && !self.left_key.is_down {
                    self.das.start(Dir::Right, now);
                }
                // If both held, don't move (neutral SOCD)
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant, out: &mut Vec<Command>) {
        // Detect enhanced input capability (terminal supports Release/Repeat)
        if matches!(key.kind, KeyEventKind::Repeat | KeyEventKind::Release)
            && (matches_keycode(key.code, self.bindings.move_left)
                || matches_keycode(key.code, self.bindings.move_right)
                || matches_keycode(key.code, self.bindings.soft_drop))
        {
            self.enhanced_input = true;
        }

        // Fallback for terminals without enhanced input - use stateful virtual keys
        if !self.enhanced_input {
            self.handle_key_fallback(key, now, out);
            return;
        }

        // Handle lockout (still process Release events to track physical state)
        if self.lockout_active(now) {
            self.handle_key_during_lockout(key, now);
            return;
        }

        // Movement keys - Left
        if matches_keycode(key.code, self.bindings.move_left) {
            match key.kind {
                KeyEventKind::Press => {
                    self.left_key.press(now);
                    self.handle_left_press(now, out);
                }
                KeyEventKind::Release => {
                    self.left_key.release();
                    // DAS resolution happens in repeat_commands via resolve_das_direction
                }
                KeyEventKind::Repeat => {
                    // Only update event time - don't trigger any action
                    // This keeps the key "alive" for auto-release safety
                    self.left_key.update_event_time(now);
                }
            }
            return;
        }

        // Movement keys - Right
        if matches_keycode(key.code, self.bindings.move_right) {
            match key.kind {
                KeyEventKind::Press => {
                    self.right_key.press(now);
                    self.handle_right_press(now, out);
                }
                KeyEventKind::Release => {
                    self.right_key.release();
                }
                KeyEventKind::Repeat => {
                    self.right_key.update_event_time(now);
                }
            }
            return;
        }

        // Movement keys - Soft Drop
        if matches_keycode(key.code, self.bindings.soft_drop) {
            match key.kind {
                KeyEventKind::Press => {
                    if !self.down_key.is_down {
                        self.down_key.press(now);
                        self.soft_drop.start(now);
                        out.push(Command::SoftDrop); // Immediate soft drop on press
                    }
                }
                KeyEventKind::Release => {
                    self.down_key.release();
                    self.soft_drop.stop();
                }
                KeyEventKind::Repeat => {
                    self.down_key.update_event_time(now);
                }
            }
            return;
        }

        // Non-movement keys - only trigger on Press
        if matches!(key.kind, KeyEventKind::Press) {
            if matches_keycode(key.code, self.bindings.rotate_left) {
                out.push(Command::RotateLeft);
            } else if matches_keycode(key.code, self.bindings.rotate_right) {
                out.push(Command::RotateRight);
            } else if matches_keycode(key.code, self.bindings.rotate_180) {
                out.push(Command::Rotate180);
            } else if matches_keycode(key.code, self.bindings.hold) {
                out.push(Command::Hold);
            } else if matches_keycode(key.code, self.bindings.hard_drop) {
                out.push(Command::HardDrop);
            } else if is_quit_key(key.code, self.bindings.quit) {
                out.push(Command::Quit);
            }
        }
    }

    /// Handle left key press - implements SOCD with last-input priority
    fn handle_left_press(&mut self, now: Instant, out: &mut Vec<Command>) {
        // Check if switching from right (DCD applies)
        let was_going_right = matches!(self.das.active_dir, Some(Dir::Right));

        if was_going_right {
            // Direction switch: apply DCD (pre-charge DAS)
            let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
            self.das.start_with_precharge(Dir::Left, now, precharge);
        } else {
            // Fresh start or was already going left
            self.das.start(Dir::Left, now);
        }

        // Always emit immediate move on press
        out.push(Command::MoveLeft);
    }

    /// Handle right key press - implements SOCD with last-input priority
    fn handle_right_press(&mut self, now: Instant, out: &mut Vec<Command>) {
        let was_going_left = matches!(self.das.active_dir, Some(Dir::Left));

        if was_going_left {
            let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
            self.das.start_with_precharge(Dir::Right, now, precharge);
        } else {
            self.das.start(Dir::Right, now);
        }

        out.push(Command::MoveRight);
    }

    /// Handle key events during lockout - only track physical state, no commands
    fn handle_key_during_lockout(&mut self, key: KeyEvent, now: Instant) {
        if matches_keycode(key.code, self.bindings.move_left) {
            match key.kind {
                KeyEventKind::Press => self.left_key.press(now),
                KeyEventKind::Release => self.left_key.release(),
                KeyEventKind::Repeat => self.left_key.update_event_time(now),
            }
        } else if matches_keycode(key.code, self.bindings.move_right) {
            match key.kind {
                KeyEventKind::Press => self.right_key.press(now),
                KeyEventKind::Release => self.right_key.release(),
                KeyEventKind::Repeat => self.right_key.update_event_time(now),
            }
        } else if matches_keycode(key.code, self.bindings.soft_drop) {
            match key.kind {
                KeyEventKind::Press => self.down_key.press(now),
                KeyEventKind::Release => {
                    self.down_key.release();
                    self.soft_drop.stop();
                }
                KeyEventKind::Repeat => self.down_key.update_event_time(now),
            }
        }
    }

    /// Fallback for terminals without enhanced input - stateful with virtual key tracking
    /// In legacy mode, OS only sends Press events. We infer Release from timing gaps.
    fn handle_key_fallback(&mut self, key: KeyEvent, now: Instant, out: &mut Vec<Command>) {
        if !matches!(key.kind, KeyEventKind::Press) {
            return;
        }

        let threshold = self.legacy_calibration.threshold();

        // Movement keys - Left
        if matches_keycode(key.code, self.bindings.move_left) {
            // Record interval for calibration
            if let Some(last) = self.legacy_left.last_press {
                let interval = now.duration_since(last).as_millis() as u64;
                self.legacy_calibration.record_interval(interval);
            }

            let is_new_press = self.legacy_left.on_press(now, threshold);
            if is_new_press {
                // New press: emit immediate command and start DAS
                self.handle_legacy_left_press(now, out);
            }
            // OS repeat: suppress command, let DAS/ARR handle it
            return;
        }

        // Movement keys - Right
        if matches_keycode(key.code, self.bindings.move_right) {
            if let Some(last) = self.legacy_right.last_press {
                let interval = now.duration_since(last).as_millis() as u64;
                self.legacy_calibration.record_interval(interval);
            }

            let is_new_press = self.legacy_right.on_press(now, threshold);
            if is_new_press {
                self.handle_legacy_right_press(now, out);
            }
            return;
        }

        // Movement keys - Soft Drop
        if matches_keycode(key.code, self.bindings.soft_drop) {
            if let Some(last) = self.legacy_down.last_press {
                let interval = now.duration_since(last).as_millis() as u64;
                self.legacy_calibration.record_interval(interval);
            }

            let is_new_press = self.legacy_down.on_press(now, threshold);
            if is_new_press {
                // New press: emit immediate soft drop and start repeat timer
                self.soft_drop.start(now);
                out.push(Command::SoftDrop);
            }
            // OS repeat: suppress, let soft_drop timer handle it
            return;
        }

        // Non-movement keys - fire on every Press (no DAS/ARR)
        if matches_keycode(key.code, self.bindings.rotate_left) {
            out.push(Command::RotateLeft);
        } else if matches_keycode(key.code, self.bindings.rotate_right) {
            out.push(Command::RotateRight);
        } else if matches_keycode(key.code, self.bindings.rotate_180) {
            out.push(Command::Rotate180);
        } else if matches_keycode(key.code, self.bindings.hold) {
            out.push(Command::Hold);
        } else if matches_keycode(key.code, self.bindings.hard_drop) {
            out.push(Command::HardDrop);
        } else if is_quit_key(key.code, self.bindings.quit) {
            out.push(Command::Quit);
        }
    }

    /// Handle left key press in legacy mode - SOCD with last-input priority
    fn handle_legacy_left_press(&mut self, now: Instant, out: &mut Vec<Command>) {
        let was_going_right = matches!(self.das.active_dir, Some(Dir::Right));

        if was_going_right {
            // Direction switch: apply DCD
            let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
            self.das.start_with_precharge(Dir::Left, now, precharge);
        } else {
            self.das.start(Dir::Left, now);
        }

        out.push(Command::MoveLeft);
    }

    /// Handle right key press in legacy mode - SOCD with last-input priority
    fn handle_legacy_right_press(&mut self, now: Instant, out: &mut Vec<Command>) {
        let was_going_left = matches!(self.das.active_dir, Some(Dir::Left));

        if was_going_left {
            let precharge = self.cfg.das_ms.saturating_sub(self.cfg.dcd_ms);
            self.das.start_with_precharge(Dir::Right, now, precharge);
        } else {
            self.das.start(Dir::Right, now);
        }

        out.push(Command::MoveRight);
    }

    pub fn soft_drop_active(&self) -> bool {
        if self.enhanced_input {
            self.down_key.is_down
        } else {
            self.legacy_down.is_held
        }
    }

    pub fn enhanced_input_active(&self) -> bool {
        self.enhanced_input
    }

    pub fn clear_motion(&mut self) {
        // Enhanced mode state
        self.left_key.release();
        self.right_key.release();
        self.down_key.release();
        // Legacy mode state
        self.legacy_left.is_held = false;
        self.legacy_right.is_held = false;
        self.legacy_down.is_held = false;
        // Shared state
        self.das.stop();
        self.soft_drop.stop();
    }

    pub fn lockout_movement(&mut self, now: Instant, duration_ms: u64) {
        self.das.stop();
        self.soft_drop.stop();
        self.lockout_until = Some(now + Duration::from_millis(duration_ms));
    }

    fn lockout_active(&mut self, now: Instant) -> bool {
        if let Some(until) = self.lockout_until {
            if now < until {
                return true;
            }
            self.lockout_until = None;
        }
        false
    }
}

fn matches_keycode(actual: KeyCode, expected: KeyCode) -> bool {
    match (actual, expected) {
        (KeyCode::Char(a), KeyCode::Char(b)) => a.eq_ignore_ascii_case(&b),
        _ => actual == expected,
    }
}

fn is_quit_key(actual: KeyCode, expected: KeyCode) -> bool {
    matches_keycode(actual, expected) || matches!(actual, KeyCode::Esc)
}
