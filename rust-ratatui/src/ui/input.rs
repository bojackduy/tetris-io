use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind};

use crate::core::command::Command;

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
            self.repeat_armed = false;
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
            self.repeat_armed = false;
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
    left: HeldKey,
    right: HeldKey,
    down: HeldKey,
    last_dir: Option<Dir>,
}

impl InputState {
    pub fn new(cfg: InputConfig) -> Self {
        Self {
            cfg,
            left: HeldKey::default(),
            right: HeldKey::default(),
            down: HeldKey::default(),
            last_dir: None,
        }
    }

    pub fn handle_event(&mut self, event: Event, now: Instant, out: &mut Vec<Command>) {
        if let Event::Key(key) = event {
            self.handle_key(key, now, out);
        }
    }

    pub fn repeat_commands(&mut self, now: Instant, out: &mut Vec<Command>) {
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
                }
            }
            Some(Dir::Right) => {
                if self.right.repeat_armed && should_repeat(self.right, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveRight);
                    self.right.last_repeat = Some(now);
                }
            }
            None => {
                if self.left.repeat_armed && should_repeat(self.left, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveLeft);
                    self.left.last_repeat = Some(now);
                }
                if self.right.repeat_armed && should_repeat(self.right, now, self.cfg.das_ms, self.cfg.arr_ms) {
                    out.push(Command::MoveRight);
                    self.right.last_repeat = Some(now);
                }
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent, now: Instant, out: &mut Vec<Command>) {
        match (key.code, key.kind) {
            (KeyCode::Left, KeyEventKind::Press) => {
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
            (KeyCode::Right, KeyEventKind::Press) => {
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
            (KeyCode::Down, KeyEventKind::Press) => {
                let was_down = self.down.is_down;
                self.down.press(now);
                if !was_down {
                    out.push(Command::SoftDrop);
                }
            }
            (KeyCode::Left, KeyEventKind::Release) => {
                self.left.release();
                if !self.right.is_down {
                    self.last_dir = None;
                }
            }
            (KeyCode::Right, KeyEventKind::Release) => {
                self.right.release();
                if !self.left.is_down {
                    self.last_dir = None;
                }
            }
            (KeyCode::Down, KeyEventKind::Release) => self.down.release(),
            (KeyCode::Left, KeyEventKind::Repeat) => {
                self.right.release();
                self.left.press(now);
            }
            (KeyCode::Right, KeyEventKind::Repeat) => {
                self.left.release();
                self.right.press(now);
            }
            (KeyCode::Down, KeyEventKind::Repeat) => {
                self.down.press(now);
            }
            (KeyCode::Char('z'), KeyEventKind::Press)
            | (KeyCode::Char('Z'), KeyEventKind::Press) => out.push(Command::RotateLeft),
            (KeyCode::Up, KeyEventKind::Press) => out.push(Command::RotateRight),
            (KeyCode::Char('a'), KeyEventKind::Press)
            | (KeyCode::Char('A'), KeyEventKind::Press) => out.push(Command::Rotate180),
            (KeyCode::Char('c'), KeyEventKind::Press)
            | (KeyCode::Char('C'), KeyEventKind::Press) => out.push(Command::Hold),
            (KeyCode::Char('q'), KeyEventKind::Press)
            | (KeyCode::Char('Q'), KeyEventKind::Press)
            | (KeyCode::Esc, KeyEventKind::Press) => out.push(Command::Quit),
            (KeyCode::Char(' '), KeyEventKind::Press) => out.push(Command::HardDrop),
            _ => {}
        }
    }

    pub fn soft_drop_active(&self) -> bool {
        self.down.is_down
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
