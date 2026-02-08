use crate::core::board::{Board, BOARD_HEIGHT, BOARD_WIDTH};
use crate::core::command::Command;
use crate::core::events::Event;
use crate::core::piece::{Piece, PieceType, Rotation};
use crate::core::state::GameState;

#[derive(Debug, Clone, Copy)]
pub struct RulesConfig {
    pub gravity_ms: u64,
    pub soft_drop_factor: u32,
    pub lock_delay_ms: u64,
    pub lock_reset_limit: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationDir {
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpinKind {
    TSpin,
    Mini,
    AllMini,
}

pub fn step(
    state: &mut GameState,
    cfg: RulesConfig,
    dt_ms: u64,
    commands: &[Command],
    soft_drop_active: bool,
) -> Vec<Event> {
    let mut events = Vec::new();
    if state.clear_display_ms > 0 {
        state.clear_display_ms = state.clear_display_ms.saturating_sub(dt_ms);
        if state.clear_display_ms == 0 {
            state.last_clear_label = None;
        }
    }
    if state.game_over {
        return events;
    }

    let mut moved_or_rotated = false;
    let mut soft_drop_pressed = false;
    let mut locked_this_frame = false;

    for cmd in commands {
        match cmd {
            Command::MoveLeft => {
                if try_shift(state, -1, 0) {
                    moved_or_rotated = true;
                    state.last_action_rotation = false;
                }
            }
            Command::MoveRight => {
                if try_shift(state, 1, 0) {
                    moved_or_rotated = true;
                    state.last_action_rotation = false;
                }
            }
            Command::SoftDrop => {
                soft_drop_pressed = true;
                if try_shift(state, 0, 1) {
                    moved_or_rotated = true;
                    state.last_action_rotation = false;
                    state.score = state.score.saturating_add(1);
                }
            }
            Command::HardDrop => {
                hard_drop(state, &mut events);
                locked_this_frame = true;
            }
            Command::RotateLeft => {
                if try_rotate(state, RotationDir::Ccw) {
                    moved_or_rotated = true;
                    state.last_action_rotation = true;
                }
            }
            Command::RotateRight => {
                if try_rotate(state, RotationDir::Cw) {
                    moved_or_rotated = true;
                    state.last_action_rotation = true;
                }
            }
            Command::Rotate180 => {
                if try_rotate_180(state) {
                    moved_or_rotated = true;
                    state.last_action_rotation = true;
                }
            }
            Command::Hold => {
                hold_piece(state, &mut events);
                state.last_action_rotation = false;
            }
            Command::Quit => {
                state.game_over = true;
                events.push(Event::GameOver);
            }
        }
        if locked_this_frame || state.game_over {
            break;
        }
    }

    if locked_this_frame || state.game_over {
        return events;
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
        if try_shift(state, 0, 1) {
            moved_or_rotated = true;
            state.last_action_rotation = false;
        } else {
            break;
        }
    }

    let was_grounded = state.grounded;
    let grounded_now = !can_place(&state.board, offset_piece(state.active, 0, 1));

    if grounded_now {
        if !state.lock_started {
            state.lock_started = true;
            state.lock_delay_ms = 0;
        } else if was_grounded && moved_or_rotated && state.lock_reset_count < cfg.lock_reset_limit {
            state.lock_delay_ms = 0;
            state.lock_reset_count = state.lock_reset_count.saturating_add(1);
        }

        state.grounded = true;
        state.lock_delay_ms = state.lock_delay_ms.saturating_add(dt_ms);
        if state.lock_delay_ms >= cfg.lock_delay_ms {
            lock_piece(state, &mut events);
        }
    } else {
        if was_grounded
            && state.lock_started
            && state.lock_reset_count < cfg.lock_reset_limit
        {
            // Leaving the ground grants a limited reset to make tricky placements feel fair.
            state.lock_delay_ms = 0;
            state.lock_reset_count = state.lock_reset_count.saturating_add(1);
        }
        state.grounded = false;
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
    let mut distance = 0u32;
    while try_shift(state, 0, 1) {
        distance = distance.saturating_add(1);
    }
    if distance > 0 {
        state.score = state.score.saturating_add(distance.saturating_mul(2));
    }
    lock_piece(state, events);
}

fn lock_piece(state: &mut GameState, events: &mut Vec<Event>) {
    let spin = detect_spin(state);
    for (x, y) in state.active.blocks() {
        state.board.set(x, y, Some(state.active.kind));
    }
    events.push(Event::Locked);
    let cleared = state.board.clear_full_rows();
    if cleared > 0 {
        events.push(Event::LinesCleared(cleared as u8));
    }
    let perfect_clear = cleared > 0 && is_board_empty(&state.board);
    apply_scoring(state, spin, cleared as u8, perfect_clear);

    let next = state.pop_next();
    state.reset_active(next);
    state.hold_used = false;

    if !can_place(&state.board, state.active) {
        state.game_over = true;
        events.push(Event::GameOver);
    }
}

fn try_shift(state: &mut GameState, dx: i32, dy: i32) -> bool {
    let candidate = offset_piece(state.active, dx, dy);
    if can_place(&state.board, candidate) {
        state.active = candidate;
        true
    } else {
        false
    }
}

fn try_rotate(state: &mut GameState, dir: RotationDir) -> bool {
    if let Some(piece) = rotate_with_kicks(&state.board, state.active, dir) {
        state.active = piece;
        true
    } else {
        false
    }
}

fn try_rotate_180(state: &mut GameState) -> bool {
    if let Some(piece) = rotate_180_with_kicks(&state.board, state.active) {
        state.active = piece;
        true
    } else {
        false
    }
}

fn rotate_180_with_kicks(board: &Board, piece: Piece) -> Option<Piece> {
    let target = piece.rotation.half();
    if piece.kind == PieceType::O {
        let candidate = Piece { rotation: target, ..piece };
        return if can_place(board, candidate) { Some(candidate) } else { None };
    }

    let kicks = kicks_180(piece.rotation);
    for (dx, dy) in kicks {
        let candidate = Piece {
            rotation: target,
            x: piece.x + dx,
            y: piece.y + dy,
            ..piece
        };
        if can_place(board, candidate) {
            return Some(candidate);
        }
    }
    None
}

fn kicks_180(from: Rotation) -> &'static [(i32, i32); 6] {
    match from.0 % 4 {
        0 => &KICKS_180_NS,
        1 => &KICKS_180_EW,
        2 => &KICKS_180_SN,
        3 => &KICKS_180_WE,
        _ => &KICKS_180_NS,
    }
}

fn rotate_with_kicks(board: &Board, piece: Piece, dir: RotationDir) -> Option<Piece> {
    let target = match dir {
        RotationDir::Cw => piece.rotation.cw(),
        RotationDir::Ccw => piece.rotation.ccw(),
    };

    if piece.kind == PieceType::O {
        let candidate = Piece { rotation: target, ..piece };
        return if can_place(board, candidate) {
            Some(candidate)
        } else {
            None
        };
    }

    let kicks = match piece.kind {
        PieceType::I => i_kicks(dir, piece.rotation),
        _ => jlstz_kicks(dir, piece.rotation),
    };

    for (dx, dy) in kicks {
        let candidate = Piece {
            rotation: target,
            x: piece.x + dx,
            y: piece.y + dy,
            ..piece
        };
        if can_place(board, candidate) {
            return Some(candidate);
        }
    }
    None
}

fn jlstz_kicks(dir: RotationDir, from: Rotation) -> &'static [(i32, i32); 5] {
    let idx = from.0 as usize;
    match dir {
        RotationDir::Cw => &JLSTZ_KICKS_CW[idx],
        RotationDir::Ccw => &JLSTZ_KICKS_CCW[idx],
    }
}

fn i_kicks(dir: RotationDir, from: Rotation) -> &'static [(i32, i32); 5] {
    let idx = from.0 as usize;
    match dir {
        RotationDir::Ccw => &I_KICKS_CCW[idx],
        RotationDir::Cw => &I_KICKS_CW[idx],
    }
}

const JLSTZ_KICKS_CW: [[(i32, i32); 5]; 4] = [
    // 0 -> R
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
    // R -> 2
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
    // 2 -> L
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    // L -> 0
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
];

const JLSTZ_KICKS_CCW: [[(i32, i32); 5]; 4] = [
    // 0 -> L
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
    // R -> 0
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
    // 2 -> R
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
    // L -> 2
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
];

const I_KICKS_CCW: [[(i32, i32); 5]; 4] = [
    // 0 -> L
    [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
    // R -> 0
    [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
    // 2 -> R
    [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
    // L -> 2
    [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
];

// SRS+ for I: CW kicks are mirrored from CCW for symmetry.
const I_KICKS_CW: [[(i32, i32); 5]; 4] = [
    // 0 -> R (mirror of 0 -> L)
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    // R -> 2 (mirror of R -> 0)
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    // 2 -> L (mirror of 2 -> R)
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    // L -> 0 (mirror of L -> 2)
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
];

// TETR.IO-style 180 kicks (compact 6-test table).
const KICKS_180_NS: [(i32, i32); 6] = [
    (0, 0),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
];

const KICKS_180_SN: [(i32, i32); 6] = [
    (0, 0),
    (0, -1),
    (-1, 1),
    (1, 1),
    (-1, 0),
    (1, 0),
];

const KICKS_180_EW: [(i32, i32); 6] = [
    (0, 0),
    (-1, 0),
    (1, -1),
    (1, 1),
    (0, -1),
    (0, 1),
];

const KICKS_180_WE: [(i32, i32); 6] = [
    (0, 0),
    (1, 0),
    (-1, 1),
    (-1, -1),
    (0, 1),
    (0, -1),
];

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

const CLEAR_DISPLAY_MS: u64 = 2000;

fn apply_scoring(state: &mut GameState, spin: Option<SpinKind>, cleared: u8, perfect_clear: bool) {
    let had_clear = cleared > 0;
    if had_clear {
        state.lines = state.lines.saturating_add(cleared as u32);
        state.combo = state.combo.saturating_add(1);
    } else {
        state.combo = 0;
    }

    let difficult = is_difficult_clear(spin, cleared);
    if had_clear {
        if difficult {
            state.b2b = state.b2b.saturating_add(1);
        } else {
            state.b2b = 0;
        }
    }

    let mut points = base_points(spin, cleared);
    if difficult && state.b2b >= 2 {
        points = points.saturating_mul(3) / 2;
    }
    if had_clear {
        points = points.saturating_add(50 * state.combo);
    }
    if perfect_clear {
        points = points.saturating_add(3500);
    }
    state.score = state.score.saturating_add(points);

    if had_clear || spin.is_some() || perfect_clear {
        let mut parts = Vec::new();
        if let Some(label) = base_label(spin, cleared) {
            parts.push(label);
        }
        if perfect_clear {
            parts.push("Perfect Clear".to_string());
        }
        if difficult && state.b2b >= 1 {
            parts.push(format!("B2B x{}", state.b2b));
        }
        if had_clear {
            parts.push(format!("Combo x{}", state.combo));
        }
        if !parts.is_empty() {
            state.last_clear_label = Some(parts.join(" | "));
            state.clear_display_ms = CLEAR_DISPLAY_MS;
        }
    }
}

fn base_points(spin: Option<SpinKind>, cleared: u8) -> u32 {
    match spin {
        None => match cleared {
            1 => 100,
            2 => 300,
            3 => 500,
            4 => 800,
            _ => 0,
        },
        Some(SpinKind::TSpin) => match cleared {
            0 => 400,
            1 => 800,
            2 => 1200,
            3 => 1600,
            4 => 2600,
            _ => 0,
        },
        Some(SpinKind::Mini) | Some(SpinKind::AllMini) => match cleared {
            0 => 100,
            1 => 200,
            2 => 400,
            3 => 800,
            4 => 1600,
            _ => 0,
        },
    }
}

fn base_label(spin: Option<SpinKind>, cleared: u8) -> Option<String> {
    match spin {
        None => {
            if cleared == 0 {
                None
            } else {
                Some(line_label(cleared).to_string())
            }
        }
        Some(SpinKind::TSpin) => Some(spin_label("T-Spin", cleared)),
        Some(SpinKind::Mini) => Some(spin_label("Mini T-Spin", cleared)),
        Some(SpinKind::AllMini) => Some(spin_label("Mini Spin", cleared)),
    }
}

fn spin_label(prefix: &str, cleared: u8) -> String {
    if cleared == 0 {
        prefix.to_string()
    } else {
        format!("{} {}", prefix, line_label(cleared))
    }
}

fn line_label(cleared: u8) -> &'static str {
    match cleared {
        1 => "Single",
        2 => "Double",
        3 => "Triple",
        4 => "Quad",
        _ => "",
    }
}

fn is_difficult_clear(spin: Option<SpinKind>, cleared: u8) -> bool {
    if cleared == 0 {
        return false;
    }
    cleared == 4 || spin.is_some()
}

fn detect_spin(state: &GameState) -> Option<SpinKind> {
    if !state.last_action_rotation {
        return None;
    }
    let piece = state.active;
    if piece.kind == PieceType::T {
        let (corner_count, front_count) = t_spin_corner_info(&state.board, piece);
        if corner_count >= 3 {
            if front_count <= 1 {
                return Some(SpinKind::Mini);
            }
            return Some(SpinKind::TSpin);
        }
        return None;
    }

    if is_all_mini(&state.board, piece) {
        return Some(SpinKind::AllMini);
    }
    None
}

fn t_spin_corner_info(board: &Board, piece: Piece) -> (u8, u8) {
    let cx = piece.x + 1;
    let cy = piece.y + 1;
    let corners = [
        (cx - 1, cy - 1),
        (cx + 1, cy - 1),
        (cx - 1, cy + 1),
        (cx + 1, cy + 1),
    ];
    let mut occupied = 0u8;
    for (x, y) in corners {
        if corner_occupied(board, x, y) {
            occupied += 1;
        }
    }

    let front = t_spin_front_corners(piece.rotation, cx, cy);
    let mut front_count = 0u8;
    for (x, y) in front {
        if corner_occupied(board, x, y) {
            front_count += 1;
        }
    }
    (occupied, front_count)
}

fn t_spin_front_corners(rotation: Rotation, cx: i32, cy: i32) -> [(i32, i32); 2] {
    match rotation.0 % 4 {
        0 => [(cx - 1, cy - 1), (cx + 1, cy - 1)], // up
        1 => [(cx + 1, cy - 1), (cx + 1, cy + 1)], // right
        2 => [(cx - 1, cy + 1), (cx + 1, cy + 1)], // down
        _ => [(cx - 1, cy - 1), (cx - 1, cy + 1)], // left
    }
}

fn corner_occupied(board: &Board, x: i32, y: i32) -> bool {
    if x < 0 || x >= BOARD_WIDTH as i32 || y < 0 || y >= BOARD_HEIGHT as i32 {
        return true;
    }
    board.get(x, y).is_some()
}

fn is_all_mini(board: &Board, piece: Piece) -> bool {
    !can_place(board, offset_piece(piece, -1, 0))
        && !can_place(board, offset_piece(piece, 1, 0))
        && !can_place(board, offset_piece(piece, 0, 1))
}

fn is_board_empty(board: &Board) -> bool {
    for y in 0..BOARD_HEIGHT as i32 {
        for x in 0..BOARD_WIDTH as i32 {
            if board.get(x, y).is_some() {
                return false;
            }
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
        state.board.set(1, 0, Some(PieceType::I));
        let piece = Piece {
            kind: PieceType::O,
            rotation: Rotation(0),
            x: 0,
            y: 0,
        };
        assert!(!can_place(&state.board, piece));
    }

    #[test]
    fn jlstz_wall_kick_from_right_wall() {
        let board = Board::new();
        let piece = Piece {
            kind: PieceType::T,
            rotation: Rotation(3), // L
            x: 8,
            y: 0,
        };

        let rotated = rotate_with_kicks(&board, piece, RotationDir::Cw).expect("kick should fit");
        assert_eq!(rotated.rotation.0, 0);
        assert_eq!(rotated.x, 7);
        assert_eq!(rotated.y, 0);
    }

    #[test]
    fn i_wall_kick_from_right_wall() {
        let board = Board::new();
        let piece = Piece {
            kind: PieceType::I,
            rotation: Rotation(1), // R (vertical)
            x: 7,
            y: 0,
        };

        let rotated = rotate_with_kicks(&board, piece, RotationDir::Cw).expect("kick should fit");
        assert_eq!(rotated.rotation.0, 2);
        assert_eq!(rotated.x, 5);
    }

    #[test]
    fn kick_180_uses_offset() {
        let board = Board::new();
        let piece = Piece {
            kind: PieceType::T,
            rotation: Rotation(3), // L
            x: 8,
            y: 0,
        };

        let rotated = rotate_180_with_kicks(&board, piece).expect("180 kick should fit");
        assert_eq!(rotated.rotation.0, 1);
        assert_eq!(rotated.x, 7);
        assert_eq!(rotated.y, 1);
    }
}
