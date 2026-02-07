use crate::core::board::{Board, BOARD_HEIGHT, BOARD_WIDTH};
use crate::core::command::Command;
use crate::core::events::Event;
use crate::core::piece::{Piece, Rotation};
use crate::core::state::GameState;

#[derive(Debug, Clone, Copy)]
pub struct RulesConfig {
    pub gravity_ms: u64,
    pub soft_drop_factor: u32,
}

pub fn step(
    state: &mut GameState,
    cfg: RulesConfig,
    dt_ms: u64,
    commands: &[Command],
    soft_drop_active: bool,
) -> Vec<Event> {
    let mut events = Vec::new();
    if state.game_over {
        return events;
    }

    let mut soft_drop_pressed = false;
    for cmd in commands {
        match cmd {
            Command::MoveLeft => {
                try_move(state, -1, 0);
            }
            Command::MoveRight => {
                try_move(state, 1, 0);
            }
            Command::SoftDrop => {
                soft_drop_pressed = true;
                if !try_move(state, 0, 1) {
                    lock_piece(state, &mut events);
                }
            }
            Command::HardDrop => {
                hard_drop(state, &mut events);
            }
            Command::RotateLeft => {
                try_rotate(state, state.active.rotation.ccw());
            }
            Command::RotateRight => {
                try_rotate(state, state.active.rotation.cw());
            }
            Command::Rotate180 => {
                try_rotate(state, state.active.rotation.half());
            }
            Command::Hold => {
                hold_piece(state, &mut events);
            }
            Command::Quit => {
                state.game_over = true;
                events.push(Event::GameOver);
            }
        }
    }

    let effective_gravity = if soft_drop_active || soft_drop_pressed {
        let factor = cfg.soft_drop_factor.max(1) as u64;
        (cfg.gravity_ms / factor).max(1)
    } else {
        cfg.gravity_ms
    };

    state.gravity_accum_ms = state.gravity_accum_ms.saturating_add(dt_ms);
    while state.gravity_accum_ms >= effective_gravity {
        state.gravity_accum_ms -= effective_gravity;
        if !try_move(state, 0, 1) {
            lock_piece(state, &mut events);
            break;
        }
    }

    events
}

pub fn ghost_piece(state: &GameState) -> Piece {
    let mut ghost = state.active;
    while can_place(&state.board, offset_piece(ghost, 0, 1)) {
        ghost.y += 1;
    }
    ghost
}

fn hold_piece(state: &mut GameState, events: &mut Vec<Event>) {
    if state.hold_used {
        return;
    }
    let current = state.active.kind;
    if let Some(held) = state.hold {
        state.hold = Some(current);
        state.reset_active(held);
    } else {
        state.hold = Some(current);
        let next = state.pop_next();
        state.reset_active(next);
    }
    state.hold_used = true;
    if !can_place(&state.board, state.active) {
        state.game_over = true;
        events.push(Event::GameOver);
    }
}

fn hard_drop(state: &mut GameState, events: &mut Vec<Event>) {
    while try_move(state, 0, 1) {}
    lock_piece(state, events);
}

fn lock_piece(state: &mut GameState, events: &mut Vec<Event>) {
    for (x, y) in state.active.blocks() {
        state.board.set(x, y, Some(state.active.kind));
    }
    events.push(Event::Locked);
    let cleared = state.board.clear_full_rows();
    if cleared > 0 {
        state.lines = state.lines.saturating_add(cleared as u32);
        events.push(Event::LinesCleared(cleared as u8));
    }

    let next = state.pop_next();
    state.reset_active(next);
    state.hold_used = false;

    if !can_place(&state.board, state.active) {
        state.game_over = true;
        events.push(Event::GameOver);
    }
}

fn try_move(state: &mut GameState, dx: i32, dy: i32) -> bool {
    let candidate = offset_piece(state.active, dx, dy);
    if can_place(&state.board, candidate) {
        state.active = candidate;
        true
    } else {
        false
    }
}

fn try_rotate(state: &mut GameState, rotation: Rotation) -> bool {
    let mut candidate = state.active;
    candidate.rotation = rotation;
    if can_place(&state.board, candidate) {
        state.active = candidate;
        true
    } else {
        false
    }
}

fn offset_piece(piece: Piece, dx: i32, dy: i32) -> Piece {
    Piece {
        x: piece.x + dx,
        y: piece.y + dy,
        ..piece
    }
}

fn can_place(board: &Board, piece: Piece) -> bool {
    for (x, y) in piece.blocks() {
        if x < 0 || x >= BOARD_WIDTH as i32 || y < 0 || y >= BOARD_HEIGHT as i32 {
            return false;
        }
        if board.get(x, y).is_some() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::piece::PieceType;
    use crate::core::state::GameState;

    #[test]
    fn collision_detected() {
        let mut state = GameState::new(1);
        state.board.set(0, 0, Some(PieceType::I));
        let piece = Piece {
            kind: PieceType::O,
            rotation: Rotation(0),
            x: 0,
            y: 0,
        };
        assert!(!can_place(&state.board, piece));
    }
}
