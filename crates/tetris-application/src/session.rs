use std::time::{Duration, Instant};

use tetris_domain::state::GameState;

use crate::flow::GameMode;

#[derive(Debug)]
pub struct GameSessionState {
    pub mode: GameMode,
    pub state: GameState,
    pub start_time: Instant,
    pub last_frame: Instant,
    pub paused_at: Option<Instant>,
    pub paused_total: Duration,
    pub game_over_time: Option<u64>,
    pub time_up: bool,
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
            time_up: false,
        }
    }

    pub fn elapsed_secs(&self, now: Instant) -> u64 {
        self.game_over_time.unwrap_or_else(|| {
            let effective_now = self.paused_at.unwrap_or(now);
            let elapsed = effective_now
                .duration_since(self.start_time)
                .saturating_sub(self.paused_total);
            elapsed.as_secs()
        })
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
}
