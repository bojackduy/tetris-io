use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingMode {
    Down,
    Rng,
}

#[derive(Debug, Clone, Copy)]
pub struct AttackTable {
    pub single: u32,
    pub double: u32,
    pub triple: u32,
    pub quad: u32,
    pub t_spin_zero: u32,
    pub t_spin_single: u32,
    pub t_spin_double: u32,
    pub t_spin_triple: u32,
    pub mini_zero: u32,
    pub mini_single: u32,
    pub mini_double: u32,
}

impl Default for AttackTable {
    fn default() -> Self {
        Self {
            single: 0,
            double: 1,
            triple: 2,
            quad: 4,
            t_spin_zero: 0,
            t_spin_single: 2,
            t_spin_double: 4,
            t_spin_triple: 6,
            mini_zero: 0,
            mini_single: 0,
            mini_double: 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GarbageConfig {
    pub attack: AttackTable,
    pub combo_multiplier_step: f32,
    pub combo_log_multiplier: f32,
    pub rounding: RoundingMode,
    pub b2b_bonus: u32,
    pub surge_start: u32,
    pub perfect_clear_bonus: u32,
    pub opener_phase_pieces: u32,
    pub opener_cancel_multiplier: u32,
}

impl Default for GarbageConfig {
    fn default() -> Self {
        Self {
            attack: AttackTable::default(),
            combo_multiplier_step: 0.25,
            combo_log_multiplier: 1.25,
            rounding: RoundingMode::Down,
            b2b_bonus: 1,
            surge_start: 4,
            perfect_clear_bonus: 10,
            opener_phase_pieces: 14,
            opener_cancel_multiplier: 2,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GarbageChunk {
    pub lines: u32,
    pub hole: usize,
}

#[derive(Debug, Default, Clone)]
pub struct GarbageQueue {
    chunks: VecDeque<GarbageChunk>,
}

impl GarbageQueue {
    pub fn total_lines(&self) -> u32 {
        self.chunks.iter().map(|chunk| chunk.lines).sum()
    }

    pub fn push(&mut self, chunk: GarbageChunk) {
        if chunk.lines == 0 {
            return;
        }
        self.chunks.push_back(chunk);
    }

    pub fn cancel_lines(&mut self, mut lines: u32) -> u32 {
        let mut cancelled = 0u32;
        while lines > 0 {
            let Some(front) = self.chunks.front_mut() else {
                break;
            };
            let take = lines.min(front.lines);
            front.lines = front.lines.saturating_sub(take);
            lines = lines.saturating_sub(take);
            cancelled = cancelled.saturating_add(take);
            if front.lines == 0 {
                self.chunks.pop_front();
            }
        }
        cancelled
    }

    pub fn drain_all(&mut self) -> Vec<GarbageChunk> {
        self.chunks.drain(..).collect()
    }
}
