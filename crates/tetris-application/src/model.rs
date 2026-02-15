use crate::flow::{BLITZ_DEFAULT_SECS, CHEESE_DEFAULT_TARGET, FORTY_LINES_DEFAULT_TARGET, Screen};

#[derive(Debug, Clone)]
pub struct PendingScore {
    pub score: u32,
    pub lines: u32,
    pub time_secs: u64,
    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub screen: Screen,
    pub join_input: String,
    pub join_error: Option<String>,
    pub name_input: String,
    pub pending_score: Option<PendingScore>,
    pub blitz_duration_secs: u64,
    pub forty_lines_target: u32,
    pub cheese_target: u32,
    pub exit: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            screen: Screen::Menu { selected: 0 },
            join_input: String::new(),
            join_error: None,
            name_input: String::new(),
            pending_score: None,
            blitz_duration_secs: BLITZ_DEFAULT_SECS,
            forty_lines_target: FORTY_LINES_DEFAULT_TARGET,
            cheese_target: CHEESE_DEFAULT_TARGET,
            exit: false,
        }
    }
}
