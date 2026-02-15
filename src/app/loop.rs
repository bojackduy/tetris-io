use std::io;
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

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
use crate::app::net::{NetClient, NetLobby, NetMessage};
use crate::app::storage::{HighScore, Storage};
use crate::core::command::Command;
use crate::core::events::Event as GameEvent;
use crate::core::rng::random_seed;
use crate::core::rules::{RulesConfig, step};
use crate::core::state::GameState;
use crate::core::timing::clamp_dt_ms;
use crate::ui::input::{InputConfig, InputState, KeyBindings, keycode_label};
use crate::ui::render::{
    RenderContext, render_game, render_game_over, render_game_over_custom, render_info_screen,
    render_menu, render_pause, render_settings,
};
use tetris_application::flow::{
    BLITZ_DEFAULT_SECS, CHEESE_DEFAULT_TARGET, FORTY_LINES_DEFAULT_TARGET, GAME_OVER_ITEMS,
    GameMode, MENU_BLITZ, MENU_CHEESE, MENU_FORTY_LINES, MENU_HOST, MENU_ITEMS, MENU_JOIN,
    MENU_NEW_GAME, MENU_SCORES, MENU_SETTINGS, NET_HANDSHAKE_TIMEOUT_SECS, NET_OVER_ITEMS,
    NetOutcome, PAUSE_ITEMS, SETTINGS_ITEMS, Screen, SettingItem, adjust_blitz_duration,
    adjust_cheese_target, adjust_forty_lines_target, clamp_u64, format_time_mmss, menu_items,
    mode_label, move_selection,
};
use tetris_application::session::GameSessionState;

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
    state: GameState,
    input: InputState,
    net: NetClient,
    start_time: Instant,
    last_frame: Instant,
    remote_over: bool,
    remote_disconnect: bool,
}

struct NetPending {
    client: NetClient,
    since: Instant,
}

impl NetSession {
    fn new(config: &AppConfig, seed: u64, net: NetClient) -> Self {
        let now = Instant::now();
        let input_cfg = InputConfig {
            das_ms: config.das_ms,
            arr_ms: config.arr_ms,
            dcd_ms: config.dcd_ms,
        };
        let input = InputState::new(input_cfg, config.key_bindings);
        Self {
            state: GameState::new(seed),
            input,
            net,
            start_time: now,
            last_frame: now,
            remote_over: false,
            remote_disconnect: false,
        }
    }

    fn elapsed_secs(&self, now: Instant) -> u64 {
        now.duration_since(self.start_time).as_secs()
    }
}

#[derive(Debug, Clone)]
struct PendingScore {
    score: u32,
    lines: u32,
    time_secs: u64,
    mode: String,
}

struct App {
    storage: Storage,
    config: AppConfig,
    screen: Screen,
    session: GameSession,
    net_lobby: Option<NetLobby>,
    net_host_pending: Option<NetPending>,
    net_pending: Option<NetClient>,
    net_session: Option<NetSession>,
    join_input: String,
    join_error: Option<String>,
    name_input: String,
    pending_score: Option<PendingScore>,
    high_scores: Vec<HighScore>,
    blitz_duration_secs: u64,
    forty_lines_target: u32,
    cheese_target: u32,
    exit: bool,
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
            screen: Screen::Menu { selected: 0 },
            session,
            net_lobby: None,
            net_host_pending: None,
            net_pending: None,
            net_session: None,
            join_input: String::new(),
            join_error: None,
            name_input: String::new(),
            pending_score: None,
            high_scores: Vec::new(),
            blitz_duration_secs: BLITZ_DEFAULT_SECS,
            forty_lines_target: FORTY_LINES_DEFAULT_TARGET,
            cheese_target: CHEESE_DEFAULT_TARGET,
            exit: false,
        })
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
                screen,
                config,
                session,
                net_lobby,
                net_host_pending,
                net_pending,
                net_session,
                join_input,
                join_error,
                name_input,
                pending_score,
                high_scores,
                blitz_duration_secs,
                forty_lines_target,
                cheese_target,
                exit,
            } = &mut app;
            match screen {
                Screen::Playing => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press)
                            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P'))
                        {
                            session.pause(now);
                            session.input.clear_motion();
                            *screen = Screen::Paused { selected: 0 };
                            continue;
                        }
                    }
                    session.input.handle_event(ev, now, &mut commands);
                }
                Screen::NetPlaying => {
                    if let Some(net) = net_session.as_mut() {
                        if let CrosstermEvent::Key(key) = ev {
                            if matches!(key.kind, KeyEventKind::Press)
                                && matches!(key.code, KeyCode::Esc)
                            {
                                net.input.clear_motion();
                                *net_session = None;
                                *screen = Screen::Menu { selected: 0 };
                                continue;
                            }
                        }
                        net.input.handle_event(ev, now, &mut commands);
                    }
                }
                Screen::Menu { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match key.code {
                            KeyCode::Up => move_selection(selected, MENU_ITEMS.len(), -1),
                            KeyCode::Down => move_selection(selected, MENU_ITEMS.len(), 1),
                            KeyCode::Left => {
                                if *selected == MENU_BLITZ {
                                    *blitz_duration_secs =
                                        adjust_blitz_duration(*blitz_duration_secs, -1);
                                } else if *selected == MENU_FORTY_LINES {
                                    *forty_lines_target =
                                        adjust_forty_lines_target(*forty_lines_target, -1);
                                } else if *selected == MENU_CHEESE {
                                    *cheese_target = adjust_cheese_target(*cheese_target, -1);
                                }
                            }
                            KeyCode::Right => {
                                if *selected == MENU_BLITZ {
                                    *blitz_duration_secs =
                                        adjust_blitz_duration(*blitz_duration_secs, 1);
                                } else if *selected == MENU_FORTY_LINES {
                                    *forty_lines_target =
                                        adjust_forty_lines_target(*forty_lines_target, 1);
                                } else if *selected == MENU_CHEESE {
                                    *cheese_target = adjust_cheese_target(*cheese_target, 1);
                                }
                            }
                            KeyCode::Enter => match *selected {
                                MENU_NEW_GAME => {
                                    *net_lobby = None;
                                    *net_host_pending = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    *session = GameSession::new(config, GameMode::Endless);
                                    *screen = Screen::Playing;
                                }
                                MENU_BLITZ => {
                                    *net_lobby = None;
                                    *net_host_pending = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    *session = GameSession::new(
                                        config,
                                        GameMode::Blitz {
                                            duration_secs: *blitz_duration_secs,
                                        },
                                    );
                                    *screen = Screen::Playing;
                                }
                                MENU_FORTY_LINES => {
                                    *net_lobby = None;
                                    *net_host_pending = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    *session = GameSession::new(
                                        config,
                                        GameMode::FortyLines {
                                            target: *forty_lines_target,
                                        },
                                    );
                                    *screen = Screen::Playing;
                                }
                                MENU_CHEESE => {
                                    *net_lobby = None;
                                    *net_host_pending = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    *session = GameSession::new(
                                        config,
                                        GameMode::Cheese {
                                            target: *cheese_target,
                                        },
                                    );
                                    *screen = Screen::Playing;
                                }
                                MENU_HOST => {
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
                                MENU_JOIN => {
                                    *net_lobby = None;
                                    *net_host_pending = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    join_input.clear();
                                    *join_error = None;
                                    *screen = Screen::NetJoinInput;
                                }
                                MENU_SCORES => {
                                    *high_scores =
                                        storage.fetch_high_scores(10).unwrap_or_default();
                                    *screen = Screen::HighScores;
                                }
                                MENU_SETTINGS => {
                                    *screen = Screen::Settings {
                                        selected: 0,
                                        capture: None,
                                    };
                                }
                                _ => *exit = true,
                            },
                            KeyCode::Esc => *exit = true,
                            KeyCode::Char('q') | KeyCode::Char('Q') => *exit = true,
                            _ => {}
                        }
                    }
                }
                Screen::Settings { selected, capture } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if let Some(item) = capture {
                            if matches!(key.kind, KeyEventKind::Press) {
                                if matches!(key.code, KeyCode::Esc) {
                                    *capture = None;
                                } else {
                                    apply_binding(config, *item, normalize_keycode(key.code));
                                    let _ = storage.save_config(config);
                                    *capture = None;
                                }
                            }
                            continue;
                        }

                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }

                        let item = SETTINGS_ITEMS[*selected];
                        match key.code {
                            KeyCode::Up => move_selection(selected, SETTINGS_ITEMS.len(), -1),
                            KeyCode::Down => move_selection(selected, SETTINGS_ITEMS.len(), 1),
                            KeyCode::Left => {
                                adjust_setting(config, item, -1);
                                let _ = storage.save_config(config);
                            }
                            KeyCode::Right => {
                                adjust_setting(config, item, 1);
                                let _ = storage.save_config(config);
                            }
                            KeyCode::Enter => {
                                if item == SettingItem::Back {
                                    *screen = Screen::Menu { selected: 0 };
                                } else if item.is_binding() {
                                    *capture = Some(item);
                                }
                            }
                            KeyCode::Esc => {
                                *screen = Screen::Menu { selected: 0 };
                            }
                            _ => {}
                        }
                    }
                }
                Screen::GameOver { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match key.code {
                            KeyCode::Up => move_selection(selected, GAME_OVER_ITEMS.len(), -1),
                            KeyCode::Down => move_selection(selected, GAME_OVER_ITEMS.len(), 1),
                            KeyCode::Enter => match *selected {
                                0 => {
                                    let mode = session.mode;
                                    *session = GameSession::new(config, mode);
                                    *screen = Screen::Playing;
                                }
                                1 => *screen = Screen::Menu { selected: 0 },
                                _ => *exit = true,
                            },
                            KeyCode::Esc => *screen = Screen::Menu { selected: 0 },
                            _ => {}
                        }
                    }
                }
                Screen::NameEntry => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press) {
                            match key.code {
                                KeyCode::Esc => {
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
                                KeyCode::Enter => {
                                    if let Some(score) = pending_score.take() {
                                        let name = name_input.trim();
                                        let resolved = if name.is_empty() {
                                            "Player".to_string()
                                        } else {
                                            name.to_string()
                                        };
                                        let entry = HighScore {
                                            name: resolved,
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
                                KeyCode::Backspace => {
                                    name_input.pop();
                                }
                                KeyCode::Char(c) => {
                                    if (c.is_ascii_graphic() || c == ' ') && name_input.len() < 12 {
                                        name_input.push(c);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Screen::HighScores => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press) {
                            match key.code {
                                KeyCode::Esc | KeyCode::Enter => {
                                    *screen = Screen::Menu { selected: 0 };
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Screen::NetHostWait => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press)
                            && matches!(key.code, KeyCode::Esc)
                        {
                            *net_lobby = None;
                            *net_host_pending = None;
                            *screen = Screen::Menu { selected: 0 };
                        }
                    }
                }
                Screen::NetJoinInput => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press) {
                            match key.code {
                                KeyCode::Esc => {
                                    *join_error = None;
                                    *screen = Screen::Menu { selected: 0 };
                                }
                                KeyCode::Enter => match NetClient::connect(join_input.trim()) {
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
                                },
                                KeyCode::Backspace => {
                                    join_input.pop();
                                    *join_error = None;
                                }
                                KeyCode::Char(c) => {
                                    if c.is_ascii_graphic() || c == ' ' {
                                        join_input.push(c);
                                        *join_error = None;
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Screen::NetJoinWait => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press)
                            && matches!(key.code, KeyCode::Esc)
                        {
                            *net_pending = None;
                            *screen = Screen::Menu { selected: 0 };
                        }
                    }
                }
                Screen::NetOver { selected, .. } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }
                        match key.code {
                            KeyCode::Up => move_selection(selected, NET_OVER_ITEMS.len(), -1),
                            KeyCode::Down => move_selection(selected, NET_OVER_ITEMS.len(), 1),
                            KeyCode::Enter => match *selected {
                                0 => {
                                    *net_session = None;
                                    *screen = Screen::Menu { selected: 0 };
                                }
                                _ => *exit = true,
                            },
                            KeyCode::Esc => {
                                *net_session = None;
                                *screen = Screen::Menu { selected: 0 };
                            }
                            _ => {}
                        }
                    }
                }
                Screen::Paused { selected } => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press)
                            && matches!(
                                key.code,
                                KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Esc
                            )
                        {
                            session.resume(now);
                            *screen = Screen::Playing;
                            continue;
                        }

                        if !is_press_or_repeat(key.kind) {
                            continue;
                        }

                        match key.code {
                            KeyCode::Up => move_selection(selected, PAUSE_ITEMS.len(), -1),
                            KeyCode::Down => move_selection(selected, PAUSE_ITEMS.len(), 1),
                            KeyCode::Enter => match *selected {
                                0 => {
                                    session.resume(now);
                                    *screen = Screen::Playing;
                                }
                                1 => *screen = Screen::Menu { selected: 0 },
                                _ => *exit = true,
                            },
                            _ => {}
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

            if let Some(limit) = app.session.mode.time_limit_secs() {
                if app.session.elapsed_secs(now) >= limit {
                    app.session.game_over_time = Some(limit);
                    app.session.time_up = true;
                    app.session.state.game_over = true;
                    app.session.input.clear_motion();
                    queue_name_entry(&mut app, now);
                    continue;
                }
            }

            if let GameMode::FortyLines { target } = app.session.mode {
                if app.session.state.lines >= target {
                    app.session.game_over_time = Some(app.session.elapsed_secs(now));
                    app.session.time_up = true;
                    app.session.state.game_over = true;
                    app.session.input.clear_motion();
                    queue_name_entry(&mut app, now);
                    continue;
                }
            }

            if let GameMode::Cheese { .. } = app.session.mode {
                if app.session.state.garbage_remaining == 0 {
                    app.session.game_over_time = Some(app.session.elapsed_secs(now));
                    app.session.state.game_over = true;
                    app.session.input.clear_motion();
                    queue_name_entry(&mut app, now);
                    continue;
                }
            }

            let mut dt = now.duration_since(app.session.last_frame);
            app.session.last_frame = now;
            dt = Duration::from_millis(clamp_dt_ms(dt.as_millis() as u64, 50));

            let rules_cfg = RulesConfig {
                gravity_ms: app.config.gravity_ms,
                soft_drop_factor: app.config.soft_drop_factor,
                lock_delay_ms: app.config.lock_delay_ms,
                lock_reset_limit: app.config.lock_reset_limit,
            };

            let soft_drop_active = app.session.input.soft_drop_active();
            let events = step(
                &mut app.session.state,
                rules_cfg,
                dt.as_millis() as u64,
                &commands,
                soft_drop_active,
            );
            handle_events(&mut app.session.state, &events);
            if events.iter().any(|e| matches!(e, GameEvent::GameOver)) {
                app.session.game_over_time = Some(app.session.elapsed_secs(now));
                app.session.input.clear_motion();
                queue_name_entry(&mut app, now);
            }
        }

        if matches!(app.screen, Screen::NetHostWait) {
            if app.net_host_pending.is_none() {
                if let Some(lobby) = app.net_lobby.as_ref() {
                    if let Some(stream) = lobby.try_accept()? {
                        let client = NetClient::from_stream(stream)?;
                        app.net_host_pending = Some(NetPending { client, since: now });
                    }
                }
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
            } else if ready {
                if let Some(mut pending) = app.net_host_pending.take() {
                    let seed = random_seed();
                    let _ = pending.client.send_start(seed);
                    app.net_session = Some(NetSession::new(&app.config, seed, pending.client));
                    app.net_lobby = None;
                    app.screen = Screen::NetPlaying;
                }
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
            } else if let Some(start_seed) = seed {
                if let Some(pending) = app.net_pending.take() {
                    app.net_session = Some(NetSession::new(&app.config, start_seed, pending));
                    app.join_error = None;
                    app.screen = Screen::NetPlaying;
                }
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

            let mut dt = now.duration_since(net_session.last_frame);
            net_session.last_frame = now;
            dt = Duration::from_millis(clamp_dt_ms(dt.as_millis() as u64, 50));

            let rules_cfg = RulesConfig {
                gravity_ms: app.config.gravity_ms,
                soft_drop_factor: app.config.soft_drop_factor,
                lock_delay_ms: app.config.lock_delay_ms,
                lock_reset_limit: app.config.lock_reset_limit,
            };

            let soft_drop_active = net_session.input.soft_drop_active();
            let events = step(
                &mut net_session.state,
                rules_cfg,
                dt.as_millis() as u64,
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

            if net_session.state.game_over || net_session.remote_over {
                let outcome = if net_session.remote_disconnect {
                    NetOutcome::Disconnect
                } else if net_session.state.game_over && net_session.remote_over {
                    NetOutcome::Draw
                } else if net_session.state.game_over {
                    NetOutcome::Lose
                } else {
                    NetOutcome::Win
                };
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

fn handle_events(state: &mut GameState, events: &[GameEvent]) {
    for event in events {
        match event {
            GameEvent::LinesCleared(_) => {
                let _ = state;
            }
            _ => {}
        }
    }
}

fn queue_name_entry(app: &mut App, now: Instant) {
    let time_secs = app
        .session
        .game_over_time
        .unwrap_or_else(|| app.session.elapsed_secs(now));
    let pending = PendingScore {
        score: app.session.state.score,
        lines: app.session.state.lines,
        time_secs,
        mode: mode_label(app.session.mode),
    };
    app.pending_score = Some(pending);
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
        SettingItem::Arr => config.arr_ms = clamp_u64(config.arr_ms, delta * 1, 0, 200),
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
