use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::core::command::Command;

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
            if let Some(rest) = raw.strip_prefix("Char:").or_else(|| raw.strip_prefix("char:")) {
                let mut chars = rest.chars();
                let c = chars.next()?;
                if chars.next().is_none() {
                    return Some(KeyCode::Char(c));
                }
            }
            if let Some(rest) = lower.strip_prefix('f') {
                if let Ok(num) = rest.parse::<u8>() {
                    return Some(KeyCode::F(num));
                }
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

#[derive(Debug, Default, Clone, Copy)]
struct HeldKey {
    is_down: bool,
    pressed_at: Option<Instant>,
    last_repeat: Option<Instant>,
    last_event: Option<Instant>,
    repeat_armed: bool,
}

impl HeldKey {
    fn press(&mut self, now: Instant) {
        if !self.is_down {
            self.pressed_at = Some(now);
            self.last_repeat = Some(now);
            self.repeat_armed = true;
        } else {
            self.repeat_armed = true;
        }
        self.is_down = true;
        self.last_event = Some(now);
    }

    fn press_with_dcd(&mut self, now: Instant, das_ms: u64, dcd_ms: u64, use_dcd: bool) {
        let offset = if use_dcd {
            das_ms.saturating_sub(dcd_ms)
        } else {
            0
        };
        if !self.is_down {
            self.pressed_at = Some(now - Duration::from_millis(offset));
            self.last_repeat = Some(now);
            self.repeat_armed = true;
        } else {
            self.repeat_armed = true;
        }
        self.is_down = true;
        self.last_event = Some(now);
    }

    fn release(&mut self) {
        self.is_down = false;
        self.pressed_at = None;
        self.last_repeat = None;
        self.last_event = None;
        self.repeat_armed = false;
    }
}

pub struct InputState {
    cfg: InputConfig,
    bindings: KeyBindings,
    left: HeldKey,
    right: HeldKey,
    down: HeldKey,
    last_dir: Option<Dir>,
    lockout_until: Option<Instant>,
    enhanced_input: bool,
}

impl InputState {
    pub fn new(cfg: InputConfig, bindings: KeyBindings) -> Self {
        Self {
            cfg,
            bindings,
            left: HeldKey::default(),
            right: HeldKey::default(),
            down: HeldKey::default(),
            last_dir: None,
            lockout_until: None,
            enhanced_input: false,
        }
    }

    pub fn handle_event(&mut self, event: Event, now: Instant, out: &mut Vec<Command>) {
        if let Event::Key(key) = event {
            self.handle_key(key, now, out);
        }
    }

    pub fn repeat_commands(&mut self, now: Instant, out: &mut Vec<Command>) {
        if !self.enhanced_input {
            return;
        }
        let short_timeout_ms = 180;
        let long_timeout_ms = (self.cfg.das_ms + self.cfg.arr_ms * 2).max(1000);
        let left_armed = self.left.repeat_armed;
        let right_armed = self.right.repeat_armed;
        let down_armed = self.down.repeat_armed;
        auto_release_if_idle(
            &mut self.left,
            now,
            if left_armed { long_timeout_ms } else { short_timeout_ms },
        );
        auto_release_if_idle(
            &mut self.right,
            now,
            if right_armed { long_timeout_ms } else { short_timeout_ms },
        );
        auto_release_if_idle(
            &mut self.down,
            now,
            if down_armed { long_timeout_ms } else { short_timeout_ms },
        );

        match self.last_dir {
            Some(Dir::Left) => {
                if self.left.repeat_armed && should_repeat(self.left, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveLeft);
                    self.left.last_repeat = Some(now);
                    self.left.last_event = Some(now);
                }
            }
            Some(Dir::Right) => {
                if self.right.repeat_armed && should_repeat(self.right, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveRight);
                    self.right.last_repeat = Some(now);
                    self.right.last_event = Some(now);
                }
            }
            None => {
                if self.left.repeat_armed && should_repeat(self.left, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveLeft);
                    self.left.last_repeat = Some(now);
                    self.left.last_event = Some(now);
                }
                if self.right.repeat_armed && should_repeat(self.right, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveRight);
                    self.right.last_repeat = Some(now);
                    self.right.last_event = Some(now);
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant, out: &mut Vec<Command>) {
        if matches!(key.kind, KeyEventKind::Repeat | KeyEventKind::Release)
            && (matches_keycode(key.code, self.bindings.move_left)
                || matches_keycode(key.code, self.bindings.move_right)
                || matches_keycode(key.code, self.bindings.soft_drop))
        {
            self.enhanced_input = true;
        }

        if !self.enhanced_input {
            if matches_keycode(key.code, self.bindings.move_left) && matches!(key.kind, KeyEventKind::Press)
            {
                out.push(Command::MoveLeft);
                return;
            }
            if matches_keycode(key.code, self.bindings.move_right) && matches!(key.kind, KeyEventKind::Press)
            {
                out.push(Command::MoveRight);
                return;
            }
            if matches_keycode(key.code, self.bindings.soft_drop) && matches!(key.kind, KeyEventKind::Press) {
                out.push(Command::SoftDrop);
                return;
            }
        }

        if self.lockout_active(now) {
            if matches_keycode(key.code, self.bindings.move_left) && matches!(key.kind, KeyEventKind::Release)
            {
                self.left.release();
            } else if matches_keycode(key.code, self.bindings.move_right)
                && matches!(key.kind, KeyEventKind::Release)
            {
                self.right.release();
            } else if matches_keycode(key.code, self.bindings.soft_drop)
                && matches!(key.kind, KeyEventKind::Release)
            {
                self.down.release();
            }
            return;
        }
        if matches_keycode(key.code, self.bindings.move_left) {
            match key.kind {
                KeyEventKind::Press => {
                    self.right.release();
                    let use_dcd = matches!(self.last_dir, Some(Dir::Right));
                    let was_down = self.left.is_down;
                    self.left
                        .press_with_dcd(now, self.cfg.das_ms, self.cfg.dcd_ms, use_dcd);
                    if !was_down {
                        out.push(Command::MoveLeft);
                    }
                    self.last_dir = Some(Dir::Left);
                }
                KeyEventKind::Release => {
                    self.left.release();
                    if !self.right.is_down {
                        self.last_dir = None;
                    }
                }
                KeyEventKind::Repeat => {
                    self.right.release();
                    self.left.press(now);
                }
            }
            return;
        }
        if matches_keycode(key.code, self.bindings.move_right) {
            match key.kind {
                KeyEventKind::Press => {
                    self.left.release();
                    let use_dcd = matches!(self.last_dir, Some(Dir::Left));
                    let was_down = self.right.is_down;
                    self.right
                        .press_with_dcd(now, self.cfg.das_ms, self.cfg.dcd_ms, use_dcd);
                    if !was_down {
                        out.push(Command::MoveRight);
                    }
                    self.last_dir = Some(Dir::Right);
                }
                KeyEventKind::Release => {
                    self.right.release();
                    if !self.left.is_down {
                        self.last_dir = None;
                    }
                }
                KeyEventKind::Repeat => {
                    self.left.release();
                    self.right.press(now);
                }
            }
            return;
        }
        if matches_keycode(key.code, self.bindings.soft_drop) {
            match key.kind {
                KeyEventKind::Press => {
                    let was_down = self.down.is_down;
                    self.down.press(now);
                    if !was_down {
                        out.push(Command::SoftDrop);
                    }
                }
                KeyEventKind::Release => self.down.release(),
                KeyEventKind::Repeat => self.down.press(now),
            }
            return;
        }

        match (key.kind, key.code) {
            (KeyEventKind::Press, code) if matches_keycode(code, self.bindings.rotate_left) => {
                out.push(Command::RotateLeft)
            }
            (KeyEventKind::Press, code) if matches_keycode(code, self.bindings.rotate_right) => {
                out.push(Command::RotateRight)
            }
            (KeyEventKind::Press, code) if matches_keycode(code, self.bindings.rotate_180) => {
                out.push(Command::Rotate180)
            }
            (KeyEventKind::Press, code) if matches_keycode(code, self.bindings.hold) => {
                out.push(Command::Hold)
            }
            (KeyEventKind::Press, code) if matches_keycode(code, self.bindings.hard_drop) => {
                out.push(Command::HardDrop)
            }
            (KeyEventKind::Press, code) if is_quit_key(code, self.bindings.quit) => {
                out.push(Command::Quit)
            }
            _ => {}
        }
    }

    pub fn soft_drop_active(&self) -> bool {
        self.down.is_down
    }

    pub fn enhanced_input_active(&self) -> bool {
        self.enhanced_input
    }

    pub fn clear_motion(&mut self) {
        self.left.release();
        self.right.release();
        self.down.release();
        self.last_dir = None;
    }

    pub fn lockout_movement(&mut self, now: Instant, duration_ms: u64) {
        self.clear_motion();
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

fn should_repeat(key: HeldKey, now: Instant, das_ms: u64, arr_ms: u64) -> bool {
    if !key.is_down {
        return false;
    }
    let pressed_at = match key.pressed_at {
        Some(t) => t,
        None => return false,
    };
    if now.duration_since(pressed_at) < Duration::from_millis(das_ms) {
        return false;
    }
    let last = match key.last_repeat {
        Some(t) => t,
        None => return true,
    };
    now.duration_since(last) >= Duration::from_millis(arr_ms)
}

fn auto_release_if_idle(key: &mut HeldKey, now: Instant, timeout_ms: u64) {
    if !key.is_down {
        return;
    }
    if let Some(last) = key.last_event {
        if now.duration_since(last) >= Duration::from_millis(timeout_ms) {
            key.release();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Left,
    Right,
}

fn matches_keycode(actual: KeyCode, expected: KeyCode) -> bool {
    match (actual, expected) {
        (KeyCode::Char(a), KeyCode::Char(b)) => a.to_ascii_lowercase() == b.to_ascii_lowercase(),
        _ => actual == expected,
    }
}

fn is_quit_key(actual: KeyCode, expected: KeyCode) -> bool {
    matches_keycode(actual, expected) || matches!(actual, KeyCode::Esc)
}
