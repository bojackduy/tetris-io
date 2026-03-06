use std::io;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use crate::adapters::input::{InputConfig, InputState, KeyBindings, keycode_label};
use crate::adapters::net::{NetClient, NetLobby, NetMessage};
use crate::adapters::render::{
    RenderContext, render_game, render_game_over, render_game_over_custom, render_info_screen,
    render_menu, render_pause, render_settings,
};
use crate::domain::command::Command;
use crate::domain::events::Event as GameEvent;
use crate::domain::rng::random_seed;
use crate::domain::rules::RulesConfig;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::app::config::AppConfig;
use crate::app::storage::{HighScore, Storage};
use crate::application::flow::{
    GAME_OVER_ITEMS, GameMode, MENU_BLITZ, MENU_CHEESE, MENU_FORTY_LINES,
    NET_HANDSHAKE_TIMEOUT_SECS, NET_OVER_ITEMS, NetOutcome, PAUSE_ITEMS, SETTINGS_ITEMS, Screen,
    SettingItem, clamp_u64, format_time_mmss, menu_items,
};
use crate::application::gameplay::{
    SinglePlayerStep, advance_sim_state, advance_single_player, compute_net_outcome, frame_dt_ms,
};
use crate::application::model::AppState;
use crate::application::reducer::{
    GameOverIntent, JoinInputIntent, MenuIntent, NameEntryIntent, NetOverIntent, PauseIntent,
    SettingsIntent, UiKey, reduce_cancel_input, reduce_game_over_input, reduce_join_input,
    reduce_menu_input, reduce_name_entry_input, reduce_net_over_input, reduce_paused_input,
    reduce_settings_input, reduce_simple_back_input,
};
use crate::application::service::{build_pending_score, resolve_player_name};
use crate::application::session::{GameSessionState, NetSessionState};

struct GameSession {
    session: GameSessionState,
    input: InputState,
}

impl GameSession {
    fn new(config: &AppConfig, mode: GameMode) -> Self {
        let seed = random_seed();
        Self {
            session: GameSessionState::new(seed, mode),
            input: InputState::new(
                InputConfig {
                    das_ms: config.das_ms,
                    arr_ms: config.arr_ms,
                    dcd_ms: config.dcd_ms,
                },
                config.key_bindings,
            ),
        }
    }
}

impl Deref for GameSession {
    type Target = GameSessionState;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl DerefMut for GameSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

struct NetSession {
    session: NetSessionState,
    input: InputState,
    net: NetClient,
}

struct NetPending {
    client: NetClient,
    since: Instant,
}

impl NetSession {
    fn new(config: &AppConfig, seed: u64, net: NetClient) -> Self {
        let input_cfg = InputConfig {
            das_ms: config.das_ms,
            arr_ms: config.arr_ms,
            dcd_ms: config.dcd_ms,
        };
        let input = InputState::new(input_cfg, config.key_bindings);
        Self {
            session: NetSessionState::new(seed),
            input,
            net,
        }
    }
}

impl Deref for NetSession {
    type Target = NetSessionState;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl DerefMut for NetSession {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

struct App {
    storage: Storage,
    config: AppConfig,
    session: GameSession,
    net_lobby: Option<NetLobby>,
    net_host_pending: Option<NetPending>,
    net_pending: Option<NetClient>,
    net_session: Option<NetSession>,
    state: AppState,
    high_scores: Vec<HighScore>,
}

impl App {
    fn new(storage: Storage) -> io::Result<Self> {
        let config = match storage.load_config()? {
            Some(config) => config,
            None => {
                let config = AppConfig::default();
                let _ = storage.save_config(&config);
                config
            }
        };
        let session = GameSession::new(&config, GameMode::Endless);
        Ok(Self {
            storage,
            config,
            session,
            net_lobby: None,
            net_host_pending: None,
            net_pending: None,
            net_session: None,
            state: AppState::default(),
            high_scores: Vec::new(),
        })
    }
}

impl Deref for App {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for App {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

pub fn run() -> io::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;
    let _ = stdout.execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
            | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
    ));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let _cleanup = CleanupGuard;

    let storage = Storage::open_default()?;
    let mut app = App::new(storage)?;
    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let frame_duration = Duration::from_millis(app.config.frame_ms);
        if now.duration_since(last_tick) < frame_duration {
            std::thread::sleep(frame_duration.saturating_sub(now.duration_since(last_tick)));
            continue;
        }
        last_tick = now;

        let mut commands = Vec::new();
        while event::poll(Duration::from_millis(0))? {
            let ev = event::read()?;
            let App {
                storage,
                config,
                session,
                net_lobby,
                net_host_pending,
                net_pending,
                net_session,
                high_scores,
                state,
            } = &mut app;
            let AppState {
                screen,
                join_input,
                join_error,
                name_input,
                pending_score,
                blitz_duration_secs,
                forty_lines_target,
                cheese_target,
                exit,
            } = state;
            match screen {
                Screen::Playing => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                        && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
                    {
                        session.pause(now);
                        session.input.clear_motion();
                        *screen = Screen::Paused { selected: 0 };
                        continue;
                    }
                    session.input.handle_event(ev, now, &mut commands);
                }
                Screen::NetPlaying => {
                    if let Some(net) = net_session.as_mut() {
                        if let CrosstermEvent::Key(key) = ev
                            && matches!(key.kind, KeyEventKind::Press)
                            && matches!(key.code, KeyCode::Esc)
                        {
                            net.input.clear_motion();
                            *net_session = None;
                            *screen = Screen::Menu { selected: 0 };
                            continue;
                        }
                        net.input.handle_event(ev, now, &mut commands);
                    }
                }
                Screen::Menu { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        let intent = reduce_menu_input(
                            selected,
                            to_ui_key(key.code),
                            blitz_duration_secs,
                            forty_lines_target,
                            cheese_target,
                        );
                        match intent {
                            MenuIntent::None => {}
                            MenuIntent::StartMode(mode) => {
                                *net_lobby = None;
                                *net_host_pending = None;
                                *net_pending = None;
                                *net_session = None;
                                *session = GameSession::new(config, mode);
                                *screen = Screen::Playing;
                            }
                            MenuIntent::Host => {
                                *net_host_pending = None;
                                *net_pending = None;
                                *net_session = None;
                                *join_error = None;
                                match NetLobby::bind_any() {
                                    Ok(lobby) => {
                                        *net_lobby = Some(lobby);
                                        *screen = Screen::NetHostWait;
                                    }
                                    Err(err) => {
                                        *join_error = Some(format!("Host error: {}", err));
                                        *screen = Screen::NetJoinInput;
                                    }
                                }
                            }
                            MenuIntent::Join => {
                                *net_lobby = None;
                                *net_host_pending = None;
                                *net_pending = None;
                                *net_session = None;
                                join_input.clear();
                                *join_error = None;
                                *screen = Screen::NetJoinInput;
                            }
                            MenuIntent::Scores => {
                                *high_scores = storage.fetch_high_scores(10).unwrap_or_default();
                                *screen = Screen::HighScores;
                            }
                            MenuIntent::Settings => {
                                *screen = Screen::Settings {
                                    selected: 0,
                                    capture: None,
                                };
                            }
                            MenuIntent::Quit => *exit = true,
                        }
                    }
                }
                Screen::Settings { selected, capture } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if capture.is_some() && !matches!(key.kind, KeyEventKind::Press) {
                            continue;
                        }
                        if capture.is_none() && !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match reduce_settings_input(selected, capture, to_ui_key(key.code)) {
                            SettingsIntent::None => {}
                            SettingsIntent::AdjustTiming { item, direction } => {
                                adjust_setting(config, item, direction);
                                let _ = storage.save_config(config);
                            }
                            SettingsIntent::ApplyBinding { item } => {
                                apply_binding(config, item, normalize_keycode(key.code));
                                let _ = storage.save_config(config);
                            }
                            SettingsIntent::BackToMenu => {
                                *screen = Screen::Menu { selected: 0 };
                            }
                        }
                    }
                }
                Screen::GameOver { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match reduce_game_over_input(
                            selected,
                            to_ui_key(key.code),
                            GAME_OVER_ITEMS.len(),
                        ) {
                            GameOverIntent::None => {}
                            GameOverIntent::Retry => {
                                let mode = session.mode;
                                *session = GameSession::new(config, mode);
                                *screen = Screen::Playing;
                            }
                            GameOverIntent::Menu => {
                                *screen = Screen::Menu { selected: 0 };
                            }
                            GameOverIntent::Quit => *exit = true,
                        }
                    }
                }
                Screen::NameEntry => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                    {
                        match reduce_name_entry_input(name_input, to_ui_key(key.code), 12) {
                            NameEntryIntent::None => {}
                            NameEntryIntent::SaveDefault => {
                                if let Some(score) = pending_score.take() {
                                    let entry = HighScore {
                                        name: "Player".to_string(),
                                        score: score.score,
                                        lines: score.lines,
                                        time_secs: score.time_secs,
                                        mode: score.mode,
                                    };
                                    let _ = storage.insert_high_score(&entry);
                                    *high_scores =
                                        storage.fetch_high_scores(10).unwrap_or_default();
                                }
                                name_input.clear();
                                *screen = Screen::GameOver { selected: 0 };
                            }
                            NameEntryIntent::Save => {
                                if let Some(score) = pending_score.take() {
                                    let entry = HighScore {
                                        name: resolve_player_name(name_input),
                                        score: score.score,
                                        lines: score.lines,
                                        time_secs: score.time_secs,
                                        mode: score.mode,
                                    };
                                    let _ = storage.insert_high_score(&entry);
                                    *high_scores =
                                        storage.fetch_high_scores(10).unwrap_or_default();
                                }
                                name_input.clear();
                                *screen = Screen::GameOver { selected: 0 };
                            }
                        }
                    }
                }
                Screen::HighScores => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                        && reduce_simple_back_input(to_ui_key(key.code))
                    {
                        *screen = Screen::Menu { selected: 0 };
                    }
                }
                Screen::NetHostWait => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                        && reduce_cancel_input(to_ui_key(key.code))
                    {
                        *net_lobby = None;
                        *net_host_pending = None;
                        *screen = Screen::Menu { selected: 0 };
                    }
                }
                Screen::NetJoinInput => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                    {
                        match reduce_join_input(join_input, join_error, to_ui_key(key.code)) {
                            JoinInputIntent::None => {}
                            JoinInputIntent::BackToMenu => {
                                *screen = Screen::Menu { selected: 0 };
                            }
                            JoinInputIntent::Connect => {
                                match NetClient::connect(join_input.trim()) {
                                    Ok(mut client) => match client.send_ready() {
                                        Ok(()) => {
                                            *net_pending = Some(client);
                                            *screen = Screen::NetJoinWait;
                                        }
                                        Err(err) => {
                                            *join_error = Some(format!("Ready error: {}", err));
                                        }
                                    },
                                    Err(err) => {
                                        *join_error = Some(format!("Connect error: {}", err));
                                    }
                                }
                            }
                        }
                    }
                }
                Screen::NetJoinWait => {
                    if let CrosstermEvent::Key(key) = ev
                        && matches!(key.kind, KeyEventKind::Press)
                        && reduce_cancel_input(to_ui_key(key.code))
                    {
                        *net_pending = None;
                        *screen = Screen::Menu { selected: 0 };
                    }
                }
                Screen::NetOver { selected, .. } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match reduce_net_over_input(
                            selected,
                            to_ui_key(key.code),
                            NET_OVER_ITEMS.len(),
                        ) {
                            NetOverIntent::None => {}
                            NetOverIntent::Menu => {
                                *net_session = None;
                                *screen = Screen::Menu { selected: 0 };
                            }
                            NetOverIntent::Quit => *exit = true,
                        }
                    }
                }
                Screen::Paused { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        let ui_key = if matches!(key.kind, KeyEventKind::Repeat)
                            && matches!(
                                key.code,
                                KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Esc
                            ) {
                            UiKey::Other
                        } else {
                            to_ui_key(key.code)
                        };
                        match reduce_paused_input(selected, ui_key, PAUSE_ITEMS.len()) {
                            PauseIntent::None => {}
                            PauseIntent::Resume => {
                                session.resume(now);
                                *screen = Screen::Playing;
                            }
                            PauseIntent::Menu => {
                                *screen = Screen::Menu { selected: 0 };
                            }
                            PauseIntent::Quit => *exit = true,
                        }
                    }
                }
            }
            if app.exit {
                break;
            }
        }

        if app.exit {
            break;
        }

        if matches!(app.screen, Screen::Playing) {
            app.session.input.repeat_commands(now, &mut commands);
            if commands.iter().any(|c| matches!(c, Command::Quit)) {
                app.session.input.clear_motion();
                app.screen = Screen::Menu { selected: 0 };
                continue;
            }

            let dt_ms = frame_dt_ms(&mut app.session.last_frame, now, 50);

            let rules_cfg = RulesConfig {
                gravity_ms: app.config.gravity_ms,
                soft_drop_factor: app.config.soft_drop_factor,
                lock_delay_ms: app.config.lock_delay_ms,
                lock_reset_limit: app.config.lock_reset_limit,
            };

            let soft_drop_active = app.session.input.soft_drop_active();
            match advance_single_player(
                &mut app.session,
                now,
                rules_cfg,
                dt_ms,
                &commands,
                soft_drop_active,
            ) {
                SinglePlayerStep::Continue { .. } => {}
                SinglePlayerStep::Ended => {
                    app.session.input.clear_motion();
                    queue_name_entry(&mut app, now);
                }
            }
            if matches!(app.screen, Screen::NameEntry) {
                app.session.input.clear_motion();
            }
        }

        if matches!(app.screen, Screen::NetHostWait) {
            if app.net_host_pending.is_none()
                && let Some(lobby) = app.net_lobby.as_ref()
                && let Some(stream) = lobby.try_accept()?
            {
                let client = NetClient::from_stream(stream)?;
                app.net_host_pending = Some(NetPending { client, since: now });
            }

            let mut ready = false;
            let mut disconnected = false;
            let mut timed_out = false;
            if let Some(pending) = app.net_host_pending.as_mut() {
                while let Some(msg) = pending.client.try_recv() {
                    match msg {
                        NetMessage::Ready => {
                            ready = true;
                        }
                        NetMessage::Disconnect => {
                            disconnected = true;
                            break;
                        }
                        _ => {}
                    }
                }
                timed_out =
                    now.duration_since(pending.since).as_secs() >= NET_HANDSHAKE_TIMEOUT_SECS;
            }

            if disconnected || timed_out {
                app.net_host_pending = None;
            } else if ready && let Some(mut pending) = app.net_host_pending.take() {
                let seed = random_seed();
                let _ = pending.client.send_start(seed);
                app.net_session = Some(NetSession::new(&app.config, seed, pending.client));
                app.net_lobby = None;
                app.screen = Screen::NetPlaying;
            }
        }

        if matches!(app.screen, Screen::NetJoinWait) {
            let mut seed = None;
            let mut disconnected = false;
            if let Some(pending) = app.net_pending.as_mut() {
                while let Some(msg) = pending.try_recv() {
                    match msg {
                        NetMessage::Start(start_seed) => {
                            seed = Some(start_seed);
                        }
                        NetMessage::Disconnect => {
                            disconnected = true;
                            break;
                        }
                        _ => {}
                    }
                }
            }
            if disconnected {
                app.net_pending = None;
                app.join_error = Some("Disconnected before start".to_string());
                app.screen = Screen::NetJoinInput;
            } else if let Some(start_seed) = seed
                && let Some(pending) = app.net_pending.take()
            {
                app.net_session = Some(NetSession::new(&app.config, start_seed, pending));
                app.join_error = None;
                app.screen = Screen::NetPlaying;
            }
        }

        if matches!(app.screen, Screen::NetPlaying) {
            let Some(net_session) = app.net_session.as_mut() else {
                app.screen = Screen::Menu { selected: 0 };
                continue;
            };

            net_session.input.repeat_commands(now, &mut commands);

            if commands.iter().any(|c| matches!(c, Command::Quit)) {
                let _ = net_session.net.send_over();
                net_session.input.clear_motion();
                app.net_session = None;
                app.screen = Screen::Menu { selected: 0 };
                continue;
            }

            let dt_ms = frame_dt_ms(&mut net_session.last_frame, now, 50);

            let rules_cfg = RulesConfig {
                gravity_ms: app.config.gravity_ms,
                soft_drop_factor: app.config.soft_drop_factor,
                lock_delay_ms: app.config.lock_delay_ms,
                lock_reset_limit: app.config.lock_reset_limit,
            };

            let soft_drop_active = net_session.input.soft_drop_active();
            let events = advance_sim_state(
                &mut net_session.state,
                rules_cfg,
                dt_ms,
                &commands,
                soft_drop_active,
            );

            for event in &events {
                match event {
                    GameEvent::OutgoingGarbage(lines) => {
                        let _ = net_session.net.send_garbage(*lines);
                    }
                    GameEvent::GameOver => {
                        let _ = net_session.net.send_over();
                    }
                    _ => {}
                }
            }

            while let Some(msg) = net_session.net.try_recv() {
                match msg {
                    NetMessage::Garbage(lines) => {
                        net_session.state.receive_garbage(lines);
                    }
                    NetMessage::Over => {
                        net_session.remote_over = true;
                    }
                    NetMessage::Disconnect => {
                        net_session.remote_over = true;
                        net_session.remote_disconnect = true;
                    }
                    _ => {}
                }
            }

            if let Some(outcome) = compute_net_outcome(
                net_session.state.game_over,
                net_session.remote_over,
                net_session.remote_disconnect,
            ) {
                app.screen = Screen::NetOver {
                    selected: 0,
                    outcome,
                };
            }
        }

        terminal.draw(|f| match app.screen {
            Screen::Menu { selected } => {
                let items = menu_items(
                    app.blitz_duration_secs,
                    app.forty_lines_target,
                    app.cheese_target,
                );
                let footer = match selected {
                    MENU_BLITZ => "Enter: play   Left/Right: adjust time   Esc/Q: quit",
                    MENU_FORTY_LINES => "Enter: play   Left/Right: adjust lines   Esc/Q: quit",
                    MENU_CHEESE => "Enter: play   Left/Right: adjust lines   Esc/Q: quit",
                    _ => "Enter: select   Esc/Q: quit",
                };
                render_menu(
                    f,
                    "TETRIS MVP",
                    &items,
                    selected,
                    footer,
                    app.config.key_bindings,
                );
            }
            Screen::Settings { selected, capture } => {
                let items = settings_lines(&app.config);
                let footer = if let Some(item) = capture {
                    format!("Press a key for {} (Esc to cancel)", item.label())
                } else {
                    "Enter: rebind   Left/Right: adjust   Esc: back".to_string()
                };
                render_settings(f, "Settings", &items, selected, &footer);
            }
            Screen::Playing => {
                let (time_secs, time_label) = match app.session.mode {
                    GameMode::Endless => (app.session.elapsed_secs(now), "Time"),
                    GameMode::Blitz { .. } => {
                        (app.session.remaining_secs(now).unwrap_or(0), "Left")
                    }
                    GameMode::FortyLines { .. } => (app.session.elapsed_secs(now), "Time"),
                    GameMode::Cheese { .. } => (app.session.elapsed_secs(now), "Time"),
                };
                render_game(
                    f,
                    &app.session.state,
                    RenderContext {
                        time_secs,
                        time_label,
                        enhanced_input: app.session.input.enhanced_input_active(),
                    },
                );
            }
            Screen::GameOver { selected } => {
                let time_secs = match app.session.mode {
                    GameMode::Endless => app.session.elapsed_secs(now),
                    GameMode::Blitz { duration_secs } => duration_secs,
                    GameMode::FortyLines { .. } => app.session.elapsed_secs(now),
                    GameMode::Cheese { .. } => app.session.elapsed_secs(now),
                };
                let title = match app.session.mode {
                    GameMode::FortyLines { .. } => "40 LINES",
                    GameMode::Cheese { .. } => "CHEESE RACE",
                    _ => {
                        if app.session.time_up {
                            "TIME UP"
                        } else {
                            "GAME OVER"
                        }
                    }
                };
                render_game_over(
                    f,
                    &app.session.state,
                    RenderContext {
                        time_secs,
                        time_label: "Time",
                        enhanced_input: app.session.input.enhanced_input_active(),
                    },
                    selected,
                    title,
                );
            }
            Screen::NameEntry => {
                let mut lines = vec![
                    "Enter your name".to_string(),
                    format!("> {}", app.name_input),
                ];
                if let Some(score) = &app.pending_score {
                    lines.push(format!("Score {:06}", score.score));
                    lines.push(format!("Lines {:02}", score.lines));
                    lines.push(format!("Time {}", format_time_mmss(score.time_secs)));
                    lines.push(format!("Mode {}", score.mode));
                }
                render_info_screen(f, "SAVE SCORE", &lines, "Enter: save   Esc: default");
            }
            Screen::HighScores => {
                let lines = high_score_lines(&app.high_scores);
                render_info_screen(f, "HIGH SCORES", &lines, "Esc: back");
            }
            Screen::NetHostWait => {
                let port = app.net_lobby.as_ref().map(|l| l.port).unwrap_or(0);
                let status = if app.net_host_pending.is_some() {
                    "Client connected. Waiting for ready..."
                } else {
                    "Waiting for connection..."
                };
                let lines = vec![
                    status.to_string(),
                    format!("Listening on 0.0.0.0:{}", port),
                    "Connect using the host LAN IP".to_string(),
                ];
                render_info_screen(f, "HOST MULTIPLAYER", &lines, "Esc: cancel");
            }
            Screen::NetJoinInput => {
                let mut lines = vec!["Enter host:port".to_string(), app.join_input.clone()];
                if let Some(err) = &app.join_error {
                    lines.push(String::new());
                    lines.push(err.clone());
                }
                render_info_screen(f, "JOIN MULTIPLAYER", &lines, "Enter: connect   Esc: back");
            }
            Screen::NetJoinWait => {
                let lines = vec![
                    "Connecting...".to_string(),
                    "Waiting for host...".to_string(),
                ];
                render_info_screen(f, "JOIN MULTIPLAYER", &lines, "Esc: cancel");
            }
            Screen::NetPlaying => {
                if let Some(net_session) = &app.net_session {
                    let time_secs = net_session.elapsed_secs(now);
                    render_game(
                        f,
                        &net_session.state,
                        RenderContext {
                            time_secs,
                            time_label: "Time",
                            enhanced_input: net_session.input.enhanced_input_active(),
                        },
                    );
                }
            }
            Screen::NetOver { selected, outcome } => {
                if let Some(net_session) = &app.net_session {
                    let time_secs = net_session.elapsed_secs(now);
                    let title = match outcome {
                        NetOutcome::Win => "YOU WIN",
                        NetOutcome::Lose => "YOU LOSE",
                        NetOutcome::Draw => "DRAW",
                        NetOutcome::Disconnect => "DISCONNECTED",
                    };
                    render_game_over_custom(
                        f,
                        &net_session.state,
                        RenderContext {
                            time_secs,
                            time_label: "Time",
                            enhanced_input: net_session.input.enhanced_input_active(),
                        },
                        selected,
                        title,
                        &NET_OVER_ITEMS,
                    );
                }
            }
            Screen::Paused { selected } => {
                let (time_secs, time_label) = match app.session.mode {
                    GameMode::Endless => (app.session.elapsed_secs(now), "Time"),
                    GameMode::Blitz { .. } => {
                        (app.session.remaining_secs(now).unwrap_or(0), "Left")
                    }
                    GameMode::FortyLines { .. } => (app.session.elapsed_secs(now), "Time"),
                    GameMode::Cheese { .. } => (app.session.elapsed_secs(now), "Time"),
                };
                render_pause(
                    f,
                    &app.session.state,
                    RenderContext {
                        time_secs,
                        time_label,
                        enhanced_input: app.session.input.enhanced_input_active(),
                    },
                    selected,
                );
            }
        })?;
    }

    Ok(())
}

fn queue_name_entry(app: &mut App, now: Instant) {
    app.pending_score = Some(build_pending_score(&app.session, now));
    app.name_input.clear();
    app.screen = Screen::NameEntry;
}

fn settings_lines(config: &AppConfig) -> Vec<String> {
    let KeyBindings {
        move_left,
        move_right,
        soft_drop,
        rotate_left,
        rotate_right,
        rotate_180,
        hold,
        hard_drop,
        ..
    } = config.key_bindings;
    SETTINGS_ITEMS
        .iter()
        .map(|item| match item {
            SettingItem::Das => format!("{}: {}", item.label(), config.das_ms),
            SettingItem::Arr => format!("{}: {}", item.label(), config.arr_ms),
            SettingItem::Dcd => format!("{}: {}", item.label(), config.dcd_ms),
            SettingItem::MoveLeft => format!("{}: {}", item.label(), keycode_label(move_left)),
            SettingItem::MoveRight => format!("{}: {}", item.label(), keycode_label(move_right)),
            SettingItem::SoftDrop => format!("{}: {}", item.label(), keycode_label(soft_drop)),
            SettingItem::RotateLeft => format!("{}: {}", item.label(), keycode_label(rotate_left)),
            SettingItem::RotateRight => {
                format!("{}: {}", item.label(), keycode_label(rotate_right))
            }
            SettingItem::Rotate180 => format!("{}: {}", item.label(), keycode_label(rotate_180)),
            SettingItem::Hold => format!("{}: {}", item.label(), keycode_label(hold)),
            SettingItem::HardDrop => format!("{}: {}", item.label(), keycode_label(hard_drop)),
            SettingItem::Back => item.label().to_string(),
        })
        .collect()
}

fn high_score_lines(scores: &[HighScore]) -> Vec<String> {
    if scores.is_empty() {
        return vec!["No scores yet.".to_string()];
    }

    let mut lines = Vec::with_capacity(scores.len() + 1);
    lines.push(format!(
        "{:<3} {:<12} {:>6}  {}",
        "#", "Name", "Score", "Mode"
    ));
    for (idx, score) in scores.iter().enumerate() {
        let name = truncate_name(&score.name, 12);
        lines.push(format!(
            "{:>2}. {:<12} {:>6}  {}",
            idx + 1,
            name,
            score.score,
            score.mode
        ));
    }
    lines
}

fn truncate_name(name: &str, max_len: usize) -> String {
    let mut out = name.trim().to_string();
    if out.len() > max_len {
        out.truncate(max_len);
    }
    out
}

fn adjust_setting(config: &mut AppConfig, item: SettingItem, direction: i32) {
    let delta = direction as i64;
    match item {
        SettingItem::Das => config.das_ms = clamp_u64(config.das_ms, delta * 5, 0, 1000),
        SettingItem::Arr => config.arr_ms = clamp_u64(config.arr_ms, delta, 0, 200),
        SettingItem::Dcd => config.dcd_ms = clamp_u64(config.dcd_ms, delta * 5, 0, 500),
        _ => {}
    }
}

fn apply_binding(config: &mut AppConfig, item: SettingItem, code: KeyCode) {
    match item {
        SettingItem::MoveLeft => config.key_bindings.move_left = code,
        SettingItem::MoveRight => config.key_bindings.move_right = code,
        SettingItem::SoftDrop => config.key_bindings.soft_drop = code,
        SettingItem::RotateLeft => config.key_bindings.rotate_left = code,
        SettingItem::RotateRight => config.key_bindings.rotate_right = code,
        SettingItem::Rotate180 => config.key_bindings.rotate_180 = code,
        SettingItem::Hold => config.key_bindings.hold = code,
        SettingItem::HardDrop => config.key_bindings.hard_drop = code,
        _ => {}
    }
}

fn is_press_or_repeat(kind: KeyEventKind) -> bool {
    matches!(kind, KeyEventKind::Press | KeyEventKind::Repeat)
}

fn to_ui_key(code: KeyCode) -> UiKey {
    match code {
        KeyCode::Up => UiKey::Up,
        KeyCode::Down => UiKey::Down,
        KeyCode::Left => UiKey::Left,
        KeyCode::Right => UiKey::Right,
        KeyCode::Enter => UiKey::Enter,
        KeyCode::Esc => UiKey::Esc,
        KeyCode::Backspace => UiKey::Backspace,
        KeyCode::Char(c) => UiKey::Char(c),
        _ => UiKey::Other,
    }
}

fn normalize_keycode(code: KeyCode) -> KeyCode {
    match code {
        KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
        other => other,
    }
}

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
        let _ = disable_raw_mode();
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}
