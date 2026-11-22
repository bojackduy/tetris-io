use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tetris_io::adapters::audio::{Audio, SoundCue};
use tetris_io::adapters::render::{RenderContext, render_game, render_game_over, render_pause};
use tetris_io::adapters::storage::SqliteStorage as Storage;
use tetris_io::app::config::AppConfig;
use tetris_io::application::flow::GameMode;
use tetris_io::application::session::GameSessionState;
use tetris_io::domain::board::{BOARD_HEIGHT, BOARD_WIDTH, VISIBLE_HEIGHT, VISIBLE_START_Y};
use tetris_io::domain::command::Command;
use tetris_io::domain::events::Event;
use tetris_io::domain::piece::{Piece, PieceType, Rotation};
use tetris_io::domain::rules::{RulesConfig, optimal_finesse, step};
use tetris_io::domain::state::GameState;

fn jstris_rules() -> RulesConfig {
    let cfg = AppConfig::default();
    RulesConfig {
        gravity_ms: cfg.gravity_ms,
        soft_drop_factor: cfg.soft_drop_factor,
        lock_delay_ms: cfg.lock_delay_ms,
        lock_reset_limit: cfg.lock_reset_limit,
    }
}

#[test]
fn jstris_default_config_matches_verified_profile() {
    let cfg = AppConfig::default();
    assert_eq!(cfg.das_ms, 133);
    assert_eq!(cfg.arr_ms, 10);
    assert_eq!(cfg.dcd_ms, 0);
    assert_eq!(cfg.soft_drop_factor, 120);
    assert_eq!(cfg.lock_delay_ms, 500);
    assert_eq!(cfg.lock_reset_limit, 0);
    assert_eq!(cfg.sound_volume, 60);
    assert!(!cfg.rotation_sound);
    assert!(cfg.ghost);
    assert_eq!(cfg.grid_style, 1);
    assert_eq!(VISIBLE_HEIGHT, 20);
    assert_eq!(BOARD_WIDTH, 10);
}

#[test]
fn jstris_180_kicks_match_exact_two_test_shipped_table() {
    let cfg = RulesConfig {
        gravity_ms: 999_999,
        ..jstris_rules()
    };

    // 0 -> 2: tries (0, 0), then (0, -1) (1 cell up) when floor/stack blocks rot 2.
    let mut state = GameState::new(1);
    let bottom = BOARD_HEIGHT as i32 - 1;
    state.active = Piece {
        kind: PieceType::T,
        rotation: Rotation(0),
        x: 4,
        y: bottom - 1, // bottom of T(0) sits on bottom row; T(2) needs y+2 which hits floor
    };
    let events = step(&mut state, cfg, 0, &[Command::Rotate180], false);
    assert!(events.contains(&Event::Rotated));
    assert_eq!(state.active.rotation, Rotation(2));
    assert_eq!(state.active.x, 4);
    assert_eq!(state.active.y, bottom - 2);

    // 1 -> 3: tries (0, 0), then (1, 0) (1 cell right) at left wall.
    let mut state = GameState::new(1);
    state.active = Piece {
        kind: PieceType::T,
        rotation: Rotation(1),
        x: -1,
        y: VISIBLE_START_Y as i32 + 5,
    };
    let events = step(&mut state, cfg, 0, &[Command::Rotate180], false);
    assert!(events.contains(&Event::Rotated));
    assert_eq!(state.active.rotation, Rotation(3));
    assert_eq!(state.active.x, 0);

    // 3 -> 1: tries (0, 0), then (-1, 0) (1 cell left) at right wall.
    let mut state = GameState::new(1);
    state.active = Piece {
        kind: PieceType::T,
        rotation: Rotation(3),
        x: 8,
        y: VISIBLE_START_Y as i32 + 5,
    };
    let events = step(&mut state, cfg, 0, &[Command::Rotate180], false);
    assert!(events.contains(&Event::Rotated));
    assert_eq!(state.active.rotation, Rotation(1));
    assert_eq!(state.active.x, 7);

    // Unlike TETR.IO's 6-kick table, Jstris has NO diagonal 180 kicks.
    let mut blocked = GameState::new(1);
    blocked.active = Piece {
        kind: PieceType::T,
        rotation: Rotation(0),
        x: 4,
        y: bottom - 1,
    };
    // Block (5, bottom) and (5, bottom - 1)'s upward kick cell (5, bottom - 1 is occupied by T; block (4, bottom - 1) etc.)
    blocked.board.set(4, bottom - 1, Some(PieceType::O));
    blocked.board.set(6, bottom - 1, Some(PieceType::O));
    let before = blocked.active;
    let events = step(&mut blocked, cfg, 0, &[Command::Rotate180], false);
    assert!(!events.contains(&Event::Rotated));
    assert_eq!(blocked.active, before);
}

#[test]
fn jstris_optimal_finesse_table_matches_shipped_replayer_values() {
    // Spawn column (no moves needed) = 0 optimal presses for all flat/default pieces
    assert_eq!(optimal_finesse(Piece::new(PieceType::I, 3, 10)), 0);
    assert_eq!(optimal_finesse(Piece::new(PieceType::O, 3, 10)), 0);
    assert_eq!(optimal_finesse(Piece::new(PieceType::T, 3, 10)), 0);
    // DAS to left wall = 1 optimal press
    assert_eq!(optimal_finesse(Piece::new(PieceType::I, 0, 10)), 1);
    assert_eq!(optimal_finesse(Piece::new(PieceType::T, 0, 10)), 1);
    // T rotated 180 at spawn column = 2 optimal presses (per Jstris finesse table row 2)
    assert_eq!(
        optimal_finesse(Piece {
            kind: PieceType::T,
            rotation: Rotation(2),
            x: 3,
            y: 10,
        }),
        2
    );
}

#[test]
fn finesse_faults_accumulate_when_extra_manual_inputs_used() {
    let mut state = GameState::new(1);
    state.active = Piece::new(PieceType::T, 3, VISIBLE_START_Y as i32);
    // Drop at spawn (optimal 0) after 3 unnecessary manual taps -> 3 finesse faults.
    state.add_finesse_inputs(3);
    step(&mut state, jstris_rules(), 0, &[Command::HardDrop], false);
    assert_eq!(state.finesse_faults, 3);
    assert_eq!(state.piece_manual_inputs, 0);
}

#[test]
fn session_tracks_subsecond_elapsed_and_remaining_ms() {
    let mut session = GameSessionState::new(1, GameMode::Blitz { duration_secs: 120 });
    let t0 = session.start_time;
    let t1 = t0 + Duration::from_millis(12_345);
    assert_eq!(session.elapsed_ms(t1), 12_345);
    assert_eq!(session.remaining_ms(t1), Some(107_655));

    session.pause(t1);
    let t2 = t1 + Duration::from_millis(5_000);
    assert_eq!(session.elapsed_ms(t2), 12_345);
    session.resume(t2);
    let t3 = t2 + Duration::from_millis(655);
    assert_eq!(session.elapsed_ms(t3), 13_000);
}

#[test]
fn sqlite_migration_upgrades_legacy_v1_schema_preserving_scores_and_bindings() {
    let dir = std::env::temp_dir().join(format!(
        "tetris_io_migration_test_{}",
        Instant::now().elapsed().as_nanos()
    ));
    let _ = std::fs::create_dir_all(&dir);
    let db_path = dir.join("legacy.db");
    let _ = std::fs::remove_file(&db_path);

    // Create a legacy v1 table without the new columns and with old default timings + custom key.
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open raw sqlite");
        conn.execute_batch(
            "CREATE TABLE config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                gravity_ms INTEGER NOT NULL,
                frame_ms INTEGER NOT NULL,
                das_ms INTEGER NOT NULL,
                arr_ms INTEGER NOT NULL,
                dcd_ms INTEGER NOT NULL,
                soft_drop_factor INTEGER NOT NULL,
                lock_delay_ms INTEGER NOT NULL,
                lock_reset_limit INTEGER NOT NULL,
                key_move_left TEXT NOT NULL,
                key_move_right TEXT NOT NULL,
                key_soft_drop TEXT NOT NULL,
                key_rotate_left TEXT NOT NULL,
                key_rotate_right TEXT NOT NULL,
                key_rotate_180 TEXT NOT NULL,
                key_hold TEXT NOT NULL,
                key_hard_drop TEXT NOT NULL,
                key_quit TEXT NOT NULL
            );
            INSERT INTO config VALUES (
                1, 1000, 16, 167, 16, 33, 6, 700, 10,
                'Left', 'Right', 'Down', 'z', 'Up', 'a', 'c', 'Space', 'q'
            );
            CREATE TABLE high_scores (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                score INTEGER NOT NULL,
                lines INTEGER NOT NULL,
                time_secs INTEGER NOT NULL,
                mode TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            INSERT INTO high_scores (name, score, lines, time_secs, mode, created_at)
            VALUES ('ACE', 42000, 40, 55, '40 Lines', 1700000000);",
        )
        .expect("seed v1 db");
    }

    let storage = Storage::open(db_path.clone()).expect("migrate legacy db");
    let cfg = storage
        .load_config()
        .expect("load config")
        .expect("config row exists");
    assert_eq!(cfg.das_ms, 133);
    assert_eq!(cfg.arr_ms, 10);
    assert_eq!(cfg.dcd_ms, 0);
    assert_eq!(cfg.soft_drop_factor, 120);
    assert_eq!(cfg.lock_delay_ms, 500);
    assert_eq!(cfg.lock_reset_limit, 0);
    assert_eq!(cfg.sound_volume, 60);
    assert!(!cfg.rotation_sound);
    assert!(cfg.ghost);
    assert_eq!(cfg.grid_style, 1);
    assert_eq!(cfg.key_hold, "c");

    let scores = storage.fetch_high_scores(5).expect("fetch scores");
    assert_eq!(scores.len(), 1);
    assert_eq!(scores[0].name, "ACE");
    assert_eq!(scores[0].score, 42000);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ratatui_renderer_displays_jstris_hud_and_input_capability_on_standard_80x24() {
    let mut state = GameState::new(7);
    state.hold = Some(PieceType::T);
    state.pieces_placed = 12;
    state.lines = 4;
    state.attack = 5;
    state.finesse_faults = 2;
    state.score = 1250;
    state.last_clear_label = Some("T-Spin Double | B2B x1 | Combo x2".to_string());
    state.receive_garbage(4);

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");

    // 1. Enhanced input + Standard grid + Ghost ON
    let ctx_enh = RenderContext {
        time_ms: 12_340,
        time_label: "Time",
        enhanced_input: true,
        ghost: true,
        grid_style: 1,
        key_presses: 36,
        lock_delay_ms: 500,
        mode_label: "40 LINES",
        lines_target: Some(40),
    };
    terminal
        .draw(|f| render_game(f, &state, ctx_enh))
        .expect("draw enhanced");
    let text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(text.contains("HOLD"));
    assert!(text.contains("NEXT"));
    assert!(text.contains("0:12.34"));
    assert!(text.contains("PPS"));
    assert!(text.contains("KPP"));
    assert!(text.contains("Finesse"));
    assert!(text.contains("ENHANCED DAS/ARR"));
    assert!(text.contains("T-Spin Double"));
    assert!(
        text.contains("░░"),
        "ghost piece should render when enabled"
    );

    // 2. Legacy input warning + Ghost OFF + all grid styles render cleanly
    for grid_style in 0..=4 {
        let ctx_leg = RenderContext {
            enhanced_input: false,
            ghost: false,
            grid_style,
            ..ctx_enh
        };
        terminal
            .draw(|f| render_game(f, &state, ctx_leg))
            .expect("draw legacy");
        let buf_text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(buf_text.contains("LEGACY INPUT"));
        assert!(
            !buf_text.contains("░░"),
            "ghost must not render when disabled"
        );
    }

    // 3. Pause and Game Over overlays render cleanly with full Jstris summary stats
    terminal
        .draw(|f| render_pause(f, &state, ctx_enh, 0))
        .expect("draw pause");
    let pause_text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for stat in ["PAUSED", "PPS", "KPP", "Finesse", "Attack", "0:12.34"] {
        assert!(pause_text.contains(stat), "Pause missing {stat}");
    }

    terminal
        .draw(|f| render_game_over(f, &state, ctx_enh, 0, "40 LINES"))
        .expect("draw game over");
    let over_text: String = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for stat in [
        "40 LINES", "PPS", "KPP", "Finesse", "Attack", "Pieces", "0:12.34",
    ] {
        assert!(over_text.contains(stat), "GameOver missing {stat}");
    }
}

#[test]
fn audio_adapter_is_nonblocking_and_handles_all_cues_safely() {
    let audio = Audio::new();
    // Play at volume 0 so CI/headless environments remain silent while exercising the path.
    for cue in [
        SoundCue::Start,
        SoundCue::Rotate,
        SoundCue::HardDrop,
        SoundCue::Lock,
        SoundCue::Combo(1),
        SoundCue::Combo(15),
        SoundCue::PerfectClear,
        SoundCue::GameOver,
    ] {
        audio.play(cue, 0);
    }
}
