use crate::domain::board::PLAY_HEIGHT;

pub const MENU_ITEMS: [&str; 9] = [
    "New Game",
    "Blitz",
    "40 Lines",
    "Cheese Race",
    "Host Multiplayer",
    "Join Multiplayer",
    "High Scores",
    "Settings",
    "Quit",
];
pub const MENU_NEW_GAME: usize = 0;
pub const MENU_BLITZ: usize = 1;
pub const MENU_FORTY_LINES: usize = 2;
pub const MENU_CHEESE: usize = 3;
pub const MENU_HOST: usize = 4;
pub const MENU_JOIN: usize = 5;
pub const MENU_SCORES: usize = 6;
pub const MENU_SETTINGS: usize = 7;
pub const GAME_OVER_ITEMS: [&str; 3] = ["Retry", "Menu", "Quit"];
pub const NET_OVER_ITEMS: [&str; 2] = ["Menu", "Quit"];
pub const PAUSE_ITEMS: [&str; 3] = ["Resume", "Menu", "Quit"];
pub const BLITZ_DEFAULT_SECS: u64 = 120;
pub const BLITZ_MIN_SECS: u64 = 30;
pub const BLITZ_MAX_SECS: u64 = 600;
pub const BLITZ_STEP_SECS: u64 = 30;
pub const FORTY_LINES_DEFAULT_TARGET: u32 = 40;
pub const FORTY_LINES_MIN_TARGET: u32 = 10;
pub const FORTY_LINES_MAX_TARGET: u32 = 200;
pub const FORTY_LINES_STEP_TARGET: u32 = 10;
pub const CHEESE_DEFAULT_TARGET: u32 = 10;
pub const CHEESE_MIN_TARGET: u32 = 10;
pub const CHEESE_MAX_TARGET: u32 = PLAY_HEIGHT as u32 + 20;
pub const CHEESE_STEP_TARGET: u32 = 1;
pub const NET_HANDSHAKE_TIMEOUT_SECS: u64 = 10;

const BLITZ_LABEL_WIDTH: usize = 26;
const FORTY_LINES_LABEL_WIDTH: usize = 26;
const CHEESE_LABEL_WIDTH: usize = 26;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu {
        selected: usize,
    },
    Settings {
        selected: usize,
        capture: Option<SettingItem>,
    },
    Playing,
    Paused {
        selected: usize,
    },
    GameOver {
        selected: usize,
    },
    NameEntry,
    HighScores,
    NetHostWait,
    NetJoinInput,
    NetJoinWait,
    NetPlaying,
    NetOver {
        selected: usize,
        outcome: NetOutcome,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    Endless,
    Blitz { duration_secs: u64 },
    FortyLines { target: u32 },
    Cheese { target: u32 },
}

impl GameMode {
    pub fn time_limit_secs(self) -> Option<u64> {
        match self {
            GameMode::Endless => None,
            GameMode::Blitz { duration_secs } => Some(duration_secs),
            GameMode::FortyLines { .. } => None,
            GameMode::Cheese { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingItem {
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
pub enum NetOutcome {
    Win,
    Lose,
    Draw,
    Disconnect,
}

impl SettingItem {
    pub fn label(self) -> &'static str {
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

    pub fn is_binding(self) -> bool {
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

pub const SETTINGS_ITEMS: [SettingItem; 12] = [
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

pub fn format_time_mmss(duration_secs: u64) -> String {
    format!("{}:{:02}", duration_secs / 60, duration_secs % 60)
}

pub fn mode_label(mode: GameMode) -> String {
    match mode {
        GameMode::Endless => "Endless".to_string(),
        GameMode::Blitz { duration_secs } => format!("Blitz {}", format_time_mmss(duration_secs)),
        GameMode::FortyLines { target } => format!("40 Lines {}", target),
        GameMode::Cheese { target } => format!("Cheese {}", target),
    }
}

pub fn menu_items(
    blitz_duration_secs: u64,
    forty_lines_target: u32,
    cheese_target: u32,
) -> Vec<String> {
    MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(idx, label)| match idx {
            MENU_BLITZ => format_blitz_menu_item(blitz_duration_secs),
            MENU_FORTY_LINES => format_forty_lines_menu_item(forty_lines_target),
            MENU_CHEESE => format_cheese_menu_item(cheese_target),
            _ => (*label).to_string(),
        })
        .collect()
}

pub fn format_blitz_menu_item(duration_secs: u64) -> String {
    let time_label = format_time_mmss(duration_secs);
    format!(
        "{:<width$} {}",
        "Blitz",
        time_label,
        width = BLITZ_LABEL_WIDTH
    )
}

pub fn format_forty_lines_menu_item(target: u32) -> String {
    format!(
        "{:<width$} {}",
        "40 Lines",
        target,
        width = FORTY_LINES_LABEL_WIDTH
    )
}

pub fn format_cheese_menu_item(target: u32) -> String {
    format!(
        "{:<width$} {}",
        "Cheese Race",
        target,
        width = CHEESE_LABEL_WIDTH
    )
}

pub fn adjust_blitz_duration(duration_secs: u64, direction: i32) -> u64 {
    let delta = direction as i64 * BLITZ_STEP_SECS as i64;
    clamp_u64(duration_secs, delta, BLITZ_MIN_SECS, BLITZ_MAX_SECS)
}

pub fn adjust_forty_lines_target(target: u32, direction: i32) -> u32 {
    let delta = direction as i64 * FORTY_LINES_STEP_TARGET as i64;
    clamp_u64(
        target as u64,
        delta,
        FORTY_LINES_MIN_TARGET as u64,
        FORTY_LINES_MAX_TARGET as u64,
    ) as u32
}

pub fn adjust_cheese_target(target: u32, direction: i32) -> u32 {
    let delta = direction as i64 * CHEESE_STEP_TARGET as i64;
    clamp_u64(
        target as u64,
        delta,
        CHEESE_MIN_TARGET as u64,
        CHEESE_MAX_TARGET as u64,
    ) as u32
}

pub fn clamp_u64(value: u64, delta: i64, min: u64, max: u64) -> u64 {
    let mut v = value as i64 + delta;
    if v < min as i64 {
        v = min as i64;
    }
    if v > max as i64 {
        v = max as i64;
    }
    v as u64
}

pub fn move_selection(selected: &mut usize, len: usize, direction: i32) {
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
