use std::collections::VecDeque;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use super::board::{BOARD_HEIGHT, BOARD_WIDTH, Board, DEADLINE_Y, PLAY_HEIGHT};
use super::garbage::{GarbageChunk, GarbageConfig, GarbageQueue};
use super::piece::{Piece, PieceType, Rotation};
use super::rng::{BagRng, PieceRng};

const SPAWN_X: i32 = 3;
// Verified in Jstris getBlockSets(): I cells sit on row VISIBLE_START_Y (y=1),
// while O/T/S/Z/J/L straddle DEADLINE_Y (y=0) and VISIBLE_START_Y (y=1).
pub const SPAWN_Y: i32 = DEADLINE_Y;

#[derive(Debug)]
pub struct GameState {
    pub board: Board,
    pub active: Piece,
    pub hold: Option<PieceType>,
    pub hold_used: bool,
    pub queue: VecDeque<PieceType>,
    pub rng: BagRng,
    pub garbage_rng: StdRng,
    pub garbage_queue: GarbageQueue,
    pub garbage_config: GarbageConfig,
    pub pieces_placed: u32,
    pub game_over: bool,
    pub gravity_accum_ms: u64,
    pub soft_drop_accum_millirows: u64,
    pub lock_delay_ms: u64,
    pub lock_reset_count: u32,
    pub grounded: bool,
    pub lock_started: bool,
    pub elapsed_since_spawn_ms: u64,
    pub elapsed_since_first_contact_ms: u64,
    pub combo: u32,
    pub b2b: u32,
    pub last_clear_label: Option<String>,
    pub last_action_rotation: bool,
    pub lines: u32,
    pub score: u32,
    pub attack: u32,
    pub finesse_faults: u32,
    pub piece_manual_inputs: u32,
    pub last_clear_was_pc: bool,
    pub level: u32,
    pub garbage_rows: [bool; BOARD_HEIGHT],
    pub garbage_remaining: u32,
}

impl GameState {
    pub fn new(seed: u64) -> Self {
        let mut rng = BagRng::new(seed);
        let garbage_rng = StdRng::seed_from_u64(seed ^ 0xD1B54A32D192ED03);
        let mut queue = VecDeque::new();
        for _ in 0..6 {
            queue.push_back(rng.next_piece());
        }
        let active = Piece::new(queue.pop_front().unwrap_or(PieceType::I), SPAWN_X, SPAWN_Y);
        Self {
            board: Board::new(),
            active,
            hold: None,
            hold_used: false,
            queue,
            rng,
            garbage_rng,
            garbage_queue: GarbageQueue::default(),
            garbage_config: GarbageConfig::default(),
            pieces_placed: 0,
            game_over: false,
            gravity_accum_ms: 0,
            soft_drop_accum_millirows: 0,
            lock_delay_ms: 0,
            lock_reset_count: 0,
            grounded: false,
            lock_started: false,
            elapsed_since_spawn_ms: 0,
            elapsed_since_first_contact_ms: 0,
            combo: 0,
            b2b: 0,
            last_clear_label: None,
            last_action_rotation: false,
            lines: 0,
            score: 0,
            attack: 0,
            finesse_faults: 0,
            piece_manual_inputs: 0,
            last_clear_was_pc: false,
            level: 1,
            garbage_rows: [false; BOARD_HEIGHT],
            garbage_remaining: 0,
        }
    }

    pub fn reset_active(&mut self, kind: PieceType) {
        self.active = Piece {
            kind,
            rotation: Rotation(0),
            x: SPAWN_X,
            y: SPAWN_Y,
        };
        self.gravity_accum_ms = 0;
        self.soft_drop_accum_millirows = 0;
        self.lock_delay_ms = 0;
        self.lock_reset_count = 0;
        self.grounded = false;
        self.lock_started = false;
        self.elapsed_since_spawn_ms = 0;
        self.elapsed_since_first_contact_ms = 0;
        self.last_action_rotation = false;
        self.piece_manual_inputs = 0;
    }

    pub fn add_finesse_inputs(&mut self, inputs: u32) {
        self.piece_manual_inputs = self.piece_manual_inputs.saturating_add(inputs);
    }

    pub fn pop_next(&mut self) -> PieceType {
        while self.queue.len() < 6 {
            self.queue.push_back(self.rng.next_piece());
        }
        self.queue
            .pop_front()
            .unwrap_or_else(|| self.rng.next_piece())
    }

    pub fn peek_next(&self, count: usize) -> Vec<PieceType> {
        self.queue.iter().take(count).copied().collect()
    }

    pub fn seed_cheese(&mut self, target_lines: u32, seed: u64) {
        let max_lines = PLAY_HEIGHT as u32;
        let lines = target_lines.min(max_lines);
        if lines == 0 {
            return;
        }

        self.garbage_rows = [false; BOARD_HEIGHT];
        self.garbage_remaining = lines;

        let mut rng = StdRng::seed_from_u64(seed ^ 0x9E3779B97F4A7C15);
        for i in 0..lines {
            let row = BOARD_HEIGHT - 1 - i as usize;
            self.garbage_rows[row] = true;
            let hole = rng.gen_range(0..BOARD_WIDTH);
            for x in 0..BOARD_WIDTH {
                if x != hole {
                    self.board
                        .set_garbage(x as i32, row as i32, Some(PieceType::O));
                }
            }
        }
    }

    pub fn apply_row_clear_mask(&mut self, cleared_mask: &[bool; BOARD_HEIGHT]) -> u32 {
        if self.garbage_remaining == 0 {
            return 0;
        }

        let mut cleared_garbage = 0u32;
        let mut dst = BOARD_HEIGHT as i32 - 1;
        for src in (0..BOARD_HEIGHT as i32).rev() {
            if cleared_mask[src as usize] {
                if self.garbage_rows[src as usize] {
                    cleared_garbage = cleared_garbage.saturating_add(1);
                }
            } else {
                if dst != src {
                    self.garbage_rows[dst as usize] = self.garbage_rows[src as usize];
                }
                dst -= 1;
            }
        }
        while dst >= 0 {
            self.garbage_rows[dst as usize] = false;
            dst -= 1;
        }

        self.garbage_remaining = self.garbage_remaining.saturating_sub(cleared_garbage);
        cleared_garbage
    }

    pub fn opener_active(&self) -> bool {
        self.pieces_placed < self.garbage_config.opener_phase_pieces
    }

    pub fn pending_garbage(&self) -> u32 {
        self.garbage_queue.total_lines()
    }

    pub fn receive_garbage(&mut self, lines: u32) {
        if lines == 0 {
            return;
        }
        let hole = self.garbage_rng.gen_range(0..BOARD_WIDTH);
        self.garbage_queue.push(GarbageChunk { lines, hole });
    }

    pub fn cancel_garbage(&mut self, lines: u32) -> u32 {
        self.garbage_queue.cancel_lines(lines)
    }

    pub fn apply_pending_garbage(&mut self) -> bool {
        let chunks = self.garbage_queue.drain_all();
        let mut top_out = false;
        for chunk in chunks {
            let holes = vec![chunk.hole; chunk.lines as usize];
            if self.board.insert_garbage(chunk.lines, &holes) {
                top_out = true;
            }
        }
        top_out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_game_spawns_at_top_of_visible_field() {
        let state = GameState::new(1);

        assert_eq!(state.active.x, SPAWN_X);
        assert_eq!(state.active.y, SPAWN_Y);
        assert_eq!(state.active.rotation, Rotation(0));
        // Bottom row of every spawned piece sits on the top visible row (VISIBLE_START_Y),
        // while non-I pieces have their top row in the single deadline buffer row (DEADLINE_Y).
        let max_y = state.active.blocks().iter().map(|&(_, y)| y).max().unwrap();
        assert_eq!(max_y, super::super::board::VISIBLE_START_Y as i32);
    }

    #[test]
    fn reset_active_restores_spawn_and_clears_piece_timers() {
        let mut state = GameState::new(1);
        state.active.x = 0;
        state.active.y = BOARD_HEIGHT as i32 - 2;
        state.active.rotation = Rotation(3);
        state.gravity_accum_ms = 91;
        state.soft_drop_accum_millirows = 800;
        state.lock_delay_ms = 499;
        state.lock_reset_count = 15;
        state.grounded = true;
        state.lock_started = true;
        state.last_action_rotation = true;
        state.elapsed_since_spawn_ms = 19_999;
        state.elapsed_since_first_contact_ms = 4999;

        for kind in PieceType::ALL {
            state.reset_active(kind);

            assert_eq!(state.active, Piece::new(kind, SPAWN_X, SPAWN_Y));
            assert_eq!(state.gravity_accum_ms, 0);
            assert_eq!(state.soft_drop_accum_millirows, 0);
            assert_eq!(state.lock_delay_ms, 0);
            assert_eq!(state.lock_reset_count, 0);
            assert!(!state.grounded);
            assert!(!state.lock_started);
            assert!(!state.last_action_rotation);
            assert_eq!(state.elapsed_since_spawn_ms, 0);
            assert_eq!(state.elapsed_since_first_contact_ms, 0);
        }
    }

    #[test]
    fn cheese_stays_at_bottom_below_spawn() {
        let mut state = GameState::new(1);
        state.seed_cheese(5, 7);

        assert_eq!(state.garbage_remaining, 5);
        for y in 0..BOARD_HEIGHT {
            let filled = state.board.cells()[y]
                .iter()
                .filter(|cell| cell.is_some())
                .count();
            if y >= BOARD_HEIGHT - 5 {
                assert!(state.garbage_rows[y]);
                assert_eq!(filled, BOARD_WIDTH - 1);
            } else {
                assert!(!state.garbage_rows[y]);
                assert_eq!(filled, 0);
            }
        }
        assert!(
            state
                .active
                .blocks()
                .iter()
                .all(|&(x, y)| state.board.get(x, y).is_none())
        );
    }
}
