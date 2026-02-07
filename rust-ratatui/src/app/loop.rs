use std::io;
use std::time::{Duration, Instant};

use crossterm::event;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::config::AppConfig;
use crate::core::command::Command;
use crate::core::events::Event;
use crate::core::rng::random_seed;
use crate::core::rules::{step, RulesConfig};
use crate::core::state::GameState;
use crate::core::timing::clamp_dt_ms;
use crate::ui::input::{InputConfig, InputState};
use crate::ui::render::{render, RenderContext};

pub fn run() -> io::Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let _cleanup = CleanupGuard;

    let config = AppConfig::default();
    let rules_cfg = RulesConfig {
        gravity_ms: config.gravity_ms,
        soft_drop_factor: config.soft_drop_factor,
    };

    let mut input = InputState::new(InputConfig {
        das_ms: config.das_ms,
        arr_ms: config.arr_ms,
        dcd_ms: config.dcd_ms,
    });

    let mut state = GameState::new(random_seed());
    let mut last_frame = Instant::now();
    let start_time = Instant::now();

    loop {
        let now = Instant::now();
        let mut dt = now.duration_since(last_frame);
        if dt < Duration::from_millis(config.frame_ms) {
            std::thread::sleep(Duration::from_millis(config.frame_ms) - dt);
            continue;
        }
        last_frame = now;
        dt = Duration::from_millis(clamp_dt_ms(dt.as_millis() as u64, 50));

        let mut commands = Vec::new();
        while event::poll(Duration::from_millis(0))? {
            let ev = event::read()?;
            input.handle_event(ev, now, &mut commands);
        }
        input.repeat_commands(now, &mut commands);

        if commands.iter().any(|c| matches!(c, Command::Quit)) {
            break;
        }

        let events = step(
            &mut state,
            rules_cfg,
            dt.as_millis() as u64,
            &commands,
            input.soft_drop_active(),
        );
        handle_events(&mut state, &events);

        let elapsed_secs = start_time.elapsed().as_secs();
        terminal.draw(|f| {
            render(
                f,
                &state,
                RenderContext {
                    elapsed_secs,
                },
            )
        })?;
    }

    Ok(())
}

fn handle_events(state: &mut GameState, events: &[Event]) {
    for event in events {
        match event {
            Event::LinesCleared(_) => {
                // Scoring/level rules will be added later.
                let _ = state;
            }
            _ => {}
        }
    }
}

struct CleanupGuard;

impl Drop for CleanupGuard {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = disable_raw_mode();
        let _ = stdout.execute(LeaveAlternateScreen);
    }
}
