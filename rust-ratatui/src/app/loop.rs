use std::io;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, Event as CrosstermEvent, KeyCode, KeyEventKind, KeyboardEnhancementFlags,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::config::AppConfig;
use crate::app::net::{NetClient, NetLobby, NetMessage};
use crate::core::command::Command;
use crate::core::events::Event as GameEvent;
use crate::core::rng::random_seed;
use crate::core::rules::{step, RulesConfig};
use crate::core::state::GameState;
use crate::core::timing::clamp_dt_ms;
use crate::core::board::PLAY_HEIGHT;
use crate::ui::input::{InputConfig, InputState, KeyBindings};
use crate::ui::render::{
    render_game, render_game_over, render_game_over_custom, render_info_screen, render_menu,
    render_pause, render_settings, RenderContext,
};

const MENU_ITEMS: [&str; 8] = [
    "New Game",
    "Blitz",
    "40 Lines",
    "Cheese Race",
    "Host Multiplayer",
    "Join Multiplayer",
    "Settings",
    "Quit",
];
const GAME_OVER_ITEMS: [&str; 3] = ["Retry", "Menu", "Quit"];
const NET_OVER_ITEMS: [&str; 2] = ["Menu", "Quit"];
const PAUSE_ITEMS: [&str; 3] = ["Resume", "Menu", "Quit"];
const BLITZ_DEFAULT_SECS: u64 = 120;
const BLITZ_MIN_SECS: u64 = 30;
const BLITZ_MAX_SECS: u64 = 600;
const BLITZ_STEP_SECS: u64 = 30;
const BLITZ_LABEL_WIDTH: usize = 26;
const FORTY_LINES_DEFAULT_TARGET: u32 = 40;
const FORTY_LINES_MIN_TARGET: u32 = 10;
const FORTY_LINES_MAX_TARGET: u32 = 200;
const FORTY_LINES_STEP_TARGET: u32 = 10;
const FORTY_LINES_LABEL_WIDTH: usize = 26;
const CHEESE_DEFAULT_TARGET: u32 = 10;
const CHEESE_MIN_TARGET: u32 = 10;
const CHEESE_MAX_TARGET: u32 = PLAY_HEIGHT as u32;
const CHEESE_STEP_TARGET: u32 = 1;
const CHEESE_LABEL_WIDTH: usize = 26;
const SETTINGS_ITEMS: [SettingItem; 12] = [
    SettingItem::Das,
    SettingItem::Arr,
    SettingItem::Dcd,
    SettingItem::MoveLeft,
    SettingItem::MoveRight,
    SettingItem::SoftDrop,
    SettingItem::RotateLeft,
    SettingItem::RotateRight,
    SettingItem::Rotate180,
    SettingItem::Hold,
    SettingItem::HardDrop,
    SettingItem::Back,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Menu { selected: usize },
    Settings { selected: usize, capture: Option<SettingItem> },
    Playing,
    Paused { selected: usize },
    GameOver { selected: usize },
    NetHostWait,
    NetJoinInput,
    NetJoinWait,
    NetPlaying,
    NetOver { selected: usize, outcome: NetOutcome },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GameMode {
    Endless,
    Blitz { duration_secs: u64 },
    FortyLines { target: u32 },
    Cheese { target: u32 },
}

impl GameMode {
    fn time_limit_secs(self) -> Option<u64> {
        match self {
            GameMode::Endless => None,
            GameMode::Blitz { duration_secs } => Some(duration_secs),
            GameMode::FortyLines { .. } => None,
            GameMode::Cheese { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingItem {
    Das,
    Arr,
    Dcd,
    MoveLeft,
    MoveRight,
    SoftDrop,
    RotateLeft,
    RotateRight,
    Rotate180,
    Hold,
    HardDrop,
    Back,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetOutcome {
    Win,
    Lose,
    Draw,
    Disconnect,
}

impl SettingItem {
    fn label(self) -> &'static str {
        match self {
            SettingItem::Das => "DAS (ms)",
            SettingItem::Arr => "ARR (ms)",
            SettingItem::Dcd => "DCD (ms)",
            SettingItem::MoveLeft => "Move Left",
            SettingItem::MoveRight => "Move Right",
            SettingItem::SoftDrop => "Soft Drop",
            SettingItem::RotateLeft => "Rotate Left",
            SettingItem::RotateRight => "Rotate Right",
            SettingItem::Rotate180 => "Rotate 180",
            SettingItem::Hold => "Hold",
            SettingItem::HardDrop => "Hard Drop",
            SettingItem::Back => "Back",
        }
    }

    fn is_binding(self) -> bool {
        matches!(
            self,
            SettingItem::MoveLeft
                | SettingItem::MoveRight
                | SettingItem::SoftDrop
                | SettingItem::RotateLeft
                | SettingItem::RotateRight
                | SettingItem::Rotate180
                | SettingItem::Hold
                | SettingItem::HardDrop
        )
    }
}

struct GameSession {
    mode: GameMode,
    state: GameState,
    input: InputState,
    start_time: Instant,
    last_frame: Instant,
    paused_at: Option<Instant>,
    paused_total: Duration,
    game_over_time: Option<u64>,
    time_up: bool,
}

impl GameSession {
    fn new(config: &AppConfig, mode: GameMode) -> Self {
        let now = Instant::now();
        let seed = random_seed();
        let mut state = GameState::new(seed);
        if let GameMode::Cheese { target } = mode {
            state.seed_cheese(target, seed);
        }
        Self {
            mode,
            state,
            input: InputState::new(
                InputConfig {
                    das_ms: config.das_ms,
                    arr_ms: config.arr_ms,
                    dcd_ms: config.dcd_ms,
                },
                config.key_bindings,
            ),
            start_time: now,
            last_frame: now,
            paused_at: None,
            paused_total: Duration::from_secs(0),
            game_over_time: None,
            time_up: false,
        }
    }

    fn elapsed_secs(&self, now: Instant) -> u64 {
        self.game_over_time.unwrap_or_else(|| {
            let effective_now = self.paused_at.unwrap_or(now);
            let elapsed = effective_now
                .duration_since(self.start_time)
                .saturating_sub(self.paused_total);
            elapsed.as_secs()
        })
    }

    fn remaining_secs(&self, now: Instant) -> Option<u64> {
        let limit = self.mode.time_limit_secs()?;
        let elapsed = self.elapsed_secs(now);
        Some(limit.saturating_sub(elapsed))
    }

    fn pause(&mut self, now: Instant) {
        if self.paused_at.is_none() {
            self.paused_at = Some(now);
        }
    }

    fn resume(&mut self, now: Instant) {
        if let Some(paused_at) = self.paused_at.take() {
            self.paused_total = self.paused_total.saturating_add(now.duration_since(paused_at));
            self.last_frame = now;
        }
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

struct App {
    config: AppConfig,
    screen: Screen,
    session: GameSession,
    net_lobby: Option<NetLobby>,
    net_pending: Option<NetClient>,
    net_session: Option<NetSession>,
    join_input: String,
    join_error: Option<String>,
    blitz_duration_secs: u64,
    forty_lines_target: u32,
    cheese_target: u32,
    exit: bool,
}

impl App {
    fn new() -> Self {
        let config = AppConfig::default();
        let session = GameSession::new(&config, GameMode::Endless);
        Self {
            config,
            screen: Screen::Menu { selected: 0 },
            session,
            net_lobby: None,
            net_pending: None,
            net_session: None,
            join_input: String::new(),
            join_error: None,
            blitz_duration_secs: BLITZ_DEFAULT_SECS,
            forty_lines_target: FORTY_LINES_DEFAULT_TARGET,
            cheese_target: CHEESE_DEFAULT_TARGET,
            exit: false,
        }
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

    let mut app = App::new();
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
                screen,
                config,
                session,
                net_lobby,
                net_pending,
                net_session,
                join_input,
                join_error,
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
                                if *selected == 1 {
                                    *blitz_duration_secs = adjust_blitz_duration(
                                        *blitz_duration_secs,
                                        -1,
                                    );
                                } else if *selected == 2 {
                                    *forty_lines_target = adjust_forty_lines_target(
                                        *forty_lines_target,
                                        -1,
                                    );
                                } else if *selected == 3 {
                                    *cheese_target =
                                        adjust_cheese_target(*cheese_target, -1);
                                }
                            }
                            KeyCode::Right => {
                                if *selected == 1 {
                                    *blitz_duration_secs = adjust_blitz_duration(
                                        *blitz_duration_secs,
                                        1,
                                    );
                                } else if *selected == 2 {
                                    *forty_lines_target = adjust_forty_lines_target(
                                        *forty_lines_target,
                                        1,
                                    );
                                } else if *selected == 3 {
                                    *cheese_target =
                                        adjust_cheese_target(*cheese_target, 1);
                                }
                            }
                            KeyCode::Enter => match *selected {
                                0 => {
                                    *net_lobby = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    *session = GameSession::new(config, GameMode::Endless);
                                    *screen = Screen::Playing;
                                }
                                1 => {
                                    *net_lobby = None;
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
                                2 => {
                                    *net_lobby = None;
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
                                3 => {
                                    *net_lobby = None;
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
                                4 => {
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
                                5 => {
                                    *net_lobby = None;
                                    *net_pending = None;
                                    *net_session = None;
                                    join_input.clear();
                                    *join_error = None;
                                    *screen = Screen::NetJoinInput;
                                }
                                6 => {
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
                            KeyCode::Left => adjust_setting(config, item, -1),
                            KeyCode::Right => adjust_setting(config, item, 1),
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
                Screen::NetHostWait => {
                    if let CrosstermEvent::Key(key) = ev {
                        if matches!(key.kind, KeyEventKind::Press)
                            && matches!(key.code, KeyCode::Esc)
                        {
                            *net_lobby = None;
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
                                KeyCode::Enter => {
                                    match NetClient::connect(join_input.trim()) {
                                        Ok(client) => {
                                            *net_pending = Some(client);
                                            *screen = Screen::NetJoinWait;
                                        }
                                        Err(err) => {
                                            *join_error = Some(format!("Connect error: {}", err));
                                        }
                                    }
                                }
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
                            && matches!(key.code, KeyCode::Char('p') | KeyCode::Char('P') | KeyCode::Esc)
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
                    app.screen = Screen::GameOver { selected: 0 };
                    continue;
                }
            }

            if let GameMode::FortyLines { target } = app.session.mode {
                if app.session.state.lines >= target {
                    app.session.game_over_time = Some(app.session.elapsed_secs(now));
                    app.session.time_up = true;
                    app.session.state.game_over = true;
                    app.session.input.clear_motion();
                    app.screen = Screen::GameOver { selected: 0 };
                    continue;
                }
            }

            if let GameMode::Cheese { .. } = app.session.mode {
                if app.session.state.garbage_remaining == 0 {
                    app.session.game_over_time = Some(app.session.elapsed_secs(now));
                    app.session.state.game_over = true;
                    app.session.input.clear_motion();
                    app.screen = Screen::GameOver { selected: 0 };
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

            let events = step(
                &mut app.session.state,
                rules_cfg,
                dt.as_millis() as u64,
                &commands,
                app.session.input.soft_drop_active(),
            );
            handle_events(&mut app.session.state, &events);
            if events.iter().any(|e| matches!(e, GameEvent::GameOver)) {
                app.session.game_over_time = Some(app.session.elapsed_secs(now));
                app.session.input.clear_motion();
                app.screen = Screen::GameOver { selected: 0 };
            }
        }

        if matches!(app.screen, Screen::NetHostWait) {
            if let Some(lobby) = app.net_lobby.as_ref() {
                if let Some(stream) = lobby.try_accept()? {
                    let mut client = NetClient::from_stream(stream)?;
                    let seed = random_seed();
                    let _ = client.send_start(seed);
                    app.net_session = Some(NetSession::new(&app.config, seed, client));
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

            net_session
                .input
                .repeat_commands(now, &mut commands);

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

            let events = step(
                &mut net_session.state,
                rules_cfg,
                dt.as_millis() as u64,
                &commands,
                net_session.input.soft_drop_active(),
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
                    1 => "Enter: play   Left/Right: adjust time   Esc/Q: quit",
                    2 => "Enter: play   Left/Right: adjust lines   Esc/Q: quit",
                    3 => "Enter: play   Left/Right: adjust lines   Esc/Q: quit",
                    _ => "Enter: select   Esc/Q: quit",
                };
                render_menu(
                    f,
                    "TETRIS MVP",
                    &items,
                    selected,
                    footer,
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
                    GameMode::Blitz { .. } => (app.session.remaining_secs(now).unwrap_or(0), "Left"),
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
            Screen::NetHostWait => {
                let port = app.net_lobby.as_ref().map(|l| l.port).unwrap_or(0);
                let lines = vec![
                    "Waiting for connection...".to_string(),
                    format!("Port: {}", port),
                    format!("Connect to 127.0.0.1:{}", port),
                ];
                render_info_screen(f, "HOST MULTIPLAYER", &lines, "Esc: cancel");
            }
            Screen::NetJoinInput => {
                let mut lines = vec!["Enter host:port".to_string(), app.join_input.clone()];
                if let Some(err) = &app.join_error {
                    lines.push(String::new());
                    lines.push(err.clone());
                }
                render_info_screen(
                    f,
                    "JOIN MULTIPLAYER",
                    &lines,
                    "Enter: connect   Esc: back",
                );
            }
            Screen::NetJoinWait => {
                let lines = vec!["Connecting...".to_string(), "Waiting for host...".to_string()];
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
                    GameMode::Blitz { .. } => (app.session.remaining_secs(now).unwrap_or(0), "Left"),
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
            SettingItem::RotateRight => format!("{}: {}", item.label(), keycode_label(rotate_right)),
            SettingItem::Rotate180 => format!("{}: {}", item.label(), keycode_label(rotate_180)),
            SettingItem::Hold => format!("{}: {}", item.label(), keycode_label(hold)),
            SettingItem::HardDrop => format!("{}: {}", item.label(), keycode_label(hard_drop)),
            SettingItem::Back => item.label().to_string(),
        })
        .collect()
}

fn menu_items(
    blitz_duration_secs: u64,
    forty_lines_target: u32,
    cheese_target: u32,
) -> Vec<String> {
    MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(idx, label)| match idx {
            1 => format_blitz_menu_item(blitz_duration_secs),
            2 => format_forty_lines_menu_item(forty_lines_target),
            3 => format_cheese_menu_item(cheese_target),
            _ => (*label).to_string(),
        })
        .collect()
}

fn format_blitz_menu_item(duration_secs: u64) -> String {
    let time_label = format_time_mmss(duration_secs);
    format!(
        "{:<width$} {}",
        "Blitz",
        time_label,
        width = BLITZ_LABEL_WIDTH
    )
}

fn format_time_mmss(duration_secs: u64) -> String {
    format!("{}:{:02}", duration_secs / 60, duration_secs % 60)
}

fn format_forty_lines_menu_item(target: u32) -> String {
    format!(
        "{:<width$} {}",
        "40 Lines",
        target,
        width = FORTY_LINES_LABEL_WIDTH
    )
}

fn format_cheese_menu_item(target: u32) -> String {
    format!(
        "{:<width$} {}",
        "Cheese Race",
        target,
        width = CHEESE_LABEL_WIDTH
    )
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

fn adjust_blitz_duration(duration_secs: u64, direction: i32) -> u64 {
    let delta = direction as i64 * BLITZ_STEP_SECS as i64;
    clamp_u64(duration_secs, delta, BLITZ_MIN_SECS, BLITZ_MAX_SECS)
}

fn adjust_forty_lines_target(target: u32, direction: i32) -> u32 {
    let delta = direction as i64 * FORTY_LINES_STEP_TARGET as i64;
    clamp_u64(
        target as u64,
        delta,
        FORTY_LINES_MIN_TARGET as u64,
        FORTY_LINES_MAX_TARGET as u64,
    ) as u32
}

fn adjust_cheese_target(target: u32, direction: i32) -> u32 {
    let delta = direction as i64 * CHEESE_STEP_TARGET as i64;
    clamp_u64(
        target as u64,
        delta,
        CHEESE_MIN_TARGET as u64,
        CHEESE_MAX_TARGET as u64,
    ) as u32
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

fn clamp_u64(value: u64, delta: i64, min: u64, max: u64) -> u64 {
    let mut v = value as i64 + delta;
    if v < min as i64 {
        v = min as i64;
    }
    if v > max as i64 {
        v = max as i64;
    }
    v as u64
}

fn move_selection(selected: &mut usize, len: usize, direction: i32) {
    if len == 0 {
        return;
    }
    let len_i = len as i32;
    let mut idx = *selected as i32 + direction;
    if idx < 0 {
        idx = len_i - 1;
    } else if idx >= len_i {
        idx = 0;
    }
    *selected = idx as usize;
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

fn keycode_label(code: KeyCode) -> String {
    match code {
        KeyCode::Left => "Left Arrow".to_string(),
        KeyCode::Right => "Right Arrow".to_string(),
        KeyCode::Up => "Up Arrow".to_string(),
        KeyCode::Down => "Down Arrow".to_string(),
        KeyCode::Char(' ') => "Space".to_string(),
        KeyCode::Char(c) => c.to_ascii_uppercase().to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Insert => "Insert".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "Page Up".to_string(),
        KeyCode::PageDown => "Page Down".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        other => format!("{:?}", other),
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
