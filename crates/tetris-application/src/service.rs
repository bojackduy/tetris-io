use std::time::Instant;

use crate::flow::mode_label;
use crate::model::PendingScore;
use crate::session::GameSessionState;

pub fn build_pending_score(session: &GameSessionState, now: Instant) -> PendingScore {
    let time_secs = session
        .game_over_time
        .unwrap_or_else(|| session.elapsed_secs(now));
    PendingScore {
        score: session.state.score,
        lines: session.state.lines,
        time_secs,
        mode: mode_label(session.mode),
    }
}

pub fn resolve_player_name(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        "Player".to_string()
    } else {
        trimmed.to_string()
    }
}
