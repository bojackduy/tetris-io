use crate::core::board::{Board, BOARD_HEIGHT, BOARD_WIDTH};
use crate::core::command::Command;
use crate::core::events::Event;
use crate::core::garbage::RoundingMode;
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
    let cleared_mask = full_rows_mask(&state.board);
    let cleared = state.board.clear_full_rows();
    if cleared > 0 {
        events.push(Event::LinesCleared(cleared as u8));
        let _ = state.apply_row_clear_mask(&cleared_mask);
    }
    let perfect_clear = cleared > 0 && is_board_empty(&state.board);
    let prev_combo = state.combo;
    let prev_b2b = state.b2b;

    let (attack_lines, surge_lines) = compute_attack_lines(
        state,
        spin,
        cleared as u8,
        perfect_clear,
        prev_combo,
        prev_b2b,
    );
    apply_scoring(state, spin, cleared as u8, perfect_clear);

    let mut outgoing_segments = Vec::new();
    if attack_lines > 0 {
        outgoing_segments.push(attack_lines);
    }
    if surge_lines > 0 {
        outgoing_segments.extend(split_surge_lines(surge_lines));
    }

    if !outgoing_segments.is_empty() {
        let total_outgoing: u32 = outgoing_segments.iter().sum();
        let pending = state.pending_garbage();
        let mut remaining = total_outgoing;
        if pending > 0 {
            let mut cancel_lines = total_outgoing;
            if state.opener_active() && total_outgoing < pending {
                cancel_lines = total_outgoing
                    .saturating_mul(state.garbage_config.opener_cancel_multiplier);
            }
            let cancelled = state.cancel_garbage(cancel_lines);
            let cancelled_outgoing = cancelled.min(total_outgoing);
            remaining = total_outgoing.saturating_sub(cancelled_outgoing);
        }
        if remaining > 0 {
            for segment in outgoing_segments {
                if remaining == 0 {
                    break;
                }
                let send = segment.min(remaining);
                if send > 0 {
                    events.push(Event::OutgoingGarbage(send));
                }
                remaining = remaining.saturating_sub(send);
            }
        }
    }

    if state.pending_garbage() > 0 {
        if state.apply_pending_garbage() {
            state.game_over = true;
            events.push(Event::GameOver);
            return;
        }
    }

    state.pieces_placed = state.pieces_placed.saturating_add(1);

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

fn full_rows_mask(board: &Board) -> [bool; BOARD_HEIGHT] {
    let mut mask = [false; BOARD_HEIGHT];
    for (y, row) in board.cells().iter().enumerate() {
        mask[y] = row.iter().all(|cell| cell.is_some());
    }
    mask
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

fn compute_attack_lines(
    state: &mut GameState,
    spin: Option<SpinKind>,
    cleared: u8,
    perfect_clear: bool,
    prev_combo: u32,
    prev_b2b: u32,
) -> (u32, u32) {
    if cleared == 0 {
        return (0, 0);
    }

    let current_combo = prev_combo.saturating_add(1);
    let combo_index = current_combo.saturating_sub(1);
    let base = base_attack(&state.garbage_config.attack, spin, cleared);
    let mut attack = combo_adjusted_attack(
        base,
        combo_index,
        state.garbage_config.combo_multiplier_step,
        state.garbage_config.combo_log_multiplier,
        state.garbage_config.rounding,
        &mut state.garbage_rng,
    );

    let difficult = is_difficult_clear(spin, cleared);
    if difficult && prev_b2b >= 1 {
        attack = attack.saturating_add(state.garbage_config.b2b_bonus);
    }
    if perfect_clear {
        attack = attack.saturating_add(state.garbage_config.perfect_clear_bonus);
    }

    let surge = if !difficult && prev_b2b >= 4 {
        b2b_surge_lines(prev_b2b, state.garbage_config.surge_start)
    } else {
        0
    };

    (attack, surge)
}

fn base_attack(
    table: &crate::core::garbage::AttackTable,
    spin: Option<SpinKind>,
    cleared: u8,
) -> u32 {
    match spin {
        None => match cleared {
            1 => table.single,
            2 => table.double,
            3 => table.triple,
            4 => table.quad,
            _ => 0,
        },
        Some(SpinKind::TSpin) => match cleared {
            0 => table.t_spin_zero,
            1 => table.t_spin_single,
            2 => table.t_spin_double,
            3 => table.t_spin_triple,
            _ => 0,
        },
        Some(SpinKind::Mini) | Some(SpinKind::AllMini) => match cleared {
            0 => table.mini_zero,
            1 => table.mini_single,
            2 => table.mini_double,
            _ => 0,
        },
    }
}

fn combo_adjusted_attack(
    base: u32,
    combo_index: u32,
    combo_step: f32,
    combo_log_multiplier: f32,
    rounding: RoundingMode,
    rng: &mut rand::rngs::StdRng,
) -> u32 {
    if combo_index == 0 {
        return base;
    }

    let value = if base == 0 {
        if combo_index < 2 {
            0.0
        } else {
            (1.0 + combo_log_multiplier * combo_index as f32).ln()
        }
    } else {
        base as f32 * (1.0 + combo_step * combo_index as f32)
    };

    round_attack(value, rounding, rng)
}

fn round_attack(value: f32, rounding: RoundingMode, rng: &mut rand::rngs::StdRng) -> u32 {
    if value <= 0.0 {
        return 0;
    }
    let floor = value.floor();
    let frac = value - floor;
    let mut base = floor as u32;
    if rounding == RoundingMode::Rng && frac > 0.0 {
        let roll: f32 = rand::Rng::r#gen(rng);
        if roll < frac {
            base = base.saturating_add(1);
        }
    }
    base
}

fn b2b_surge_lines(b2b: u32, surge_start: u32) -> u32 {
    if b2b < 4 {
        return 0;
    }
    b2b.saturating_add(surge_start).saturating_sub(4)
}

fn split_surge_lines(total: u32) -> Vec<u32> {
    if total == 0 {
        return Vec::new();
    }
    let base = total / 3;
    let rem = total % 3;
    let first = base + if rem >= 1 { 1 } else { 0 };
    let second = base + if rem == 2 { 1 } else { 0 };
    let third = base;
    let mut segments = Vec::new();
    if first > 0 {
        segments.push(first);
    }
    if second > 0 {
        segments.push(second);
    }
    if third > 0 {
        segments.push(third);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::piece::PieceType;
    use crate::core::state::GameState;
    use crate::core::command::Command;
    use crate::core::board::{BOARD_HEIGHT, BOARD_WIDTH};

    fn run_commands(state: &mut GameState, commands: &[Command]) -> Vec<Event> {
        let cfg = RulesConfig {
            gravity_ms: 999_999,
            soft_drop_factor: 1,
            lock_delay_ms: 999_999,
            lock_reset_limit: 10,
        };
        step(state, cfg, 0, commands, false)
    }

    fn state_with(board: Board, piece: Piece) -> GameState {
        let mut state = GameState::new(1);
        state.board = board;
        state.active = piece;
        state
    }

    fn fill_row_except(board: &mut Board, y: i32, holes: &[i32]) {
        for x in 0..BOARD_WIDTH as i32 {
            if holes.contains(&x) {
                continue;
            }
            board.set(x, y, Some(PieceType::I));
        }
    }

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

    #[test]
    fn perfect_clear_single_scores_and_labels() {
        let mut board = Board::new();
        let bottom = BOARD_HEIGHT as i32 - 1;
        fill_row_except(&mut board, bottom, &[6, 7, 8, 9]);

        let piece = Piece {
            kind: PieceType::I,
            rotation: Rotation(0),
            x: 6,
            y: bottom - 1,
        };
        let mut state = state_with(board, piece);

        run_commands(&mut state, &[Command::HardDrop]);

        let label = state.last_clear_label.as_ref().expect("label should exist");
        assert!(label.contains("Single"));
        assert!(label.contains("Perfect Clear"));
        assert_eq!(state.lines, 1);
        assert_eq!(state.score, 3650);
    }

    #[test]
    fn combo_scoring_accumulates() {
        let mut board = Board::new();
        let bottom = BOARD_HEIGHT as i32 - 1;
        board.set(0, 0, Some(PieceType::I));

        fill_row_except(&mut board, bottom, &[6, 7, 8, 9]);
        let piece = Piece {
            kind: PieceType::I,
            rotation: Rotation(0),
            x: 6,
            y: bottom - 1,
        };
        let mut state = state_with(board, piece);
        run_commands(&mut state, &[Command::HardDrop]);

        fill_row_except(&mut state.board, bottom, &[6, 7, 8, 9]);
        state.active = Piece {
            kind: PieceType::I,
            rotation: Rotation(0),
            x: 6,
            y: bottom - 1,
        };
        run_commands(&mut state, &[Command::HardDrop]);

        let label = state.last_clear_label.as_ref().expect("label should exist");
        assert!(label.contains("Combo x2"));
        assert_eq!(state.combo, 2);
        assert_eq!(state.score, 350);
    }

    #[test]
    fn t_spin_double_label_and_lines() {
        let mut board = Board::new();
        let bottom = BOARD_HEIGHT as i32 - 1;
        fill_row_except(&mut board, bottom, &[4, 5, 6]);
        fill_row_except(&mut board, bottom - 1, &[5]);
        board.set(0, 0, Some(PieceType::I));

        let piece = Piece {
            kind: PieceType::T,
            rotation: Rotation(0),
            x: 4,
            y: bottom - 1,
        };
        let mut state = state_with(board, piece);
        state.last_action_rotation = true;

        run_commands(&mut state, &[Command::HardDrop]);

        let label = state.last_clear_label.as_ref().expect("label should exist");
        assert!(label.contains("T-Spin Double"));
        assert_eq!(state.lines, 2);
    }
}
