use std::time::Instant;

use tetris_domain::command::Command;
use tetris_domain::events::Event;
use tetris_domain::rules::{RulesConfig, step};
use tetris_domain::state::GameState;
use tetris_domain::timing::clamp_dt_ms;

use crate::flow::NetOutcome;
use crate::session::GameSessionState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinglePlayerStep {
    Continue { events: Vec<Event> },
    Ended,
}

pub fn frame_dt_ms(last_frame: &mut Instant, now: Instant, max_dt_ms: u64) -> u64 {
    let dt = now.duration_since(*last_frame);
    *last_frame = now;
    clamp_dt_ms(dt.as_millis() as u64, max_dt_ms)
}

pub fn advance_single_player(
    session: &mut GameSessionState,
    now: Instant,
    cfg: RulesConfig,
    dt_ms: u64,
    commands: &[Command],
    soft_drop_active: bool,
) -> SinglePlayerStep {
    if session.apply_mode_end_if_reached(now).is_some() {
        return SinglePlayerStep::Ended;
    }

    let events = step(&mut session.state, cfg, dt_ms, commands, soft_drop_active);
    if events.iter().any(|e| matches!(e, Event::GameOver)) {
        session.game_over_time = Some(session.elapsed_secs(now));
        return SinglePlayerStep::Ended;
    }
    SinglePlayerStep::Continue { events }
}

pub fn advance_sim_state(
    state: &mut GameState,
    cfg: RulesConfig,
    dt_ms: u64,
    commands: &[Command],
    soft_drop_active: bool,
) -> Vec<Event> {
    step(state, cfg, dt_ms, commands, soft_drop_active)
}

pub fn compute_net_outcome(
    local_game_over: bool,
    remote_over: bool,
    remote_disconnect: bool,
) -> Option<NetOutcome> {
    if !local_game_over && !remote_over {
        return None;
    }
    let outcome = if remote_disconnect {
        NetOutcome::Disconnect
    } else if local_game_over && remote_over {
        NetOutcome::Draw
    } else if local_game_over {
        NetOutcome::Lose
    } else {
        NetOutcome::Win
    };
    Some(outcome)
}
