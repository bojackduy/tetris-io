use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::ops::{Deref, DerefMut};
use std::time::{Duration, Instant};

use crate::adapters::audio::{Audio, SoundCue};
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
    self, DisableFocusChange, EnableFocusChange, Event as CrosstermEvent, KeyCode, KeyEventKind,
    KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
    PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    BeginSynchronizedUpdate, EndSynchronizedUpdate, EnterAlternateScreen, LeaveAlternateScreen,
    disable_raw_mode, enable_raw_mode, supports_keyboard_enhancement,
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

struct TraceLogger {
    start: Instant,
    writer: BufWriter<File>,
}

impl TraceLogger {
    fn from_args() -> io::Result<Option<Self>> {
        let mut args = std::env::args().skip(1);
        let mut path = None;
        while let Some(arg) = args.next() {
            if arg == "--trace" {
                let next = args
                    .next()
                    .filter(|s| !s.starts_with('-'))
                    .unwrap_or_else(|| "tetris-trace.log".to_string());
                path = Some(next);
            } else if let Some(rest) = arg.strip_prefix("--trace=") {
                path = Some(rest.to_string());
            }
        }
        let Some(path) = path else {
            return Ok(None);
        };
        let file = File::create(path)?;
        Ok(Some(Self {
            start: Instant::now(),
            writer: BufWriter::new(file),
        }))
    }

    fn log(&mut self, msg: &str) {
        let ms = self.start.elapsed().as_millis();
        let _ = writeln!(self.writer, "+{ms:>7}ms {msg}");
        let _ = self.writer.flush();
    }
}

pub fn run() -> io::Result<()> {
    let mut trace = TraceLogger::from_args()?;
    let audio = Audio::new();
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _cleanup = CleanupGuard;
    stdout.execute(EnterAlternateScreen)?;
    let kitty_supported = supports_keyboard_enhancement().unwrap_or(false);
    if kitty_supported {
        stdout.execute(PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
        ))?;
    }
    stdout.execute(EnableFocusChange)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let storage = Storage::open_default()?;
    let mut app = App::new(storage)?;
    if let Some(logger) = trace.as_mut() {
        logger.log(&format!(
            "startup TERM={:?} TERM_PROGRAM={:?} TERM_PROGRAM_VERSION={:?} kitty_supported={} cfg(das={},arr={},dcd={},sd={},lock={},reset={})",
            std::env::var("TERM").ok(),
            std::env::var("TERM_PROGRAM").ok(),
            std::env::var("TERM_PROGRAM_VERSION").ok(),
            kitty_supported,
            app.config.das_ms,
            app.config.arr_ms,
            app.config.dcd_ms,
            app.config.soft_drop_factor,
            app.config.lock_delay_ms,
            app.config.lock_reset_limit,
        ));
    }

    // Never trust CSI ? u handshake alone: many terminals answer CSI ? u without
    // actually sending KeyEventKind::Release, which would leave held keys stuck.
    // Activate release-dependent DAS/ARR only after observing a real Release event
    // (for example, releasing Enter on the Main Menu).
    let mut observed_release = false;
    let mut last_render = Instant::now() - Duration::from_secs(1);

    loop {
        // Input and simulation run independently of the terminal's rendering rate.
        let _ = event::poll(Duration::from_millis(1))?;
        let now = Instant::now();
        if observed_release {
            app.session.input.set_enhanced_input(true);
            if let Some(net) = app.net_session.as_mut() {
                net.input.set_enhanced_input(true);
            }
        }

        let mut dirty = false;
        let mut commands = Vec::new();
        while event::poll(Duration::from_millis(0))? {
            let ev = event::read()?;
            if !matches!(
                ev,
                CrosstermEvent::Key(crossterm::event::KeyEvent {
                    kind: KeyEventKind::Release | KeyEventKind::Repeat,
                    ..
                })
            ) {
                dirty = true;
            }
            if let CrosstermEvent::Key(key) = ev
                && key.kind == KeyEventKind::Release
            {
                observed_release = true;
            }
            if let Some(logger) = trace.as_mut() {
                logger.log(&format!(
                    "ev={ev:?} observed_release={observed_release} enh={}",
                    app.session.input.enhanced_input_active()
                ));
            }
            if matches!(ev, CrosstermEvent::FocusLost) {
                app.session.input.clear_motion();
                if matches!(app.screen, Screen::Playing) {
                    app.session.pause(now);
                    app.screen = Screen::Paused { selected: 0 };
                }
                if let Some(net) = app.net_session.as_mut() {
                    net.input.clear_motion();
                }
                commands.clear();
                continue;
            }
            if let CrosstermEvent::Key(key) = ev
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c'))
            {
                app.exit = true;
                break;
            }
            if let CrosstermEvent::Key(key) = ev
                && key.kind == KeyEventKind::Press
                && key.code == KeyCode::F(4)
                && matches!(
                    app.screen,
                    Screen::Playing
                        | Screen::Paused { .. }
                        | Screen::GameOver { .. }
                        | Screen::NameEntry
                )
            {
                let mode = app.session.mode;
                app.session = GameSession::new(&app.config, mode);
                app.pending_score = None;
                app.screen = Screen::Playing;
                audio.play(SoundCue::Start, app.config.sound_volume);
                commands.clear();
                continue;
            }
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
                        && matches!(
                            key.code,
                            KeyCode::Esc | KeyCode::Char('p') | KeyCode::Char('P')
                        )
                    {
                        session.pause(now);
                        session.input.clear_motion();
                        *screen = Screen::Paused { selected: 0 };
                        continue;
                    }
                    if observed_release {
                        session.input.set_enhanced_input(true);
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
                        if observed_release {
                            net.input.set_enhanced_input(true);
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
                                audio.play(SoundCue::Start, config.sound_volume);
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
                                audio.play(SoundCue::Start, config.sound_volume);
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
            // Count manual finesse inputs before software ARR repeats are appended.
            let manual_finesse = count_finesse_inputs(&commands);
            if manual_finesse > 0 {
                app.session.state.add_finesse_inputs(manual_finesse);
            }
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
            let before_active = app.session.state.active;
            let before_hold = app.session.state.hold;
            let before_pieces = app.session.state.pieces_placed;
            if let Some(logger) = trace.as_mut()
                && !commands.is_empty()
            {
                logger.log(&format!(
                    "step dt={dt_ms}ms cmds={commands:?} sd={soft_drop_active} before=({:?},x={},y={},r={})",
                    app.session.state.active.kind,
                    app.session.state.active.x,
                    app.session.state.active.y,
                    app.session.state.active.rotation.0
                ));
            }
            match advance_single_player(
                &mut app.session,
                now,
                rules_cfg,
                dt_ms,
                &commands,
                soft_drop_active,
            ) {
                SinglePlayerStep::Continue { events } => {
                    if !events.is_empty()
                        || app.session.state.active != before_active
                        || app.session.state.hold != before_hold
                        || app.session.state.pieces_placed != before_pieces
                    {
                        dirty = true;
                    }
                    if let Some(logger) = trace.as_mut()
                        && (!commands.is_empty() || !events.is_empty())
                    {
                        logger.log(&format!(
                            "after active=({:?},x={},y={},r={}) events={events:?} pieces={}",
                            app.session.state.active.kind,
                            app.session.state.active.x,
                            app.session.state.active.y,
                            app.session.state.active.rotation.0,
                            app.session.state.pieces_placed
                        ));
                    }
                    play_events(&audio, &app.config, &events, &app.session.state);
                }
                SinglePlayerStep::Ended => {
                    dirty = true;
                    audio.play(
                        if app.session.time_up || !app.session.state.game_over {
                            SoundCue::PerfectClear
                        } else {
                            SoundCue::GameOver
                        },
                        app.config.sound_volume,
                    );
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

            let manual_finesse = count_finesse_inputs(&commands);
            if manual_finesse > 0 {
                net_session.state.add_finesse_inputs(manual_finesse);
            }
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
            let before_active = net_session.state.active;
            let events = advance_sim_state(
                &mut net_session.state,
                rules_cfg,
                dt_ms,
                &commands,
                soft_drop_active,
            );
            if !events.is_empty() || net_session.state.active != before_active {
                dirty = true;
            }
            play_events(&audio, &app.config, &events, &net_session.state);

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

        // Render immediately on any input or piece movement (matches Jstris's
        // synchronous redraw on move); throttle only idle clock ticks to frame_ms.
        if !dirty && now.duration_since(last_render) < Duration::from_millis(app.config.frame_ms) {
            continue;
        }
        last_render = now;
        terminal.backend_mut().execute(BeginSynchronizedUpdate)?;
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
                    format!(
                        "Enter: apply/rebind   ◂▸: adjust   Esc: back{}",
                        if audio.available() {
                            ""
                        } else {
                            "   Audio unavailable"
                        }
                    )
                };
                render_settings(f, "Settings", &items, selected, &footer);
            }
            Screen::Playing => {
                render_game(f, &app.session.state, single_render_ctx(&app, now));
            }
            Screen::GameOver { selected } => {
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
                let mut ctx = single_render_ctx(&app, now);
                if let GameMode::Blitz { duration_secs } = app.session.mode {
                    ctx.time_ms = duration_secs.saturating_mul(1000);
                    ctx.time_label = "Time";
                }
                render_game_over(f, &app.session.state, ctx, selected, title);
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
                    render_game(
                        f,
                        &net_session.state,
                        net_render_ctx(&app.config, net_session, now),
                    );
                }
            }
            Screen::NetOver { selected, outcome } => {
                if let Some(net_session) = &app.net_session {
                    let title = match outcome {
                        NetOutcome::Win => "YOU WIN",
                        NetOutcome::Lose => "YOU LOSE",
                        NetOutcome::Draw => "DRAW",
                        NetOutcome::Disconnect => "DISCONNECTED",
                    };
                    render_game_over_custom(
                        f,
                        &net_session.state,
                        net_render_ctx(&app.config, net_session, now),
                        selected,
                        title,
                        &NET_OVER_ITEMS,
                    );
                }
            }
            Screen::Paused { selected } => {
                render_pause(
                    f,
                    &app.session.state,
                    single_render_ctx(&app, now),
                    selected,
                );
            }
        })?;
        terminal.backend_mut().execute(EndSynchronizedUpdate)?;
    }

    Ok(())
}

fn queue_name_entry(app: &mut App, now: Instant) {
    app.pending_score = Some(build_pending_score(&app.session, now));
    app.name_input.clear();
    app.screen = Screen::NameEntry;
}

fn single_render_ctx(app: &App, now: Instant) -> RenderContext {
    let (time_ms, time_label, mode_label, lines_target) = match app.session.mode {
        GameMode::Endless => (app.session.elapsed_ms(now), "Time", "ENDLESS", None),
        GameMode::Blitz { .. } => (
            app.session.remaining_ms(now).unwrap_or(0),
            "Left",
            "BLITZ",
            None,
        ),
        GameMode::FortyLines { target } => (
            app.session.elapsed_ms(now),
            "Time",
            "40 LINES",
            Some(target),
        ),
        GameMode::Cheese { .. } => (app.session.elapsed_ms(now), "Time", "CHEESE", None),
    };
    RenderContext {
        time_ms,
        time_label,
        enhanced_input: app.session.input.enhanced_input_active(),
        ghost: app.config.ghost,
        grid_style: app.config.grid_style,
        key_presses: app.session.input.key_presses(),
        lock_delay_ms: app.config.lock_delay_ms,
        mode_label,
        lines_target,
    }
}

fn net_render_ctx(config: &AppConfig, net_session: &NetSession, now: Instant) -> RenderContext {
    RenderContext {
        time_ms: net_session.elapsed_ms(now),
        time_label: "Time",
        enhanced_input: net_session.input.enhanced_input_active(),
        ghost: config.ghost,
        grid_style: config.grid_style,
        key_presses: net_session.input.key_presses(),
        lock_delay_ms: config.lock_delay_ms,
        mode_label: "VERSUS",
        lines_target: None,
    }
}

fn count_finesse_inputs(commands: &[Command]) -> u32 {
    let mut total = 0u32;
    for cmd in commands {
        match cmd {
            Command::MoveLeft | Command::MoveRight | Command::RotateLeft | Command::RotateRight => {
                total = total.saturating_add(1);
            }
            // Verified in Jstris rotateCurrentBlock: 180 rotation adds +2 to finesse counter.
            Command::Rotate180 => {
                total = total.saturating_add(2);
            }
            _ => {}
        }
    }
    total
}

fn play_events(
    audio: &Audio,
    config: &AppConfig,
    events: &[GameEvent],
    state: &crate::domain::state::GameState,
) {
    for cue in event_sounds(
        events,
        state.combo,
        state.last_clear_was_pc,
        config.rotation_sound,
    ) {
        audio.play(cue, config.sound_volume);
    }
}

fn event_sounds(
    events: &[GameEvent],
    combo: u32,
    perfect_clear: bool,
    rotation_sound: bool,
) -> Vec<SoundCue> {
    let mut sounds = Vec::new();
    // Matches Jstris RainforestSFXset dispatch:
    // - rotation sound only when enabled (#SErot)
    // - harddrop aliases lock.mp3 once (do not double-trigger HardDrop + Lock)
    // - every line clear triggers the exact Rainforest comboTones.mp3 step (c0..c14)
    if events.contains(&GameEvent::Rotated) && rotation_sound {
        sounds.push(SoundCue::Rotate);
    }
    if events.contains(&GameEvent::HardDropped) {
        sounds.push(SoundCue::HardDrop);
    } else if events.contains(&GameEvent::Locked) {
        sounds.push(SoundCue::Lock);
    }
    if events
        .iter()
        .any(|event| matches!(event, GameEvent::LinesCleared(_)))
    {
        if combo > 0 {
            sounds.push(SoundCue::Combo(combo));
        }
        if perfect_clear {
            sounds.push(SoundCue::PerfectClear);
        }
    }
    if events.contains(&GameEvent::GameOver) {
        sounds.push(SoundCue::GameOver);
    }
    sounds
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
            SettingItem::SoftDropSpeed => format!("{}: {}", item.label(), config.soft_drop_label()),
            SettingItem::SoundVolume => format!("{}: {}", item.label(), config.sound_volume),
            SettingItem::RotationSound => format!(
                "{}: {}",
                item.label(),
                if config.rotation_sound { "On" } else { "Off" }
            ),
            SettingItem::Ghost => format!(
                "{}: {}",
                item.label(),
                if config.ghost { "On" } else { "Off" }
            ),
            SettingItem::Grid => format!(
                "{}: {}",
                item.label(),
                ["None", "Standard", "Partial", "Vertical", "Full"]
                    [usize::from(config.grid_style.min(4))]
            ),
            SettingItem::JstrisProfile => format!("{} (Enter)", item.label()),
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
        SettingItem::SoftDropSpeed => {
            let rates = [20, 60, 120, 180, 0];
            let current = rates
                .iter()
                .position(|rate| *rate == config.soft_drop_factor)
                .unwrap_or(2);
            config.soft_drop_factor =
                rates[(current as i32 + direction).rem_euclid(rates.len() as i32) as usize];
        }
        SettingItem::SoundVolume => {
            config.sound_volume = clamp_u64(u64::from(config.sound_volume), delta * 5, 0, 100) as u8
        }
        SettingItem::RotationSound => config.rotation_sound = !config.rotation_sound,
        SettingItem::Ghost => config.ghost = !config.ghost,
        SettingItem::Grid => {
            config.grid_style = (i32::from(config.grid_style) + direction).rem_euclid(5) as u8
        }
        SettingItem::JstrisProfile => config.apply_jstris_profile(),
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
        let _ = stdout.execute(EndSynchronizedUpdate);
        let _ = stdout.execute(DisableFocusChange);
        let _ = stdout.execute(PopKeyboardEnhancementFlags);
        let _ = disable_raw_mode();
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}
