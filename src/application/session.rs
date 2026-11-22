use std::time::{Duration, Instant};

use crate::domain::state::GameState;

use super::flow::GameMode;

#[derive(Debug)]
pub struct GameSessionState {
    pub mode: GameMode,
    pub state: GameState,
    pub start_time: Instant,
    pub last_frame: Instant,
    pub paused_at: Option<Instant>,
    pub paused_total: Duration,
    pub game_over_time: Option<u64>,
    pub game_over_ms: Option<u64>,
    pub time_up: bool,
}

#[derive(Debug)]
pub struct NetSessionState {
    pub state: GameState,
    pub start_time: Instant,
    pub last_frame: Instant,
    pub remote_over: bool,
    pub remote_disconnect: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    TimeUp,
    GoalReached,
}

impl GameSessionState {
    pub fn new(seed: u64, mode: GameMode) -> Self {
        let now = Instant::now();
        let mut state = GameState::new(seed);
        if let GameMode::Cheese { target } = mode {
            state.seed_cheese(target, seed);
        }
        Self {
            mode,
            state,
            start_time: now,
            last_frame: now,
            paused_at: None,
            paused_total: Duration::from_secs(0),
            game_over_time: None,
            game_over_ms: None,
            time_up: false,
        }
    }

    pub fn elapsed_ms(&self, now: Instant) -> u64 {
        self.game_over_ms.unwrap_or_else(|| {
            let effective_now = self.paused_at.unwrap_or(now);
            let elapsed = effective_now
                .duration_since(self.start_time)
                .saturating_sub(self.paused_total);
            elapsed.as_millis() as u64
        })
    }

    pub fn elapsed_secs(&self, now: Instant) -> u64 {
        self.game_over_time
            .unwrap_or_else(|| self.elapsed_ms(now) / 1000)
    }

    pub fn remaining_ms(&self, now: Instant) -> Option<u64> {
        let limit_ms = self.mode.time_limit_secs()?.saturating_mul(1000);
        Some(limit_ms.saturating_sub(self.elapsed_ms(now)))
    }

    pub fn remaining_secs(&self, now: Instant) -> Option<u64> {
        let limit = self.mode.time_limit_secs()?;
        let elapsed = self.elapsed_secs(now);
        Some(limit.saturating_sub(elapsed))
    }

    pub fn pause(&mut self, now: Instant) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_total = self
                .paused_total
                .saturating_add(now.duration_since(paused_at));
            self.last_frame = now;
        }
    }

    pub fn apply_mode_end_if_reached(&mut self, now: Instant) -> Option<SessionEnd> {
        if let Some(limit) = self.mode.time_limit_secs()
            && self.elapsed_ms(now) >= limit.saturating_mul(1000)
        {
            self.game_over_time = Some(limit);
            self.game_over_ms = Some(limit.saturating_mul(1000));
            self.time_up = true;
            self.state.game_over = true;
            return Some(SessionEnd::TimeUp);
        }

        if let GameMode::FortyLines { target } = self.mode
            && self.state.lines >= target
        {
            let ms = self.elapsed_ms(now);
            self.game_over_ms = Some(ms);
            self.game_over_time = Some(ms / 1000);
            self.time_up = true;
            self.state.game_over = true;
            return Some(SessionEnd::GoalReached);
        }

        if let GameMode::Cheese { .. } = self.mode
            && self.state.garbage_remaining == 0
        {
            let ms = self.elapsed_ms(now);
            self.game_over_ms = Some(ms);
            self.game_over_time = Some(ms / 1000);
            self.state.game_over = true;
            return Some(SessionEnd::GoalReached);
        }

        None
    }
}

impl NetSessionState {
    pub fn new(seed: u64) -> Self {
        let now = Instant::now();
        Self {
            state: GameState::new(seed),
            start_time: now,
            last_frame: now,
            remote_over: false,
            remote_disconnect: false,
        }
    }

    pub fn elapsed_ms(&self, now: Instant) -> u64 {
        now.duration_since(self.start_time).as_millis() as u64
    }

    pub fn elapsed_secs(&self, now: Instant) -> u64 {
        self.elapsed_ms(now) / 1000
    }
}
