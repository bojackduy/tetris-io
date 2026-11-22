use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use tetris_io::adapters::input::{InputConfig, InputState, KeyBindings};
use tetris_io::domain::board::BOARD_WIDTH;
use tetris_io::domain::command::Command;
use tetris_io::domain::piece::{Piece, PieceType};
use tetris_io::domain::rules::{RulesConfig, step};
use tetris_io::domain::state::GameState;

fn at(start: Instant, ms: u64) -> Instant {
    start + Duration::from_millis(ms)
}

fn input(das_ms: u64, arr_ms: u64, dcd_ms: u64) -> InputState {
    InputState::new(
        InputConfig {
            das_ms,
            arr_ms,
            dcd_ms,
        },
        KeyBindings::default(),
    )
}

fn key(input: &mut InputState, code: KeyCode, kind: KeyEventKind, now: Instant) -> Vec<Command> {
    let mut out = Vec::new();
    input.handle_event(
        Event::Key(KeyEvent::new_with_kind(code, KeyModifiers::NONE, kind)),
        now,
        &mut out,
    );
    out
}

fn poll(input: &mut InputState, now: Instant) -> Vec<Command> {
    let mut out = Vec::new();
    input.repeat_commands(now, &mut out);
    out
}

fn enhanced(das_ms: u64, arr_ms: u64, dcd_ms: u64, start: Instant) -> InputState {
    let mut input = input(das_ms, arr_ms, dcd_ms);
    assert!(key(&mut input, KeyCode::Left, KeyEventKind::Release, start).is_empty());
    assert!(input.enhanced_input_active());
    input
}

#[test]
fn negotiated_enhancement_is_active_before_the_first_keyboard_event() {
    let start = Instant::now();
    let mut input = input(100, 4, 33);
    input.set_enhanced_input(true);
    assert!(input.enhanced_input_active());
    assert_eq!(
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start),
        [Command::MoveLeft]
    );
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Repeat,
            at(start, 50)
        )
        .is_empty()
    );
    // Reapplying a negotiated capability must not reset a charged hold.
    input.set_enhanced_input(true);
    assert_eq!(poll(&mut input, at(start, 116)), [Command::MoveLeft; 5]);
    input.set_enhanced_input(false);
    assert!(!input.enhanced_input_active());
    assert!(poll(&mut input, at(start, 500)).is_empty());
    assert_eq!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Press,
            at(start, 501)
        ),
        [Command::MoveLeft]
    );
}

#[test]
fn explicit_enhancement_retains_keys_already_pressed_in_legacy_mode() {
    let start = Instant::now();
    let mut input = input(100, 4, 33);
    key(&mut input, KeyCode::Right, KeyEventKind::Press, start);
    key(&mut input, KeyCode::Down, KeyEventKind::Press, start);
    input.set_enhanced_input(true);
    assert!(input.soft_drop_active());
    assert!(poll(&mut input, at(start, 99)).is_empty());
    assert_eq!(poll(&mut input, at(start, 100)), [Command::MoveRight]);
}

#[test]
fn press_moves_one_cell_then_waits_for_das() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    assert_eq!(
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start),
        [Command::MoveLeft]
    );
    assert!(poll(&mut input, at(start, 99)).is_empty());
    assert_eq!(poll(&mut input, at(start, 100)), [Command::MoveLeft]);
    assert!(poll(&mut input, at(start, 103)).is_empty());
    assert_eq!(poll(&mut input, at(start, 104)), [Command::MoveLeft]);
}

#[test]
fn four_ms_arr_catches_up_over_sixteen_ms() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    assert_eq!(poll(&mut input, at(start, 100)), [Command::MoveLeft]);
    assert_eq!(poll(&mut input, at(start, 116)), [Command::MoveLeft; 4]);
}

#[test]
fn arr_is_frame_partition_invariant_and_does_not_drift() {
    let start = Instant::now();
    let run = |times: &[u64]| {
        let mut input = enhanced(100, 4, 33, start);
        let mut out = key(&mut input, KeyCode::Right, KeyEventKind::Press, start);
        for &ms in times {
            out.extend(poll(&mut input, at(start, ms)));
        }
        out
    };
    let fine: Vec<_> = (1..=149).collect();
    let expected = vec![Command::MoveRight; 14];
    assert_eq!(run(&fine), expected);
    assert_eq!(run(&[99, 101, 117, 133, 149]), expected);
    assert_eq!(run(&[149]), expected);
}

#[test]
fn zero_arr_emits_one_bounded_wall_shift_each_poll_after_das() {
    let start = Instant::now();
    for (code, command) in [
        (KeyCode::Left, Command::MoveLeft),
        (KeyCode::Right, Command::MoveRight),
    ] {
        let mut input = enhanced(100, 0, 33, start);
        let mut state = GameState::new(1);
        state.active = Piece::new(PieceType::T, 3, 0);
        let rules = RulesConfig {
            gravity_ms: 1000,
            soft_drop_factor: 6,
            lock_delay_ms: 700,
            lock_reset_limit: 10,
        };
        let press = key(&mut input, code, KeyEventKind::Press, start);
        step(&mut state, rules, 0, &press, false);
        assert!(poll(&mut input, at(start, 99)).is_empty());
        for ms in [100, 100, 116, 500] {
            let out = poll(&mut input, at(start, ms));
            assert_eq!(out, vec![command; BOARD_WIDTH]);
            step(&mut state, rules, 0, &out, false);
            let blocks = state.active.blocks();
            if code == KeyCode::Left {
                assert_eq!(blocks.iter().map(|(x, _)| *x).min(), Some(0));
            } else {
                assert_eq!(
                    blocks.iter().map(|(x, _)| *x).max(),
                    Some(BOARD_WIDTH as i32 - 1)
                );
            }
        }
    }
}

#[test]
fn enhanced_hold_survives_five_hundred_ms_without_os_repeats() {
    let start = Instant::now();
    let mut input = enhanced(100, 16, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    assert_eq!(poll(&mut input, at(start, 100)), [Command::MoveLeft]);
    assert_eq!(poll(&mut input, at(start, 500)), [Command::MoveLeft; 25]);
    assert_eq!(poll(&mut input, at(start, 1012)), [Command::MoveLeft; 32]);
}

#[test]
fn enhanced_os_repeats_do_not_reset_das() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Repeat,
            at(start, 90)
        )
        .is_empty()
    );
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Repeat,
            at(start, 99)
        )
        .is_empty()
    );
    assert_eq!(poll(&mut input, at(start, 116)), [Command::MoveLeft; 5]);
}

#[test]
fn wezterm_space_and_actions_without_release_never_deadlock() {
    let start = Instant::now();
    let mut input = enhanced(133, 10, 0, start);
    for (code, command) in [
        (KeyCode::Char(' '), Command::HardDrop),
        (KeyCode::Char('z'), Command::RotateLeft),
        (KeyCode::Char('a'), Command::Rotate180),
        (KeyCode::Char('c'), Command::Hold),
        (KeyCode::Up, Command::RotateRight),
    ] {
        // Two Press events with NO Release in between (WezTerm 20240203 raw ASCII keys).
        assert_eq!(key(&mut input, code, KeyEventKind::Press, start), [command]);
        assert!(key(&mut input, code, KeyEventKind::Repeat, at(start, 10)).is_empty());
        assert_eq!(
            key(&mut input, code, KeyEventKind::Press, at(start, 200)),
            [command]
        );
    }
    assert!(input.enhanced_input_active());
}

#[test]
fn wezterm_style_duplicate_press_while_held_keeps_enhanced_das_without_resetting() {
    let start = Instant::now();
    let mut input = enhanced(100, 10, 0, start);
    assert_eq!(
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start),
        [Command::MoveLeft]
    );
    // WezTerm 20240203 on macOS sends OS key-repeat as Press instead of Repeat while still
    // sending Release on key-up. Must stay in enhanced mode and not reset DAS or double-move.
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Press,
            at(start, 90)
        )
        .is_empty()
    );
    assert!(input.enhanced_input_active());
    assert_eq!(poll(&mut input, at(start, 110)), [Command::MoveLeft; 2]);
    // Release ends the hold cleanly.
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Release,
            at(start, 120)
        )
        .is_empty()
    );
    assert!(poll(&mut input, at(start, 500)).is_empty());
}

#[test]
fn release_never_emits_a_move_or_leaves_phantom_repeats() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Release,
            at(start, 100)
        )
        .is_empty()
    );
    for ms in [100, 116, 500] {
        assert!(poll(&mut input, at(start, ms)).is_empty());
    }
}

#[test]
fn fresh_opposite_press_gets_last_input_priority_but_no_unearned_das() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    poll(&mut input, at(start, 100));
    assert_eq!(
        key(
            &mut input,
            KeyCode::Right,
            KeyEventKind::Press,
            at(start, 101)
        ),
        [Command::MoveRight]
    );
    assert!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Repeat,
            at(start, 120)
        )
        .is_empty()
    );
    assert!(poll(&mut input, at(start, 200)).is_empty());
    assert_eq!(poll(&mut input, at(start, 201)), [Command::MoveRight]);
}

#[test]
fn releasing_active_direction_resumes_charged_opposite_after_dcd() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Press,
        at(start, 20),
    );
    assert!(
        key(
            &mut input,
            KeyCode::Right,
            KeyEventKind::Release,
            at(start, 120)
        )
        .is_empty()
    );
    assert!(poll(&mut input, at(start, 152)).is_empty());
    assert_eq!(poll(&mut input, at(start, 153)), [Command::MoveLeft]);
    assert_eq!(poll(&mut input, at(start, 169)), [Command::MoveLeft; 4]);
}

#[test]
fn resumed_opposite_still_needs_its_own_das_charge() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Press,
        at(start, 10),
    );
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Release,
        at(start, 20),
    );
    assert!(poll(&mut input, at(start, 99)).is_empty());
    assert_eq!(poll(&mut input, at(start, 100)), [Command::MoveLeft]);
}

#[test]
fn zero_dcd_resumes_charged_opposite_without_replaying_inactive_time() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 0, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Press,
        at(start, 20),
    );
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Release,
        at(start, 200),
    );
    assert_eq!(poll(&mut input, at(start, 200)), [Command::MoveLeft]);
}

#[test]
fn releasing_inactive_direction_does_not_restart_active_das() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    key(
        &mut input,
        KeyCode::Right,
        KeyEventKind::Press,
        at(start, 20),
    );
    key(
        &mut input,
        KeyCode::Left,
        KeyEventKind::Release,
        at(start, 80),
    );
    assert!(poll(&mut input, at(start, 119)).is_empty());
    assert_eq!(poll(&mut input, at(start, 120)), [Command::MoveRight]);
}

#[test]
fn enhanced_actions_fire_only_on_press_edges() {
    let start = Instant::now();
    for (code, command) in [
        (KeyCode::Char('z'), Command::RotateLeft),
        (KeyCode::Up, Command::RotateRight),
        (KeyCode::Char('a'), Command::Rotate180),
        (KeyCode::Char('c'), Command::Hold),
        (KeyCode::Char(' '), Command::HardDrop),
    ] {
        let mut input = enhanced(100, 4, 33, start);
        assert_eq!(key(&mut input, code, KeyEventKind::Press, start), [command]);
        for ms in [1, 2] {
            assert!(key(&mut input, code, KeyEventKind::Repeat, at(start, ms)).is_empty());
        }
        assert!(key(&mut input, code, KeyEventKind::Release, at(start, 3)).is_empty());
        assert_eq!(
            key(&mut input, code, KeyEventKind::Press, at(start, 4)),
            [command]
        );
    }
}

#[test]
fn soft_drop_is_one_press_command_and_held_state_not_horizontal_repeats() {
    let start = Instant::now();
    for (das_ms, arr_ms) in [(0, 0), (100, 4), (500, 500)] {
        let mut input = enhanced(das_ms, arr_ms, 33, start);
        assert_eq!(
            key(&mut input, KeyCode::Down, KeyEventKind::Press, start),
            [Command::SoftDrop]
        );
        for ms in [1, 100, 500, 1000] {
            assert!(
                key(
                    &mut input,
                    KeyCode::Down,
                    KeyEventKind::Repeat,
                    at(start, ms)
                )
                .is_empty()
            );
            assert!(poll(&mut input, at(start, ms)).is_empty());
            assert!(input.soft_drop_active());
        }
        key(
            &mut input,
            KeyCode::Down,
            KeyEventKind::Release,
            at(start, 1002),
        );
        assert!(!input.soft_drop_active());
        assert!(poll(&mut input, at(start, 2000)).is_empty());
    }
}

#[test]
fn legacy_fallback_moves_once_per_os_press_and_never_invents_a_hold() {
    let start = Instant::now();
    let mut input = input(100, 0, 33);
    assert!(!input.enhanced_input_active());
    for ms in [0, 30, 60, 90, 500] {
        assert_eq!(
            key(
                &mut input,
                KeyCode::Left,
                KeyEventKind::Press,
                at(start, ms)
            ),
            [Command::MoveLeft]
        );
        assert!(poll(&mut input, at(start, ms + 1)).is_empty());
    }
    assert!(poll(&mut input, at(start, 1000)).is_empty());
    assert!(!input.enhanced_input_active());
}

#[test]
fn legacy_soft_drop_is_discrete_without_a_phantom_held_flag() {
    let start = Instant::now();
    let mut input = input(0, 0, 33);
    for ms in [0, 30, 500] {
        assert_eq!(
            key(
                &mut input,
                KeyCode::Down,
                KeyEventKind::Press,
                at(start, ms)
            ),
            [Command::SoftDrop]
        );
        assert!(!input.soft_drop_active());
        assert!(poll(&mut input, at(start, ms + 1)).is_empty());
    }
}

#[test]
fn repeat_without_release_stays_in_legacy_mode_and_never_sticks() {
    let start = Instant::now();
    let mut input = input(16, 16, 33);
    assert_eq!(
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start),
        [Command::MoveLeft]
    );
    // Repeat without Release must NOT promote to enhanced_input (otherwise the key sticks forever).
    assert_eq!(
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Repeat,
            at(start, 32)
        ),
        [Command::MoveLeft]
    );
    assert!(!input.enhanced_input_active());
    assert!(poll(&mut input, at(start, 200)).is_empty());
}

#[test]
fn first_release_migrates_legacy_downstate_without_retroactive_repeats() {
    let start = Instant::now();
    let mut input = input(16, 16, 33);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    key(&mut input, KeyCode::Down, KeyEventKind::Press, start);
    key(&mut input, KeyCode::Char('z'), KeyEventKind::Press, start);
    assert!(
        key(
            &mut input,
            KeyCode::Char('z'),
            KeyEventKind::Release,
            at(start, 32)
        )
        .is_empty()
    );
    assert!(input.enhanced_input_active());
    assert!(input.soft_drop_active());
    assert_eq!(poll(&mut input, at(start, 32)), [Command::MoveLeft]);
    assert_eq!(poll(&mut input, at(start, 48)), [Command::MoveLeft]);
}

#[test]
fn focus_loss_and_pause_clear_every_old_motion_timestamp() {
    let start = Instant::now();
    for focus_loss in [false, true] {
        let mut input = enhanced(100, 4, 33, start);
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
        key(&mut input, KeyCode::Down, KeyEventKind::Press, start);
        if focus_loss {
            let mut out = Vec::new();
            input.handle_event(Event::FocusLost, at(start, 90), &mut out);
            assert!(out.is_empty());
        } else {
            input.clear_motion();
        }
        assert!(!input.soft_drop_active());
        assert!(poll(&mut input, at(start, 500)).is_empty());
        assert!(input.enhanced_input_active());
        assert_eq!(
            key(
                &mut input,
                KeyCode::Left,
                KeyEventKind::Press,
                at(start, 501)
            ),
            [Command::MoveLeft]
        );
        assert!(poll(&mut input, at(start, 600)).is_empty());
        assert_eq!(poll(&mut input, at(start, 601)), [Command::MoveLeft]);
    }
}

#[test]
fn legacy_clear_motion_does_not_suppress_a_quick_second_tap() {
    let start = Instant::now();
    let mut input = input(100, 4, 33);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    input.clear_motion();
    assert_eq!(
        key(&mut input, KeyCode::Left, KeyEventKind::Press, at(start, 1)),
        [Command::MoveLeft]
    );
}

#[test]
fn lockout_tracks_presses_releases_and_priority_without_emitting_commands() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    input.lockout_movement(start, 200);
    for (code, kind, ms) in [
        (KeyCode::Left, KeyEventKind::Press, 10),
        (KeyCode::Down, KeyEventKind::Press, 20),
        (KeyCode::Right, KeyEventKind::Press, 30),
        (KeyCode::Right, KeyEventKind::Release, 150),
    ] {
        assert!(key(&mut input, code, kind, at(start, ms)).is_empty());
    }
    assert!(!input.soft_drop_active());
    assert!(poll(&mut input, at(start, 199)).is_empty());
    assert_eq!(poll(&mut input, at(start, 200)), [Command::MoveLeft]);
    assert!(input.soft_drop_active());
    assert_eq!(poll(&mut input, at(start, 216)), [Command::MoveLeft; 4]);
}

#[test]
fn lockout_discards_suppressed_repeats_and_release_cancels_resume() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
    poll(&mut input, at(start, 100));
    input.lockout_movement(at(start, 101), 100);
    assert!(poll(&mut input, at(start, 200)).is_empty());
    assert_eq!(poll(&mut input, at(start, 201)), [Command::MoveLeft]);
    input.lockout_movement(at(start, 202), 100);
    key(
        &mut input,
        KeyCode::Left,
        KeyEventKind::Release,
        at(start, 203),
    );
    assert!(poll(&mut input, at(start, 500)).is_empty());
}

#[test]
fn press_near_lockout_end_still_needs_full_das() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    input.lockout_movement(start, 200);
    assert!(
        key(
            &mut input,
            KeyCode::Right,
            KeyEventKind::Press,
            at(start, 190)
        )
        .is_empty()
    );
    assert!(poll(&mut input, at(start, 200)).is_empty());
    assert!(poll(&mut input, at(start, 289)).is_empty());
    assert_eq!(poll(&mut input, at(start, 290)), [Command::MoveRight]);
}

#[test]
fn lockout_retains_last_input_priority_and_blocks_action_edges() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    input.lockout_movement(start, 200);
    for (code, ms) in [
        (KeyCode::Left, 10),
        (KeyCode::Right, 20),
        (KeyCode::Down, 30),
        (KeyCode::Char(' '), 40),
    ] {
        assert!(key(&mut input, code, KeyEventKind::Press, at(start, ms)).is_empty());
    }
    key(
        &mut input,
        KeyCode::Down,
        KeyEventKind::Release,
        at(start, 100),
    );
    assert_eq!(poll(&mut input, at(start, 200)), [Command::MoveRight]);
    assert!(!input.soft_drop_active());
    assert!(
        key(
            &mut input,
            KeyCode::Char(' '),
            KeyEventKind::Repeat,
            at(start, 201)
        )
        .is_empty()
    );
    key(
        &mut input,
        KeyCode::Char(' '),
        KeyEventKind::Release,
        at(start, 202),
    );
    assert_eq!(
        key(
            &mut input,
            KeyCode::Char(' '),
            KeyEventKind::Press,
            at(start, 203)
        ),
        [Command::HardDrop]
    );
}

#[test]
fn focus_loss_clears_action_edges_and_lockout_deadlines() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    key(&mut input, KeyCode::Char('z'), KeyEventKind::Press, start);
    input.lockout_movement(start, 1000);
    input.handle_event(Event::FocusLost, at(start, 10), &mut Vec::new());
    assert_eq!(
        key(
            &mut input,
            KeyCode::Char('z'),
            KeyEventKind::Press,
            at(start, 11)
        ),
        [Command::RotateLeft]
    );
    assert_eq!(
        key(
            &mut input,
            KeyCode::Right,
            KeyEventKind::Press,
            at(start, 11)
        ),
        [Command::MoveRight]
    );
    assert_eq!(poll(&mut input, at(start, 111)), [Command::MoveRight]);
}

#[test]
fn enhanced_hold_counts_one_manual_press_despite_arr_and_os_repeats() {
    let start = Instant::now();
    for arr_ms in [0, 4, 16] {
        let mut input = enhanced(100, arr_ms, 33, start);
        assert_eq!(input.key_presses(), 0);
        key(&mut input, KeyCode::Left, KeyEventKind::Press, start);
        for ms in [50, 100, 116, 500] {
            assert!(
                key(
                    &mut input,
                    KeyCode::Left,
                    KeyEventKind::Repeat,
                    at(start, ms)
                )
                .is_empty()
            );
            input.set_enhanced_input(true);
            poll(&mut input, at(start, ms));
        }
        assert!(
            key(
                &mut input,
                KeyCode::Left,
                KeyEventKind::Repeat,
                at(start, 501)
            )
            .is_empty()
        );
        assert_eq!(input.key_presses(), 1);
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Release,
            at(start, 502),
        );
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Press,
            at(start, 503),
        );
        assert_eq!(input.key_presses(), 2);
    }
}

#[test]
fn manual_press_count_covers_game_actions_not_quit_and_survives_pause() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    let game_keys = [
        KeyCode::Left,
        KeyCode::Right,
        KeyCode::Down,
        KeyCode::Char('z'),
        KeyCode::Up,
        KeyCode::Char('a'),
        KeyCode::Char('c'),
        KeyCode::Char(' '),
    ];
    for (index, code) in game_keys.into_iter().enumerate() {
        let now = at(start, index as u64);
        assert_eq!(key(&mut input, code, KeyEventKind::Press, now).len(), 1);
        assert!(key(&mut input, code, KeyEventKind::Repeat, now).is_empty());
        key(&mut input, code, KeyEventKind::Release, now);
        assert_eq!(input.key_presses(), index as u32 + 1);
    }
    for code in [KeyCode::Char('q'), KeyCode::Esc] {
        assert_eq!(
            key(&mut input, code, KeyEventKind::Press, at(start, 10)),
            [Command::Quit]
        );
        key(&mut input, code, KeyEventKind::Release, at(start, 10));
    }
    input.clear_motion();
    input.handle_event(Event::FocusLost, at(start, 11), &mut Vec::new());
    assert_eq!(input.key_presses(), 8);
    let fresh = InputState::new(
        InputConfig {
            das_ms: 100,
            arr_ms: 4,
            dcd_ms: 33,
        },
        KeyBindings::default(),
    );
    assert_eq!(fresh.key_presses(), 0);
}

#[test]
fn lockout_does_not_count_suppressed_manual_presses_or_resumed_arr() {
    let start = Instant::now();
    let mut input = enhanced(100, 4, 33, start);
    input.lockout_movement(start, 200);
    for code in [
        KeyCode::Left,
        KeyCode::Down,
        KeyCode::Char(' '),
        KeyCode::Char('z'),
    ] {
        assert!(key(&mut input, code, KeyEventKind::Press, at(start, 10)).is_empty());
    }
    assert_eq!(input.key_presses(), 0);
    assert_eq!(poll(&mut input, at(start, 200)), [Command::MoveLeft]);
    assert_eq!(input.key_presses(), 0);
    key(
        &mut input,
        KeyCode::Left,
        KeyEventKind::Release,
        at(start, 201),
    );
    key(
        &mut input,
        KeyCode::Left,
        KeyEventKind::Press,
        at(start, 202),
    );
    assert_eq!(input.key_presses(), 1);
}

#[test]
fn legacy_manual_press_count_honestly_includes_indistinguishable_os_presses() {
    let start = Instant::now();
    let mut input = input(100, 4, 33);
    for ms in [0, 30, 60] {
        key(
            &mut input,
            KeyCode::Left,
            KeyEventKind::Press,
            at(start, ms),
        );
    }
    assert_eq!(input.key_presses(), 3);
    assert!(poll(&mut input, at(start, 1000)).is_empty());
    input.clear_motion();
    assert_eq!(input.key_presses(), 3);
}
