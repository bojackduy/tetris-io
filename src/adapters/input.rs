use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::domain::board::BOARD_WIDTH;
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

    fn opposite(self) -> Self {
        match self {
            Dir::Left => Dir::Right,
            Dir::Right => Dir::Left,
        }
    }
}

/// Release-aware input uses physical press edges and software DAS/ARR.
///
/// Without keyboard enhancement, a tap and a hold before the first OS repeat
/// have identical event streams. Legacy input therefore emits one command per
/// OS Press, with no inferred releases, software ARR, or held soft drop. Its
/// speed and repeated actions depend on the OS keyboard repeat settings.
pub struct InputState {
    cfg: InputConfig,
    bindings: KeyBindings,
    // Shared across capability detection so the first Repeat cannot lose a Press.
    left_pressed_at: Option<Instant>,
    right_pressed_at: Option<Instant>,
    down_held: bool,
    active_dir: Option<Dir>,
    next_repeat_at: Option<Instant>,
    lockout_until: Option<Instant>,
    enhanced_input: bool,
    key_presses: u32,
}

impl InputState {
    pub fn new(cfg: InputConfig, bindings: KeyBindings) -> Self {
        Self {
            cfg,
            bindings,
            left_pressed_at: None,
            right_pressed_at: None,
            down_held: false,
            active_dir: None,
            next_repeat_at: None,
            lockout_until: None,
            enhanced_input: false,
            key_presses: 0,
        }
    }

    /// Set the negotiated terminal capability before processing keyboard input.
    /// Existing physical state is retained if capability is learned mid-hold.
    pub fn set_enhanced_input(&mut self, enabled: bool) {
        self.enhanced_input = enabled;
    }

    pub fn handle_event(&mut self, event: Event, now: Instant, out: &mut Vec<Command>) {
        match event {
            Event::Key(key) => self.handle_key(key, now, out),
            Event::FocusLost => self.clear_motion(),
            _ => {}
        }
    }

    /// Emit every elapsed ARR tick, anchored to the DAS deadline, not the frame.
    pub fn repeat_commands(&mut self, now: Instant, out: &mut Vec<Command>) {
        if self.lockout_active(now) || !self.enhanced_input {
            return;
        }
        let (Some(dir), Some(mut next)) = (self.active_dir, self.next_repeat_at) else {
            return;
        };
        if now < next {
            return;
        }
        if self.cfg.arr_ms == 0 {
            // Reapply on every poll so a new piece or changed board also reaches
            // the wall. Board width bounds work even when movement is blocked.
            out.extend(std::iter::repeat_n(dir.to_command(), BOARD_WIDTH));
            return;
        }
        let interval = Duration::from_millis(self.cfg.arr_ms);
        while next <= now {
            out.push(dir.to_command());
            next += interval;
        }
        self.next_repeat_at = Some(next);
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant, out: &mut Vec<Command>) {
        let direction = if matches_keycode(key.code, self.bindings.move_left) {
            Some(Dir::Left)
        } else if matches_keycode(key.code, self.bindings.move_right) {
            Some(Dir::Right)
        } else {
            None
        };
        let is_down = matches_keycode(key.code, self.bindings.soft_drop);
        let action = self.action_for(key.code);
        if direction.is_none() && !is_down && action.is_none() {
            return;
        }

        // Only an actual Release proves the terminal can end a held key.
        // Promoting on Repeat would leave held keys permanently stuck on terminals
        // that emit Repeat without Release.
        if !self.enhanced_input && key.kind == KeyEventKind::Release {
            self.set_enhanced_input(true);
            // Legacy Presses already moved once each. Do not replay software
            // ticks from before enhancement was discovered.
            if let Some(next) = self.next_repeat_at {
                self.next_repeat_at = Some(next.max(now));
            }
        }
        let locked = self.lockout_active(now);

        if let Some(dir) = direction {
            self.handle_direction(dir, key.kind, now, locked, out);
        } else if is_down {
            match key.kind {
                KeyEventKind::Press => {
                    // Some Kitty-capable terminals (e.g. WezTerm on macOS) emit
                    // Press instead of Repeat for OS key-repeat while still sending
                    // Release on key-up. Ignore duplicate Press while held.
                    let was_held = self.down_held;
                    self.down_held = true;
                    if !locked && (!self.enhanced_input || !was_held) {
                        out.push(Command::SoftDrop);
                        self.key_presses = self.key_presses.saturating_add(1);
                    }
                }
                KeyEventKind::Release => self.down_held = false,
                KeyEventKind::Repeat => {
                    if !self.enhanced_input && !locked {
                        out.push(Command::SoftDrop);
                        self.key_presses = self.key_presses.saturating_add(1);
                    }
                }
            }
        } else if let Some(command) = action {
            match key.kind {
                KeyEventKind::Press => {
                    // Fire on every Press edge and ignore Repeat/Release.
                    // Do not latch action keys waiting for Release: terminals like
                    // WezTerm 20240203 send Release for arrows/Enter but raw ASCII
                    // Press-only for Space/letters, which would deadlock HardDrop.
                    if !locked {
                        out.push(command);
                        if command != Command::Quit {
                            self.key_presses = self.key_presses.saturating_add(1);
                        }
                    }
                }
                KeyEventKind::Release | KeyEventKind::Repeat => {}
            }
        }
    }

    fn handle_direction(
        &mut self,
        dir: Dir,
        kind: KeyEventKind,
        now: Instant,
        locked: bool,
        out: &mut Vec<Command>,
    ) {
        match kind {
            KeyEventKind::Press => {
                let pressed_at = match dir {
                    Dir::Left => &mut self.left_pressed_at,
                    Dir::Right => &mut self.right_pressed_at,
                };
                // Some Kitty-capable terminals (e.g. WezTerm on macOS) send Press
                // instead of Repeat for OS key-repeat while still sending Release.
                // Ignore duplicate Press while held so DAS/ARR is not disturbed.
                if self.enhanced_input && pressed_at.is_some() {
                    return;
                }
                // In legacy mode each Press may be a new tap. Keep the latest
                // observed press rather than claiming an unobservable hold.
                *pressed_at = Some(now);
                self.active_dir = Some(dir);
                self.next_repeat_at = Some(now + Duration::from_millis(self.cfg.das_ms));
                self.apply_lockout_cutoff();
                if !locked {
                    out.push(dir.to_command());
                    self.key_presses = self.key_presses.saturating_add(1);
                }
            }
            KeyEventKind::Release => {
                match dir {
                    Dir::Left => self.left_pressed_at = None,
                    Dir::Right => self.right_pressed_at = None,
                }
                if self.active_dir == Some(dir) {
                    let opposite = dir.opposite();
                    let pressed_at = match opposite {
                        Dir::Left => self.left_pressed_at,
                        Dir::Right => self.right_pressed_at,
                    };
                    self.active_dir = pressed_at.map(|_| opposite);
                    // A still-held opposite charges from its own press. DCD is
                    // a cutoff on resuming it, never free DAS on a fresh press.
                    self.next_repeat_at = pressed_at.map(|start| {
                        (start + Duration::from_millis(self.cfg.das_ms))
                            .max(now + Duration::from_millis(self.cfg.dcd_ms))
                    });
                    self.apply_lockout_cutoff();
                }
            }
            KeyEventKind::Repeat => {
                // In legacy/broken-release mode, forward OS Repeat events as discrete
                // moves so holding a key still moves without sticking to a wall.
                if !self.enhanced_input && !locked {
                    out.push(dir.to_command());
                    self.key_presses = self.key_presses.saturating_add(1);
                }
            }
        }
    }

    fn action_for(&self, code: KeyCode) -> Option<Command> {
        if matches_keycode(code, self.bindings.rotate_left) {
            Some(Command::RotateLeft)
        } else if matches_keycode(code, self.bindings.rotate_right) {
            Some(Command::RotateRight)
        } else if matches_keycode(code, self.bindings.rotate_180) {
            Some(Command::Rotate180)
        } else if matches_keycode(code, self.bindings.hold) {
            Some(Command::Hold)
        } else if matches_keycode(code, self.bindings.hard_drop) {
            Some(Command::HardDrop)
        } else if is_quit_key(code, self.bindings.quit) {
            Some(Command::Quit)
        } else {
            None
        }
    }

    /// Held soft drop is reliable only with release-aware keyboard input.
    /// The core owns its repeat speed, independently of horizontal DAS/ARR.
    pub fn soft_drop_active(&self) -> bool {
        self.enhanced_input && self.down_held && self.lockout_until.is_none()
    }

    pub fn enhanced_input_active(&self) -> bool {
        self.enhanced_input
    }

    /// Accepted manual movement, rotation, hold, and drop presses this session.
    /// Legacy terminals cannot distinguish OS Press repeats from fresh taps,
    /// so their count is approximate. Pausing or clearing motion keeps the total.
    pub fn key_presses(&self) -> u32 {
        self.key_presses
    }

    pub fn clear_motion(&mut self) {
        self.left_pressed_at = None;
        self.right_pressed_at = None;
        self.down_held = false;
        self.active_dir = None;
        self.next_repeat_at = None;
        self.lockout_until = None;
    }

    pub fn lockout_movement(&mut self, now: Instant, duration_ms: u64) {
        self.lockout_until = Some(now + Duration::from_millis(duration_ms));
        self.apply_lockout_cutoff();
    }

    fn apply_lockout_cutoff(&mut self) {
        if let (Some(next), Some(until)) = (self.next_repeat_at, self.lockout_until) {
            self.next_repeat_at = Some(next.max(until));
        }
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
