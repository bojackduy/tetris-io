use super::board::{BOARD_HEIGHT, BOARD_WIDTH, Board, DEADLINE_Y, VISIBLE_START_Y};
use super::command::Command;
use super::events::Event;
use super::piece::{Piece, PieceType, Rotation};
use super::state::GameState;

#[derive(Debug, Clone, Copy)]
pub struct RulesConfig {
    pub gravity_ms: u64,
    /// Soft-drop rows per second; the legacy field name remains for config compatibility.
    /// Zero selects instant soft drop without locking.
    pub soft_drop_factor: u32,
    pub lock_delay_ms: u64,
    /// Positive values cap resets; zero uses Jstris's 5s contact and 20s piece caps.
    pub lock_reset_limit: u32,
}

// Shipped Jstris setLockDelay([500, 5000, 20000]) separates these wall-clock
// deadlines from the ordinary movement-reset lock delay.
const JSTRIS_CONTACT_CAP_MS: u64 = 5000;
const JSTRIS_PIECE_CAP_MS: u64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RotationDir {
    Cw,
    Ccw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpinKind {
    TSpin,
    Mini,
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
    update_grounded(state);

    for cmd in commands {
        let was_grounded = state.grounded;
        let reset_lock = match cmd {
            Command::MoveLeft => {
                let moved = try_shift(state, -1, 0);
                if moved {
                    state.last_action_rotation = false;
                    events.push(Event::Moved);
                }
                moved
            }
            Command::MoveRight => {
                let moved = try_shift(state, 1, 0);
                if moved {
                    state.last_action_rotation = false;
                    events.push(Event::Moved);
                }
                moved
            }
            Command::SoftDrop => {
                soft_drop_pressed = true;
                if cfg.soft_drop_factor == 0 {
                    soft_drop_to_ghost(state);
                } else if try_shift(state, 0, 1) {
                    state.last_action_rotation = false;
                    state.score = state.score.saturating_add(1);
                }
                false
            }
            Command::HardDrop => {
                events.push(Event::HardDropped);
                hard_drop(state, &mut events);
                return events;
            }
            Command::RotateLeft => {
                let rotated = try_rotate(state, RotationDir::Ccw);
                if rotated {
                    state.last_action_rotation = true;
                    events.push(Event::Rotated);
                }
                rotated
            }
            Command::RotateRight => {
                let rotated = try_rotate(state, RotationDir::Cw);
                if rotated {
                    state.last_action_rotation = true;
                    events.push(Event::Rotated);
                }
                rotated
            }
            Command::Rotate180 => {
                let rotated = try_rotate_180(state);
                if rotated {
                    state.last_action_rotation = true;
                    events.push(Event::Rotated);
                }
                rotated
            }
            Command::Hold => {
                hold_piece(state, &mut events);
                false
            }
            Command::Quit => {
                state.game_over = true;
                events.push(Event::GameOver);
                false
            }
        };
        if state.game_over {
            return events;
        }
        if reset_lock
            && was_grounded
            && (cfg.lock_reset_limit == 0 || state.lock_reset_count < cfg.lock_reset_limit)
        {
            state.lock_delay_ms = 0;
            state.lock_reset_count = state.lock_reset_count.saturating_add(1);
        }
        update_grounded(state);
    }

    let soft_drop = soft_drop_active || soft_drop_pressed;
    let instant_soft_drop = soft_drop && cfg.soft_drop_factor == 0;
    let gravity_ms = cfg.gravity_ms.max(1);
    let soft_rate = cfg.soft_drop_factor as u64;
    let use_soft_rate =
        soft_drop && !instant_soft_drop && soft_rate as u128 * gravity_ms as u128 > 1000;
    if !use_soft_rate {
        state.soft_drop_accum_millirows = 0;
    }
    let mut remaining_ms = dt_ms;

    // Commands occur at the frame's start. Advance gravity and lock deadlines
    // chronologically so time before first contact never counts toward locking.
    loop {
        if instant_soft_drop {
            soft_drop_to_ghost(state);
        }
        update_grounded(state);
        let time_until_cap = if cfg.lock_reset_limit == 0 {
            let piece_cap = JSTRIS_PIECE_CAP_MS.saturating_sub(state.elapsed_since_spawn_ms);
            if state.lock_started {
                piece_cap
                    .min(JSTRIS_CONTACT_CAP_MS.saturating_sub(state.elapsed_since_first_contact_ms))
            } else {
                piece_cap
            }
        } else {
            u64::MAX
        };
        let time_until_lock = if state.grounded {
            cfg.lock_delay_ms.saturating_sub(state.lock_delay_ms)
        } else {
            u64::MAX
        };
        let time_until_fall = if state.grounded {
            u64::MAX
        } else if use_soft_rate {
            1000u64
                .saturating_sub(state.soft_drop_accum_millirows)
                .div_ceil(soft_rate)
        } else {
            gravity_ms.saturating_sub(state.gravity_accum_ms)
        };
        let elapsed = remaining_ms
            .min(time_until_cap)
            .min(time_until_lock)
            .min(time_until_fall);
        remaining_ms -= elapsed;
        state.elapsed_since_spawn_ms = state.elapsed_since_spawn_ms.saturating_add(elapsed);
        if state.lock_started {
            state.elapsed_since_first_contact_ms =
                state.elapsed_since_first_contact_ms.saturating_add(elapsed);
        }
        if state.grounded {
            state.lock_delay_ms = state.lock_delay_ms.saturating_add(elapsed);
        }
        if use_soft_rate {
            state.soft_drop_accum_millirows = state
                .soft_drop_accum_millirows
                .saturating_add(elapsed.saturating_mul(soft_rate));
            state.gravity_accum_ms =
                ((state.gravity_accum_ms as u128 + elapsed as u128) % gravity_ms as u128) as u64;
            if state.grounded {
                state.soft_drop_accum_millirows %= 1000;
            }
        } else {
            state.gravity_accum_ms = state.gravity_accum_ms.saturating_add(elapsed);
            if state.grounded {
                // Do not bank blocked falls for later kicks.
                state.gravity_accum_ms %= gravity_ms;
            }
        }

        let cap_due = cfg.lock_reset_limit == 0 && time_until_cap == elapsed;
        let lock_due = state.grounded && time_until_lock == elapsed;
        if cap_due || lock_due {
            if cap_due {
                // Jstris force-lock deadlines also drop airborne pieces to the floor.
                hard_drop(state, &mut events);
            } else {
                lock_piece(state, &mut events);
            }
            if state.game_over || remaining_ms == 0 {
                break;
            }
        } else if !state.grounded && time_until_fall == elapsed {
            if use_soft_rate {
                state.soft_drop_accum_millirows -= 1000;
            } else {
                state.gravity_accum_ms -= gravity_ms;
            }
            if try_shift(state, 0, 1) {
                state.last_action_rotation = false;
            }
        } else {
            break;
        }
    }

    events
}

fn update_grounded(state: &mut GameState) {
    state.grounded = !can_place(&state.board, offset_piece(state.active, 0, 1));
    if state.grounded && !state.lock_started {
        state.lock_started = true;
        state.lock_delay_ms = 0;
    }
}

fn soft_drop_to_ghost(state: &mut GameState) {
    let ghost = ghost_piece(state);
    let distance = (ghost.y - state.active.y) as u32;
    if distance > 0 {
        state.active = ghost;
        state.last_action_rotation = false;
        state.score = state.score.saturating_add(distance);
    }
}

pub fn ghost_piece(state: &GameState) -> Piece {
    let mut ghost = state.active;
    while can_place(&state.board, offset_piece(ghost, 0, 1)) {
        ghost.y += 1;
    }
    ghost
}

/// Exact Jstris `setCurrentPieceToDefaultPos()` spawn and block-out check:
/// - Places piece at default spawn (`SPAWN_Y = DEADLINE_Y`).
/// - If any cell overlaps the top visible row, shifts up 1 row into the deadline buffer.
/// - Tops out (returns `false`) if still overlapping visible cells OR if any cell at
///   `DEADLINE_Y` overlaps an occupied cell in the deadline row.
fn spawn_active_piece(state: &mut GameState, kind: PieceType) -> bool {
    state.reset_active(kind);
    if !can_place(&state.board, state.active) {
        state.active.y -= 1;
    }
    let overlaps_deadline = state
        .active
        .blocks()
        .iter()
        .any(|&(x, y)| y == DEADLINE_Y && state.board.get(x, DEADLINE_Y).is_some());
    can_place(&state.board, state.active) && !overlaps_deadline
}

fn hold_piece(state: &mut GameState, events: &mut Vec<Event>) {
    if state.hold_used {
        return;
    }
    events.push(Event::Held);
    let current = state.active.kind;
    let next_kind = if let Some(held) = state.hold {
        state.hold = Some(current);
        held
    } else {
        state.hold = Some(current);
        state.pop_next()
    };
    state.hold_used = true;
    if !spawn_active_piece(state, next_kind) {
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
    // Verified in Jstris placeBlock():
    // - Cells in visible rows (y >= VISIBLE_START_Y) write into matrix.
    // - Cells in DEADLINE_Y write into deadline only if empty.
    // - Cells above DEADLINE_Y are discarded.
    // - Locking completely above the visible field (all 4 cells) is immediate lock-out.
    let mut above_visible = 0u8;
    for (x, y) in state.active.blocks() {
        if y >= VISIBLE_START_Y as i32 {
            state.board.set(x, y, Some(state.active.kind));
        } else {
            above_visible += 1;
            if y == DEADLINE_Y && state.board.get(x, DEADLINE_Y).is_none() {
                state.board.set(x, DEADLINE_Y, Some(state.active.kind));
            }
        }
    }
    events.push(Event::Locked);
    if above_visible == 4 {
        state.game_over = true;
        events.push(Event::GameOver);
        return;
    }
    let cleared_mask = full_rows_mask(&state.board);
    let cleared = state.board.clear_full_rows();
    if cleared > 0 {
        events.push(Event::LinesCleared(cleared as u8));
        let _ = state.apply_row_clear_mask(&cleared_mask);
    }
    // Jstris scores and attacks with mini doubles as full T-spin doubles.
    let spin = match spin {
        Some(SpinKind::Mini) if cleared >= 2 => Some(SpinKind::TSpin),
        other => other,
    };
    let perfect_clear = cleared > 0 && is_board_empty(&state.board);
    let prev_combo = state.combo;
    let prev_b2b = state.b2b;

    let attack_lines = compute_attack_lines(
        state,
        spin,
        cleared as u8,
        perfect_clear,
        prev_combo,
        prev_b2b,
    );
    state.attack = state.attack.saturating_add(attack_lines);
    state.last_clear_was_pc = perfect_clear;
    if state.piece_manual_inputs > 0 {
        let optimal = optimal_finesse(state.active);
        state.finesse_faults = state
            .finesse_faults
            .saturating_add(state.piece_manual_inputs.saturating_sub(optimal));
    }
    apply_scoring(state, spin, cleared as u8, perfect_clear);

    let cancelled = state.cancel_garbage(attack_lines);
    let remaining = attack_lines.saturating_sub(cancelled);
    if remaining > 0 {
        events.push(Event::OutgoingGarbage(remaining));
    }

    if state.pending_garbage() > 0 && state.apply_pending_garbage() {
        state.game_over = true;
        events.push(Event::GameOver);
        return;
    }

    state.pieces_placed = state.pieces_placed.saturating_add(1);

    let next = state.pop_next();
    state.hold_used = false;
    if !spawn_active_piece(state, next) {
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
        let kicked_up = piece.y < state.active.y;
        state.active = piece;
        if kicked_up {
            // Verified in Jstris rotateCurrentBlock: upward kick resets gravity timer.
            state.gravity_accum_ms = 0;
        }
        true
    } else {
        false
    }
}

fn try_rotate_180(state: &mut GameState) -> bool {
    if let Some(piece) = rotate_180_with_kicks(&state.board, state.active) {
        let kicked_up = piece.y < state.active.y;
        state.active = piece;
        if kicked_up {
            state.gravity_accum_ms = 0;
        }
        true
    } else {
        false
    }
}

fn rotate_180_with_kicks(board: &Board, piece: Piece) -> Option<Piece> {
    let target = piece.rotation.half();
    if piece.kind == PieceType::O {
        let candidate = Piece {
            rotation: target,
            ..piece
        };
        return if can_place(board, candidate) {
            Some(candidate)
        } else {
            None
        };
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

fn kicks_180(from: Rotation) -> &'static [(i32, i32); 2] {
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
        let candidate = Piece {
            rotation: target,
            ..piece
        };
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

// Standard SRS, converted from positive-up to positive-down y coordinates.
// https://tetris.wiki/SRS#Wall_Kicks
const I_KICKS_CW: [[(i32, i32); 5]; 4] = [
    // 0 -> R
    [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
    // R -> 2
    [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
    // 2 -> L
    [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
    // L -> 0
    [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
];

// Exact Jstris 180-degree kick table from shipped replayer.js getBlockSets(),
// converted from positive-up to positive-down y coordinates.
const KICKS_180_NS: [(i32, i32); 2] = [(0, 0), (0, -1)];
const KICKS_180_EW: [(i32, i32); 2] = [(0, 0), (1, 0)];
const KICKS_180_SN: [(i32, i32); 2] = [(0, 0), (0, 1)];
const KICKS_180_WE: [(i32, i32); 2] = [(0, 0), (-1, 0)];

/// Exact optimal keypress count for placing a piece at its final column and rotation,
/// extracted from shipped Jstris replayer.js (`finesse` table and `cc` offsets).
pub fn optimal_finesse(piece: Piece) -> u32 {
    let min_x = piece
        .blocks()
        .iter()
        .map(|&(x, _)| x)
        .min()
        .unwrap_or(0)
        .max(0) as usize;
    let rot = (piece.rotation.0 % 4) as usize;
    let row: &[u8] = match piece.kind {
        PieceType::I => match rot {
            0 | 2 => &[1, 2, 1, 0, 1, 2, 1],
            _ => &[2, 2, 2, 2, 1, 1, 2, 2, 2, 2],
        },
        PieceType::O => &[1, 2, 2, 1, 0, 1, 2, 2, 1],
        PieceType::T | PieceType::L | PieceType::J => match rot {
            0 => &[1, 2, 1, 0, 1, 2, 2, 1],
            1 => &[2, 2, 3, 2, 1, 2, 3, 3, 2],
            2 => &[3, 4, 3, 2, 3, 4, 4, 3],
            _ => &[2, 3, 2, 1, 2, 3, 3, 2, 2],
        },
        PieceType::S | PieceType::Z => match rot {
            0 | 2 => &[1, 2, 1, 0, 1, 2, 2, 1],
            _ => &[2, 2, 2, 1, 1, 2, 3, 2, 2],
        },
    };
    u32::from(row.get(min_x).copied().unwrap_or(0))
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
        if x < 0 || x >= BOARD_WIDTH as i32 || y >= BOARD_HEIGHT as i32 {
            return false;
        }
        // Verified in Jstris checkIntersection(): only visible matrix rows (y >= VISIBLE_START_Y)
        // are solid for movement/rotation; DEADLINE_Y (y == 0) is nonsolid and y < 0 is open air.
        if y >= VISIBLE_START_Y as i32 && board.get(x, y).is_some() {
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
        points = points.saturating_add(50u32.saturating_mul(state.combo.saturating_sub(1)));
    }
    if perfect_clear {
        points = points.saturating_add(3000);
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
        if had_clear && state.combo >= 2 {
            parts.push(format!("Combo x{}", state.combo - 1));
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
        Some(SpinKind::Mini) => match cleared {
            0 => 100,
            1 => 200,
            2 => 1200,
            3 => 1600,
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
    if x < 0 || x >= BOARD_WIDTH as i32 || y >= BOARD_HEIGHT as i32 {
        return true;
    }
    if y < DEADLINE_Y {
        return false;
    }
    board.get(x, y).is_some()
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
    state: &GameState,
    spin: Option<SpinKind>,
    cleared: u8,
    perfect_clear: bool,
    prev_combo: u32,
    prev_b2b: u32,
) -> u32 {
    if cleared == 0 {
        return 0;
    }

    // Verified from shipped Jstris replayer.js checkLineClears():
    // Perfect clear replaces base + B2B attack with 10, and combo attack is still added.
    const COMBO_ATTACK: [u32; 13] = [0, 0, 1, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5];
    let combo_bonus = COMBO_ATTACK[prev_combo.min(12) as usize];
    if perfect_clear {
        return state
            .garbage_config
            .perfect_clear_bonus
            .saturating_add(combo_bonus);
    }
    let mut attack =
        base_attack(&state.garbage_config.attack, spin, cleared).saturating_add(combo_bonus);

    let difficult = is_difficult_clear(spin, cleared);
    if difficult && prev_b2b >= 1 && !(spin == Some(SpinKind::Mini) && cleared == 1) {
        attack = attack.saturating_add(state.garbage_config.b2b_bonus);
    }

    attack
}

fn base_attack(table: &super::garbage::AttackTable, spin: Option<SpinKind>, cleared: u8) -> u32 {
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
        Some(SpinKind::Mini) => match cleared {
            0 => table.mini_zero,
            1 => table.mini_single,
            2 => table.t_spin_double,
            _ => 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::board::{BOARD_HEIGHT, BOARD_WIDTH};
    use super::super::command::Command;
    use super::super::piece::PieceType;
    use super::super::state::GameState;
    use super::*;

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
        state
            .board
            .set(1, VISIBLE_START_Y as i32, Some(PieceType::I));
        let piece = Piece {
            kind: PieceType::O,
            rotation: Rotation(0),
            x: 0,
            y: VISIBLE_START_Y as i32,
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
        assert_eq!(rotated.x, 6);
    }

    #[test]
    fn kick_180_uses_offset() {
        let board = Board::new();
        let piece = Piece {
            kind: PieceType::T,
            rotation: Rotation(3), // L at right wall; Jstris 3->1 kicks [(0,0), (-1,0)]
            x: 8,
            y: 0,
        };

        let rotated = rotate_180_with_kicks(&board, piece).expect("180 kick should fit");
        assert_eq!(rotated.rotation.0, 1);
        assert_eq!(rotated.x, 7);
        assert_eq!(rotated.y, 0);
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

        let events = run_commands(&mut state, &[Command::HardDrop]);

        let label = state.last_clear_label.as_ref().expect("label should exist");
        assert!(label.contains("Single"));
        assert!(label.contains("Perfect Clear"));
        assert_eq!(state.lines, 1);
        assert_eq!(state.score, 3100);
        assert_eq!(
            events,
            vec![
                Event::HardDropped,
                Event::Locked,
                Event::LinesCleared(1),
                Event::OutgoingGarbage(10),
            ],
        );
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
        assert!(label.contains("Combo x1"));
        assert_eq!(state.combo, 2);
        assert_eq!(state.score, 250);
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

#[cfg(test)]
mod modern_rules {
    use super::super::board::VISIBLE_START_Y;
    use super::*;

    fn config() -> RulesConfig {
        RulesConfig {
            gravity_ms: 100,
            soft_drop_factor: 20,
            lock_delay_ms: 500,
            lock_reset_limit: 15,
        }
    }

    fn state_with(piece: Piece) -> GameState {
        let mut state = GameState::new(1);
        state.active = piece;
        state
    }

    fn grounded_o() -> GameState {
        let mut state = state_with(Piece::new(PieceType::O, 3, BOARD_HEIGHT as i32 - 2));
        step(&mut state, config(), 0, &[], false);
        state
    }

    fn is_action_event(event: &Event) -> bool {
        matches!(event, Event::Moved | Event::Rotated | Event::Held)
    }

    fn assert_same_state(left: &GameState, right: &GameState) {
        assert_eq!(left.active, right.active);
        assert_eq!(left.board.cells(), right.board.cells());
        assert_eq!(left.gravity_accum_ms, right.gravity_accum_ms);
        assert_eq!(
            left.soft_drop_accum_millirows,
            right.soft_drop_accum_millirows,
        );
        assert_eq!(left.lock_delay_ms, right.lock_delay_ms);
        assert_eq!(left.lock_reset_count, right.lock_reset_count);
        assert_eq!(left.grounded, right.grounded);
        assert_eq!(left.lock_started, right.lock_started);
        assert_eq!(left.elapsed_since_spawn_ms, right.elapsed_since_spawn_ms);
        assert_eq!(
            left.elapsed_since_first_contact_ms,
            right.elapsed_since_first_contact_ms,
        );
        assert_eq!(left.last_action_rotation, right.last_action_rotation);
        assert_eq!(left.pieces_placed, right.pieces_placed);
        assert_eq!(left.game_over, right.game_over);
        assert_eq!(left.score, right.score);
        assert_eq!(left.lines, right.lines);
        assert_eq!(left.combo, right.combo);
        assert_eq!(left.b2b, right.b2b);
        assert_eq!(left.hold, right.hold);
        assert_eq!(left.hold_used, right.hold_used);
        assert_eq!(left.queue, right.queue);
    }

    #[test]
    fn batched_grounded_actions_each_use_one_lock_reset() {
        let mut state = grounded_o();
        state.lock_delay_ms = 400;

        let events = step(
            &mut state,
            config(),
            10,
            &[Command::MoveLeft, Command::RotateRight, Command::MoveRight],
            false,
        );

        assert_eq!(events, vec![Event::Moved, Event::Rotated, Event::Moved]);
        assert_eq!(state.lock_reset_count, 3);
        assert_eq!(state.lock_delay_ms, 10);
    }

    #[test]
    fn lock_timer_counts_only_time_after_gravity_contact() {
        let mut state = state_with(Piece::new(PieceType::O, 3, BOARD_HEIGHT as i32 - 3));
        state.gravity_accum_ms = 20;

        assert!(step(&mut state, config(), 100, &[], false).is_empty());

        assert!(state.grounded);
        assert!(state.lock_started);
        assert_eq!(state.lock_delay_ms, 20);
        assert_eq!(state.lock_reset_count, 0);
    }

    #[test]
    fn batched_actions_match_separate_actions_at_same_timestamp() {
        let mut batched = grounded_o();
        let mut separate = grounded_o();
        batched.lock_delay_ms = 400;
        separate.lock_delay_ms = 400;
        let commands = [
            Command::MoveLeft,
            Command::RotateRight,
            Command::MoveRight,
            Command::RotateLeft,
            Command::Rotate180,
        ];

        let events = step(&mut batched, config(), 120, &commands, false);
        let mut separate_events = Vec::new();
        for command in commands {
            separate_events.extend(step(&mut separate, config(), 0, &[command], false));
        }
        separate_events.extend(step(&mut separate, config(), 120, &[], false));

        assert_eq!(events, separate_events);
        assert_eq!(batched.lock_reset_count, commands.len() as u32);
        assert_same_state(&batched, &separate);
    }

    #[test]
    fn reset_limit_applies_within_batch_and_cannot_be_extended() {
        let mut state = grounded_o();
        state.lock_delay_ms = 400;
        let cfg = RulesConfig {
            lock_reset_limit: 2,
            ..config()
        };

        assert!(
            step(
                &mut state,
                cfg,
                0,
                &[Command::MoveLeft, Command::MoveRight, Command::RotateRight],
                false,
            )
            .iter()
            .all(is_action_event)
        );
        assert_eq!(state.lock_reset_count, 2);
        assert_eq!(state.lock_delay_ms, 0);
        assert_eq!(
            step(&mut state, cfg, 499, &[Command::MoveLeft], false),
            vec![Event::Moved],
        );
        assert_eq!(state.lock_reset_count, 2);
        assert_eq!(state.lock_delay_ms, 499);
        assert_eq!(
            step(&mut state, cfg, 1, &[Command::MoveRight], false),
            vec![Event::Moved, Event::Locked]
        );
        assert_eq!(state.pieces_placed, 1);
    }

    #[test]
    fn jstris_zero_reset_limit_allows_resets_until_five_second_contact_cap() {
        let mut state = grounded_o();
        let cfg = RulesConfig {
            lock_reset_limit: 0,
            ..config()
        };

        for index in 0..49 {
            let command = if index % 2 == 0 {
                Command::MoveLeft
            } else {
                Command::MoveRight
            };
            assert_eq!(
                step(&mut state, cfg, 100, &[command], false),
                vec![Event::Moved],
            );
        }
        assert_eq!(state.lock_reset_count, 49);
        assert_eq!(state.lock_delay_ms, 100);
        assert_eq!(state.elapsed_since_first_contact_ms, 4900);
        assert_eq!(
            step(&mut state, cfg, 100, &[Command::MoveRight], false),
            vec![Event::Moved, Event::Locked]
        );
        assert_eq!(state.pieces_placed, 1);
    }

    #[test]
    fn jstris_contact_time_cap_hard_drops_even_after_leaving_ground() {
        let mut state = ledge_state();
        state.elapsed_since_first_contact_ms = 4900;
        state.elapsed_since_spawn_ms = 4900;
        let cfg = RulesConfig {
            gravity_ms: 99_999,
            lock_reset_limit: 0,
            ..config()
        };
        let distance = BOARD_HEIGHT as i32 - 2 - state.active.y;

        assert_eq!(
            step(&mut state, cfg, 100, &[Command::MoveRight], false),
            vec![Event::Moved, Event::Locked]
        );

        assert_eq!(state.pieces_placed, 1);
        assert_eq!(state.score, distance as u32 * 2);
        assert!(state.board.get(5, BOARD_HEIGHT as i32 - 1).is_some());
        assert_eq!(state.elapsed_since_spawn_ms, 0);
        assert_eq!(state.elapsed_since_first_contact_ms, 0);
    }

    #[test]
    fn jstris_twenty_second_spawn_cap_hard_drops_airborne_piece() {
        let mut state = state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32));
        let expected = ghost_piece(&state);
        let cfg = RulesConfig {
            gravity_ms: 99_999,
            lock_reset_limit: 0,
            ..config()
        };
        state.elapsed_since_spawn_ms = 19_900;

        assert!(step(&mut state, cfg, 99, &[], false).is_empty());
        assert!(!state.lock_started);
        assert_eq!(state.elapsed_since_spawn_ms, 19_999);
        assert_eq!(step(&mut state, cfg, 1, &[], false), vec![Event::Locked]);
        for (x, y) in expected.blocks() {
            assert_eq!(state.board.get(x, y), Some(PieceType::T));
        }
        assert_eq!(state.pieces_placed, 1);
    }

    #[test]
    fn jstris_forced_drop_cap_is_frame_partition_invariant() {
        let cfg = RulesConfig {
            gravity_ms: 99_999,
            lock_delay_ms: 10_000,
            lock_reset_limit: 0,
            ..config()
        };
        for start_grounded in [true, false] {
            let mut whole = if start_grounded {
                grounded_o()
            } else {
                state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32))
            };
            let mut separate = if start_grounded {
                grounded_o()
            } else {
                state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32))
            };
            let total = if start_grounded { 5500 } else { 20_500 };
            let events = step(&mut whole, cfg, total, &[], false);
            let mut separate_events = Vec::new();
            for _ in 0..total / 10 {
                separate_events.extend(step(&mut separate, cfg, 10, &[], false));
            }

            assert_eq!(events, separate_events);
            assert_eq!(whole.pieces_placed, 1);
            assert_same_state(&whole, &separate);
        }
    }

    #[test]
    fn failed_wall_moves_do_not_reset_timer_or_rotation_marker() {
        let mut state = grounded_o();
        state.active.x = -1;
        state.lock_delay_ms = 350;
        state.last_action_rotation = true;
        let original = state.active;

        assert!(
            step(
                &mut state,
                config(),
                100,
                &[Command::MoveLeft, Command::MoveLeft, Command::SoftDrop],
                false,
            )
            .is_empty()
        );

        assert_eq!(state.active, original);
        assert_eq!(state.lock_delay_ms, 450);
        assert_eq!(state.lock_reset_count, 0);
        assert!(state.last_action_rotation);
    }

    #[test]
    fn failed_rotations_do_not_spend_resets() {
        let mut state = state_with(Piece::new(PieceType::T, 3, BOARD_HEIGHT as i32 - 2));
        let occupied = state.active.blocks();
        for y in 0..BOARD_HEIGHT as i32 {
            for x in 0..BOARD_WIDTH as i32 {
                if !occupied.contains(&(x, y)) {
                    state.board.set(x, y, Some(PieceType::J));
                }
            }
        }
        assert!(can_place(&state.board, state.active));
        step(&mut state, config(), 0, &[], false);
        state.lock_delay_ms = 400;
        let original = state.active;

        assert!(
            step(
                &mut state,
                config(),
                50,
                &[
                    Command::RotateLeft,
                    Command::RotateRight,
                    Command::Rotate180
                ],
                false,
            )
            .is_empty()
        );

        assert_eq!(state.active, original);
        assert_eq!(state.lock_reset_count, 0);
        assert_eq!(state.lock_delay_ms, 450);
        assert!(!state.last_action_rotation);
    }

    #[test]
    fn airborne_actions_do_not_start_lock_or_spend_resets() {
        let mut state = state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32));

        assert!(
            step(
                &mut state,
                config(),
                0,
                &[
                    Command::MoveLeft,
                    Command::RotateRight,
                    Command::MoveRight,
                    Command::RotateLeft,
                    Command::Rotate180,
                ],
                false,
            )
            .iter()
            .all(is_action_event)
        );

        assert_eq!(state.lock_reset_count, 0);
        assert_eq!(state.lock_delay_ms, 0);
        assert!(!state.lock_started);
        assert!(!state.grounded);
    }

    fn ledge_state() -> GameState {
        let mut state = state_with(Piece::new(PieceType::O, 3, BOARD_HEIGHT as i32 - 5));
        state
            .board
            .set(4, BOARD_HEIGHT as i32 - 3, Some(PieceType::J));
        step(&mut state, config(), 0, &[], false);
        assert!(state.grounded);
        state.lock_delay_ms = 400;
        state
    }

    #[test]
    fn leaving_ground_spends_one_reset_and_airborne_rotation_spends_none() {
        let mut state = ledge_state();

        assert!(
            step(
                &mut state,
                config(),
                100,
                &[Command::MoveRight, Command::RotateRight],
                false,
            )
            .iter()
            .all(is_action_event)
        );
        assert!(!state.grounded);
        assert_eq!(state.lock_reset_count, 1);
        assert_eq!(state.lock_delay_ms, 0);

        assert!(step(&mut state, config(), 225, &[], false).is_empty());
        assert!(state.grounded);
        assert_eq!(state.lock_reset_count, 1);
        assert_eq!(state.lock_delay_ms, 25);
    }

    #[test]
    fn exhausted_reset_budget_survives_airborne_time_and_recontact() {
        let mut state = ledge_state();
        state.lock_reset_count = config().lock_reset_limit;

        assert_eq!(
            step(&mut state, config(), 100, &[Command::MoveRight], false),
            vec![Event::Moved],
        );
        assert!(!state.grounded);
        assert_eq!(state.lock_delay_ms, 400);
        assert!(step(&mut state, config(), 200, &[], false).is_empty());
        assert!(state.grounded);
        assert_eq!(state.lock_delay_ms, 400);
        assert_eq!(state.lock_reset_count, 15);
        assert_eq!(
            step(&mut state, config(), 100, &[], false),
            vec![Event::Locked]
        );
    }

    #[test]
    fn contact_created_mid_batch_is_used_by_next_action() {
        let mut state = state_with(Piece::new(PieceType::O, 3, BOARD_HEIGHT as i32 - 3));

        assert!(
            step(
                &mut state,
                config(),
                0,
                &[Command::SoftDrop, Command::MoveLeft, Command::MoveRight],
                false,
            )
            .iter()
            .all(is_action_event)
        );

        assert!(state.grounded);
        assert_eq!(state.lock_reset_count, 2);
        assert_eq!(state.lock_delay_ms, 0);
    }

    #[test]
    fn gravity_and_lock_deadlines_do_not_depend_on_frame_partition() {
        for total in [99, 100, 101, 499, 500, 579, 580, 581, 1000, 3500] {
            for split in [1, 7, 25, 99, 100, 333] {
                let piece = Piece::new(PieceType::O, 3, BOARD_HEIGHT as i32 - 3);
                let mut whole = state_with(piece);
                let mut partitioned = state_with(piece);
                whole.gravity_accum_ms = 20;
                partitioned.gravity_accum_ms = 20;
                let whole_events = step(&mut whole, config(), total, &[], false);
                let mut partitioned_events = Vec::new();
                let mut remaining = total;
                while remaining > 0 {
                    let elapsed = remaining.min(split);
                    partitioned_events.extend(step(
                        &mut partitioned,
                        config(),
                        elapsed,
                        &[],
                        false,
                    ));
                    remaining -= elapsed;
                }

                assert_eq!(whole_events, partitioned_events, "{total} ms / {split} ms");
                assert_same_state(&whole, &partitioned);
            }
        }
    }

    #[test]
    fn hold_does_not_transfer_old_piece_resets_or_contact() {
        let mut state = grounded_o();
        state.hold = Some(PieceType::I);
        state.lock_delay_ms = 400;
        state.lock_reset_count = 8;

        assert!(
            step(
                &mut state,
                config(),
                10,
                &[Command::MoveLeft, Command::Hold, Command::MoveRight],
                false,
            )
            .iter()
            .all(is_action_event)
        );

        assert_eq!(state.active, Piece::new(PieceType::I, 4, DEADLINE_Y));
        assert_eq!(state.hold, Some(PieceType::O));
        assert!(state.hold_used);
        assert_eq!(state.lock_reset_count, 0);
        assert_eq!(state.lock_delay_ms, 0);
        assert_eq!(state.gravity_accum_ms, 10);
        assert_eq!(state.elapsed_since_spawn_ms, 10);
        assert_eq!(state.elapsed_since_first_contact_ms, 0);
        assert!(!state.lock_started);
        assert!(!state.grounded);
    }

    #[test]
    fn grounded_hold_spawn_starts_its_own_lock_timer() {
        let mut state = grounded_o();
        state.hold = Some(PieceType::O);
        state.lock_delay_ms = 400;
        state.lock_reset_count = 8;
        state
            .board
            .set(4, VISIBLE_START_Y as i32 + 1, Some(PieceType::J));

        assert!(
            step(
                &mut state,
                config(),
                100,
                &[Command::MoveLeft, Command::Hold],
                false,
            )
            .iter()
            .all(is_action_event)
        );
        assert!(state.grounded);
        assert!(state.lock_started);
        assert_eq!(state.lock_reset_count, 0);
        assert_eq!(state.lock_delay_ms, 100);
        let original = state.active;
        assert!(step(&mut state, config(), 100, &[Command::Hold], false).is_empty());
        assert_eq!(state.active, original);
        assert_eq!(state.lock_reset_count, 0);
        assert_eq!(state.lock_delay_ms, 200);
    }

    #[test]
    fn hold_and_next_spawn_top_out_when_deadline_obstructed_and_shift_up_when_only_top_row_blocked()
    {
        // Case 1: Only top visible row blocked, deadline empty -> shifts up 1 row and survives!
        for hold in [true, false] {
            let mut state = grounded_o();
            state.active.x = 0;
            state.hold = Some(PieceType::T);
            state.queue.push_front(PieceType::T);
            state
                .board
                .set(4, VISIBLE_START_Y as i32, Some(PieceType::J));
            let command = if hold {
                Command::Hold
            } else {
                Command::HardDrop
            };

            let events = step(&mut state, config(), 0, &[command], false);

            assert!(!state.game_over);
            assert_eq!(state.active.kind, PieceType::T);
            assert_eq!(state.active.y, DEADLINE_Y - 1);
            assert_eq!(events.contains(&Event::Locked), !hold);
        }

        // Case 2: Deadline cell blocked at spawn -> immediate block-out GameOver!
        for hold in [true, false] {
            let mut state = grounded_o();
            state.active.x = 0;
            state.hold = Some(PieceType::T);
            state.queue.push_front(PieceType::T);
            state.board.set(4, DEADLINE_Y, Some(PieceType::J));
            let command = if hold {
                Command::Hold
            } else {
                Command::HardDrop
            };

            let events = step(&mut state, config(), 0, &[command], false);

            assert!(state.game_over);
            assert_eq!(state.active.kind, PieceType::T);
            assert_eq!(events.last(), Some(&Event::GameOver));
            assert_eq!(events.contains(&Event::Locked), !hold);
        }
    }

    #[test]
    fn ten_and_eighteen_line_cheese_leave_valid_visible_spawn() {
        for lines in [10, 18] {
            let mut state = GameState::new(1);
            state.seed_cheese(lines, 7);
            assert_eq!(state.active.y, DEADLINE_Y);
            assert!(can_place(&state.board, state.active));
            assert_eq!(state.garbage_remaining, lines);
            assert_eq!(
                state.garbage_rows.iter().filter(|&&row| row).count(),
                lines as usize,
            );
        }
    }

    #[test]
    fn zero_factor_soft_drop_reaches_ghost_without_locking() {
        let mut state = state_with(Piece::new(PieceType::T, 3, 0));
        let expected = ghost_piece(&state);
        let cfg = RulesConfig {
            soft_drop_factor: 0,
            ..config()
        };

        let events = step(&mut state, cfg, 0, &[Command::SoftDrop], false);

        assert!(events.is_empty());
        assert_eq!(state.active, expected);
        assert!(state.grounded);
        assert_eq!(state.pieces_placed, 0);
        assert_eq!(state.lock_delay_ms, 0);
        assert_eq!(state.lock_reset_count, 0);
    }

    #[test]
    fn held_instant_soft_drop_keeps_lock_delay_and_repeated_presses_do_not_reset_it() {
        let mut state = state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32));
        let expected = ghost_piece(&state);
        let distance = (expected.y - state.active.y) as u32;
        let cfg = RulesConfig {
            soft_drop_factor: 0,
            ..config()
        };

        assert!(step(&mut state, cfg, 400, &[], true).is_empty());
        assert_eq!(state.active, expected);
        assert_eq!(state.score, distance);
        assert_eq!(state.lock_delay_ms, 400);
        assert_eq!(state.lock_reset_count, 0);
        assert!(step(&mut state, cfg, 99, &[Command::SoftDrop], true).is_empty());
        assert_eq!(state.lock_delay_ms, 499);
        assert_eq!(state.score, distance);
        assert_eq!(
            step(&mut state, cfg, 1, &[Command::SoftDrop], true),
            vec![Event::Locked]
        );
    }

    #[test]
    fn instant_soft_drop_to_stack_does_not_enter_cells_below_ghost() {
        let mut state = state_with(Piece::new(PieceType::O, 3, VISIBLE_START_Y as i32));
        state
            .board
            .set(4, BOARD_HEIGHT as i32 - 5, Some(PieceType::J));
        let expected = ghost_piece(&state);
        let cfg = RulesConfig {
            soft_drop_factor: 0,
            ..config()
        };

        assert!(step(&mut state, cfg, 100, &[], true).is_empty());
        assert_eq!(state.active, expected);
        assert!(can_place(&state.board, state.active));
        assert_eq!(state.lock_delay_ms, 100);
        assert_eq!(state.lock_reset_count, 0);
    }

    #[test]
    fn ordinary_soft_drop_press_and_held_gravity_remain_separate() {
        let mut state = state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32));
        let start_y = state.active.y;

        assert!(step(&mut state, config(), 0, &[Command::SoftDrop], false).is_empty());
        assert_eq!(state.active.y, start_y + 1);
        assert_eq!(state.score, 1);
        assert!(step(&mut state, config(), 100, &[], true).is_empty());
        assert_eq!(state.active.y, start_y + 3);
        assert_eq!(state.score, 1);
        assert_eq!(state.lock_reset_count, 0);
        assert!(!state.lock_started);
    }

    #[test]
    fn jstris_soft_drop_rates_are_independent_of_normal_gravity() {
        for gravity_ms in [100, 500, 1000] {
            for (rate, rows) in [(20, 2), (125, 12), (60, 6), (120, 12)] {
                let mut state = state_with(Piece::new(PieceType::O, 3, VISIBLE_START_Y as i32));
                let start_y = state.active.y;
                let cfg = RulesConfig {
                    gravity_ms,
                    soft_drop_factor: rate,
                    ..config()
                };

                assert!(step(&mut state, cfg, 100, &[], true).is_empty());

                assert_eq!(state.active.y, start_y + rows);
                assert_eq!(state.lock_reset_count, 0);
                assert!(!state.lock_started);
            }
        }
    }

    #[test]
    fn jstris_fractional_soft_drop_rates_are_frame_partition_invariant() {
        for rate in [20, 125, 60, 120] {
            let cfg = RulesConfig {
                gravity_ms: 1000,
                soft_drop_factor: rate,
                ..config()
            };
            let piece = Piece::new(PieceType::O, 3, VISIBLE_START_Y as i32);
            let mut whole = state_with(piece);
            let mut separate = state_with(piece);
            let events = step(&mut whole, cfg, 1300, &[], true);
            let mut separate_events = Vec::new();
            for _ in 0..1300 {
                separate_events.extend(step(&mut separate, cfg, 1, &[], true));
            }

            assert_eq!(events, separate_events);
            assert_same_state(&whole, &separate);
        }
    }

    #[test]
    fn faster_normal_gravity_is_not_slowed_by_soft_drop() {
        let mut state = state_with(Piece::new(PieceType::O, 3, VISIBLE_START_Y as i32));
        let start_y = state.active.y;
        let cfg = RulesConfig {
            gravity_ms: 5,
            soft_drop_factor: 60,
            ..config()
        };

        assert!(step(&mut state, cfg, 50, &[], true).is_empty());
        assert_eq!(state.active.y, start_y + 10);
        assert_eq!(state.soft_drop_accum_millirows, 0);
    }

    #[test]
    fn jstris_attack_uses_additive_combo_table_without_surge() {
        let state = GameState::new(1);
        let bonuses = [0, 0, 1, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5, 5, 5];
        for (combo, bonus) in bonuses.into_iter().enumerate() {
            assert_eq!(
                compute_attack_lines(&state, None, 4, false, combo as u32, 0),
                4 + bonus,
            );
            assert_eq!(
                compute_attack_lines(&state, None, 1, false, combo as u32, 20),
                bonus,
            );
        }
    }

    #[test]
    fn jstris_mini_double_matches_full_double_and_mini_single_has_no_b2b_attack_bonus() {
        let state = GameState::new(1);
        assert_eq!(base_points(Some(SpinKind::Mini), 2), 1200);
        assert_eq!(
            compute_attack_lines(&state, Some(SpinKind::Mini), 2, false, 0, 0),
            4,
        );
        assert_eq!(
            compute_attack_lines(&state, Some(SpinKind::Mini), 1, false, 0, 10),
            0,
        );
    }

    #[test]
    fn jstris_perfect_clear_attack_replaces_base_and_b2b_while_keeping_combo() {
        let state = GameState::new(1);
        // Zero prior combo: PC replaces base+B2B with 10.
        assert_eq!(compute_attack_lines(&state, None, 4, true, 0, 10), 10);
        // Prior combo 12 adds +5 combo attack on top of 10 (verified in replayer.js checkLineClears).
        assert_eq!(compute_attack_lines(&state, None, 4, true, 12, 10), 15);
    }

    #[test]
    fn jstris_first_clear_scores_no_combo_bonus_and_pc_adds_three_thousand() {
        let mut state = GameState::new(1);
        apply_scoring(&mut state, None, 1, true);
        assert_eq!(state.score, 3100);
        apply_scoring(&mut state, None, 1, false);
        assert_eq!(state.score, 3250);
    }

    #[test]
    fn hard_drop_locks_and_stops_later_commands() {
        let mut state = state_with(Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32));

        assert_eq!(
            step(
                &mut state,
                config(),
                100,
                &[Command::HardDrop, Command::MoveRight, Command::Hold],
                false,
            ),
            vec![Event::HardDropped, Event::Locked]
        );
        assert_eq!(state.pieces_placed, 1);
        assert_eq!(state.active.x, 3);
        assert_eq!(state.active.y, DEADLINE_Y);
        assert_eq!(state.hold, None);
        assert_eq!(state.gravity_accum_ms, 0);
    }

    #[test]
    fn jstris_deadline_row_is_nonsolid_for_movement_and_locks_out_when_all_cells_above_visible() {
        // 1. Nonsolid movement: a piece can move through an occupied deadline cell.
        let mut state = state_with(Piece::new(PieceType::T, 2, DEADLINE_Y));
        state.board.set(4, DEADLINE_Y, Some(PieceType::J));
        let events = step(&mut state, config(), 0, &[Command::MoveRight], false);
        assert_eq!(events, vec![Event::Moved]);
        assert_eq!(state.active.x, 3);

        // 2. Partial lock into deadline writes only empty deadline cells and survives.
        let mut partial = state_with(Piece::new(PieceType::O, 0, DEADLINE_Y));
        partial.board.set(1, DEADLINE_Y, Some(PieceType::Z)); // existing deadline cell must not be overwritten
        partial
            .board
            .set(1, VISIBLE_START_Y as i32 + 1, Some(PieceType::I)); // support so O locks at y = DEADLINE_Y (cols 1, 2)
        let events = step(&mut partial, config(), 0, &[Command::HardDrop], false);
        assert!(events.contains(&Event::Locked));
        assert!(!partial.game_over);
        assert_eq!(partial.board.get(1, DEADLINE_Y), Some(PieceType::Z));
        assert_eq!(partial.board.get(2, DEADLINE_Y), Some(PieceType::O));

        // 3. Locking completely above visible field (all 4 cells in/above deadline) -> lock-out GameOver!
        let mut lockout = state_with(Piece::new(PieceType::O, 3, DEADLINE_Y - 1));
        lockout
            .board
            .set(4, VISIBLE_START_Y as i32, Some(PieceType::I));
        let events = step(&mut lockout, config(), 0, &[Command::HardDrop], false);
        assert!(lockout.game_over);
        assert_eq!(
            events,
            vec![Event::HardDropped, Event::Locked, Event::GameOver]
        );
    }

    #[test]
    fn jstris_deadline_row_clears_when_full_and_pulls_down_on_visible_line_clear() {
        // 1. Full deadline row clears +1 line and becomes empty.
        // O at x = 7 occupies columns 8 and 9.
        let mut state = state_with(Piece::new(PieceType::O, 7, DEADLINE_Y));
        for x in 0..8 {
            state.board.set(x, DEADLINE_Y, Some(PieceType::J));
        }
        // Support directly beneath O so O locks straddling deadline & row 1 (filling cols 8, 9 of deadline!)
        state
            .board
            .set(8, VISIBLE_START_Y as i32 + 1, Some(PieceType::I));
        let events = step(&mut state, config(), 0, &[Command::HardDrop], false);
        assert!(events.contains(&Event::LinesCleared(1)));
        assert_eq!(state.lines, 1);
        for x in 0..BOARD_WIDTH as i32 {
            assert_eq!(state.board.get(x, DEADLINE_Y), None);
        }

        // 2. Visible line clear pulls non-full deadline cell down into top visible row (VISIBLE_START_Y).
        let mut pull = state_with(Piece::new(PieceType::I, 6, BOARD_HEIGHT as i32 - 2));
        pull.board.set(0, DEADLINE_Y, Some(PieceType::L));
        for x in 0..6 {
            pull.board
                .set(x, BOARD_HEIGHT as i32 - 1, Some(PieceType::J));
        }
        let events = step(&mut pull, config(), 0, &[Command::HardDrop], false);
        assert!(events.contains(&Event::LinesCleared(1)));
        assert_eq!(pull.board.get(0, DEADLINE_Y), None);
        assert_eq!(
            pull.board.get(0, VISIBLE_START_Y as i32),
            Some(PieceType::L)
        );
    }

    #[test]
    fn srs_i_all_eight_transition_tables_match_standard_y_down() {
        // https://tetris.wiki/SRS uses positive y upward; this engine uses downward.
        let expected_cw = [
            [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
            [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
            [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
            [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
        ];
        let expected_ccw = [
            [(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)],
            [(0, 0), (2, 0), (-1, 0), (2, -1), (-1, 2)],
            [(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)],
            [(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)],
        ];

        for from in 0..4 {
            assert_eq!(
                i_kicks(RotationDir::Cw, Rotation(from)),
                &expected_cw[from as usize],
                "clockwise from {from}",
            );
            assert_eq!(
                i_kicks(RotationDir::Ccw, Rotation(from)),
                &expected_ccw[from as usize],
                "counterclockwise from {from}",
            );
        }
    }

    #[test]
    fn srs_jlstz_all_eight_transition_tables_match_standard_y_down() {
        let expected_cw = [
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        ];
        let expected_ccw = [
            [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],
            [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],
            [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],
            [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)],
        ];

        for from in 0..4 {
            assert_eq!(
                jlstz_kicks(RotationDir::Cw, Rotation(from)),
                &expected_cw[from as usize],
            );
            assert_eq!(
                jlstz_kicks(RotationDir::Ccw, Rotation(from)),
                &expected_ccw[from as usize],
            );
        }
    }

    #[test]
    fn all_pieces_rotate_through_all_eight_transitions_without_translation_in_open_space() {
        let board = Board::new();
        for kind in PieceType::ALL {
            for from in 0..4 {
                for dir in [RotationDir::Cw, RotationDir::Ccw] {
                    let piece = Piece {
                        rotation: Rotation(from),
                        ..Piece::new(kind, 3, VISIBLE_START_Y as i32 + 3)
                    };
                    let target = match dir {
                        RotationDir::Cw => piece.rotation.cw(),
                        RotationDir::Ccw => piece.rotation.ccw(),
                    };
                    assert_eq!(
                        rotate_with_kicks(&board, piece, dir),
                        Some(Piece {
                            rotation: target,
                            ..piece
                        }),
                    );
                }
            }
        }
    }

    #[test]
    fn srs_floor_kicks_have_exact_standard_offsets() {
        let board = Board::new();
        let bottom = BOARD_HEIGHT as i32 - 1;
        for (kind, dir, dx, dy) in [
            (PieceType::T, RotationDir::Cw, -1, -1),
            (PieceType::T, RotationDir::Ccw, 1, -1),
            (PieceType::I, RotationDir::Cw, 1, -2),
            (PieceType::I, RotationDir::Ccw, -1, -2),
        ] {
            let piece = Piece::new(kind, 3, bottom - 1);
            assert!(can_place(&board, piece));
            let target = match dir {
                RotationDir::Cw => piece.rotation.cw(),
                RotationDir::Ccw => piece.rotation.ccw(),
            };

            assert_eq!(
                rotate_with_kicks(&board, piece, dir),
                Some(Piece {
                    rotation: target,
                    x: piece.x + dx,
                    y: piece.y + dy,
                    ..piece
                }),
            );
        }
    }

    #[test]
    fn srs_wall_kicks_have_exact_standard_offsets_on_both_walls() {
        let board = Board::new();
        for (kind, from, dir, x, dx) in [
            (PieceType::T, 1, RotationDir::Cw, -1, 1),
            (PieceType::T, 1, RotationDir::Ccw, -1, 1),
            (PieceType::T, 3, RotationDir::Cw, 8, -1),
            (PieceType::T, 3, RotationDir::Ccw, 8, -1),
            (PieceType::I, 1, RotationDir::Cw, -2, 2),
            (PieceType::I, 1, RotationDir::Ccw, -2, 2),
            (PieceType::I, 1, RotationDir::Cw, 7, -1),
            (PieceType::I, 3, RotationDir::Cw, 8, -2),
        ] {
            let piece = Piece {
                rotation: Rotation(from),
                ..Piece::new(kind, x, VISIBLE_START_Y as i32 + 3)
            };
            assert!(can_place(&board, piece));
            let target = match dir {
                RotationDir::Cw => piece.rotation.cw(),
                RotationDir::Ccw => piece.rotation.ccw(),
            };

            assert_eq!(
                rotate_with_kicks(&board, piece, dir),
                Some(Piece {
                    rotation: target,
                    x: piece.x + dx,
                    ..piece
                }),
            );
        }
    }
}
