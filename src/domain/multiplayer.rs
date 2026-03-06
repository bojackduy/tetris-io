use super::events::Event;
use super::rules::{RulesConfig, step};
use super::state::GameState;

#[derive(Debug)]
pub struct Arena {
    pub players: Vec<GameState>,
}

impl Arena {
    pub fn new(players: Vec<GameState>) -> Self {
        Self { players }
    }

    pub fn step_player(
        &mut self,
        idx: usize,
        cfg: RulesConfig,
        dt_ms: u64,
        commands: &[super::command::Command],
        soft_drop_active: bool,
    ) -> Vec<Event> {
        let events = step(
            &mut self.players[idx],
            cfg,
            dt_ms,
            commands,
            soft_drop_active,
        );
        let outgoing: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                Event::OutgoingGarbage(lines) => Some(*lines),
                _ => None,
            })
            .collect();
        if !outgoing.is_empty()
            && let Some(target) = self.pick_target(idx)
        {
            for lines in outgoing {
                self.players[target].receive_garbage(lines);
            }
        }
        events
    }

    fn pick_target(&self, sender_idx: usize) -> Option<usize> {
        let alive: Vec<usize> = self
            .players
            .iter()
            .enumerate()
            .filter(|(idx, state)| *idx != sender_idx && !state.game_over)
            .map(|(idx, _)| idx)
            .collect();
        alive.into_iter().next()
    }
}
