use std::collections::VecDeque;

use crate::core::board::Board;
use crate::core::piece::{Piece, PieceType, Rotation};
use crate::core::rng::{BagRng, PieceRng};

const SPAWN_X: i32 = 3;
const SPAWN_Y: i32 = 0;

#[derive(Debug)]
pub struct GameState {
    pub board: Board,
    pub active: Piece,
    pub hold: Option<PieceType>,
    pub hold_used: bool,
    pub queue: VecDeque<PieceType>,
    pub rng: BagRng,
    pub game_over: bool,
    pub gravity_accum_ms: u64,
    pub lock_delay_ms: u64,
    pub lock_reset_count: u32,
    pub grounded: bool,
    pub lines: u32,
    pub score: u32,
    pub level: u32,
}

impl GameState {
    pub fn new(seed: u64) -> Self {
        let mut rng = BagRng::new(seed);
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
            game_over: false,
            gravity_accum_ms: 0,
            lock_delay_ms: 0,
            lock_reset_count: 0,
            grounded: false,
            lines: 0,
            score: 0,
            level: 1,
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
        self.lock_delay_ms = 0;
        self.lock_reset_count = 0;
        self.grounded = false;
    }

    pub fn pop_next(&mut self) -> PieceType {
        while self.queue.len() < 6 {
            self.queue.push_back(self.rng.next_piece());
        }
        self.queue.pop_front().unwrap_or_else(|| self.rng.next_piece())
    }

    pub fn peek_next(&self, count: usize) -> Vec<PieceType> {
        self.queue.iter().take(count).copied().collect()
    }
}
