use super::flow::{
    adjust_blitz_duration, adjust_cheese_target, adjust_forty_lines_target, move_selection,
    GameMode, MENU_BLITZ, MENU_CHEESE, MENU_FORTY_LINES, MENU_HOST, MENU_ITEMS, MENU_JOIN,
    MENU_NEW_GAME, MENU_SCORES, MENU_SETTINGS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiKey {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Char(char),
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuIntent {
    None,
    StartMode(GameMode),
    Host,
    Join,
    Scores,
    Settings,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseIntent {
    None,
    Resume,
    Menu,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameOverIntent {
    None,
    Retry,
    Menu,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetOverIntent {
    None,
    Menu,
    Quit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsIntent {
    None,
    AdjustTiming {
        item: super::flow::SettingItem,
        direction: i32,
    },
    ApplyBinding {
        item: super::flow::SettingItem,
    },
    BackToMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameEntryIntent {
    None,
    Save,
    SaveDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinInputIntent {
    None,
    BackToMenu,
    Connect,
}

pub fn reduce_menu_input(
    selected: &mut usize,
    key: UiKey,
    blitz_duration_secs: &mut u64,
    forty_lines_target: &mut u32,
    cheese_target: &mut u32,
) -> MenuIntent {
    match key {
        UiKey::Up => {
            move_selection(selected, MENU_ITEMS.len(), -1);
            MenuIntent::None
        }
        UiKey::Down => {
            move_selection(selected, MENU_ITEMS.len(), 1);
            MenuIntent::None
        }
        UiKey::Left => {
            if *selected == MENU_BLITZ {
                *blitz_duration_secs = adjust_blitz_duration(*blitz_duration_secs, -1);
            } else if *selected == MENU_FORTY_LINES {
                *forty_lines_target = adjust_forty_lines_target(*forty_lines_target, -1);
            } else if *selected == MENU_CHEESE {
                *cheese_target = adjust_cheese_target(*cheese_target, -1);
            }
            MenuIntent::None
        }
        UiKey::Right => {
            if *selected == MENU_BLITZ {
                *blitz_duration_secs = adjust_blitz_duration(*blitz_duration_secs, 1);
            } else if *selected == MENU_FORTY_LINES {
                *forty_lines_target = adjust_forty_lines_target(*forty_lines_target, 1);
            } else if *selected == MENU_CHEESE {
                *cheese_target = adjust_cheese_target(*cheese_target, 1);
            }
            MenuIntent::None
        }
        UiKey::Enter => match *selected {
            MENU_NEW_GAME => MenuIntent::StartMode(GameMode::Endless),
            MENU_BLITZ => MenuIntent::StartMode(GameMode::Blitz {
                duration_secs: *blitz_duration_secs,
            }),
            MENU_FORTY_LINES => MenuIntent::StartMode(GameMode::FortyLines {
                target: *forty_lines_target,
            }),
            MENU_CHEESE => MenuIntent::StartMode(GameMode::Cheese {
                target: *cheese_target,
            }),
            MENU_HOST => MenuIntent::Host,
            MENU_JOIN => MenuIntent::Join,
            MENU_SCORES => MenuIntent::Scores,
            MENU_SETTINGS => MenuIntent::Settings,
            _ => MenuIntent::Quit,
        },
        UiKey::Esc => MenuIntent::Quit,
        UiKey::Char('q') | UiKey::Char('Q') => MenuIntent::Quit,
        _ => MenuIntent::None,
    }
}

pub fn reduce_game_over_input(
    selected: &mut usize,
    key: UiKey,
    options_len: usize,
) -> GameOverIntent {
    match key {
        UiKey::Up => {
            move_selection(selected, options_len, -1);
            GameOverIntent::None
        }
        UiKey::Down => {
            move_selection(selected, options_len, 1);
            GameOverIntent::None
        }
        UiKey::Enter => match *selected {
            0 => GameOverIntent::Retry,
            1 => GameOverIntent::Menu,
            _ => GameOverIntent::Quit,
        },
        UiKey::Esc => GameOverIntent::Menu,
        _ => GameOverIntent::None,
    }
}

pub fn reduce_paused_input(selected: &mut usize, key: UiKey, options_len: usize) -> PauseIntent {
    match key {
        UiKey::Esc | UiKey::Char('p') | UiKey::Char('P') => PauseIntent::Resume,
        UiKey::Up => {
            move_selection(selected, options_len, -1);
            PauseIntent::None
        }
        UiKey::Down => {
            move_selection(selected, options_len, 1);
            PauseIntent::None
        }
        UiKey::Enter => match *selected {
            0 => PauseIntent::Resume,
            1 => PauseIntent::Menu,
            _ => PauseIntent::Quit,
        },
        _ => PauseIntent::None,
    }
}

pub fn reduce_net_over_input(
    selected: &mut usize,
    key: UiKey,
    options_len: usize,
) -> NetOverIntent {
    match key {
        UiKey::Up => {
            move_selection(selected, options_len, -1);
            NetOverIntent::None
        }
        UiKey::Down => {
            move_selection(selected, options_len, 1);
            NetOverIntent::None
        }
        UiKey::Enter => match *selected {
            0 => NetOverIntent::Menu,
            _ => NetOverIntent::Quit,
        },
        UiKey::Esc => NetOverIntent::Menu,
        _ => NetOverIntent::None,
    }
}

pub fn reduce_simple_back_input(key: UiKey) -> bool {
    matches!(key, UiKey::Esc | UiKey::Enter)
}

pub fn reduce_cancel_input(key: UiKey) -> bool {
    matches!(key, UiKey::Esc)
}

pub fn reduce_settings_input(
    selected: &mut usize,
    capture: &mut Option<super::flow::SettingItem>,
    key: UiKey,
) -> SettingsIntent {
    if let Some(item) = *capture {
        match key {
            UiKey::Esc => {
                *capture = None;
                SettingsIntent::None
            }
            _ => {
                *capture = None;
                SettingsIntent::ApplyBinding { item }
            }
        }
    } else {
        let item = super::flow::SETTINGS_ITEMS[*selected];
        match key {
            UiKey::Up => {
                move_selection(selected, super::flow::SETTINGS_ITEMS.len(), -1);
                SettingsIntent::None
            }
            UiKey::Down => {
                move_selection(selected, super::flow::SETTINGS_ITEMS.len(), 1);
                SettingsIntent::None
            }
            UiKey::Left => SettingsIntent::AdjustTiming {
                item,
                direction: -1,
            },
            UiKey::Right => SettingsIntent::AdjustTiming { item, direction: 1 },
            UiKey::Enter => {
                if item == super::flow::SettingItem::Back {
                    SettingsIntent::BackToMenu
                } else if item.is_binding() {
                    *capture = Some(item);
                    SettingsIntent::None
                } else {
                    SettingsIntent::None
                }
            }
            UiKey::Esc => SettingsIntent::BackToMenu,
            _ => SettingsIntent::None,
        }
    }
}

pub fn reduce_name_entry_input(
    name_input: &mut String,
    key: UiKey,
    max_len: usize,
) -> NameEntryIntent {
    match key {
        UiKey::Esc => NameEntryIntent::SaveDefault,
        UiKey::Enter => NameEntryIntent::Save,
        UiKey::Backspace => {
            name_input.pop();
            NameEntryIntent::None
        }
        UiKey::Char(c) => {
            if (c.is_ascii_graphic() || c == ' ') && name_input.len() < max_len {
                name_input.push(c);
            }
            NameEntryIntent::None
        }
        _ => NameEntryIntent::None,
    }
}

pub fn reduce_join_input(
    join_input: &mut String,
    join_error: &mut Option<String>,
    key: UiKey,
) -> JoinInputIntent {
    match key {
        UiKey::Esc => {
            *join_error = None;
            JoinInputIntent::BackToMenu
        }
        UiKey::Enter => JoinInputIntent::Connect,
        UiKey::Backspace => {
            join_input.pop();
            *join_error = None;
            JoinInputIntent::None
        }
        UiKey::Char(c) => {
            if c.is_ascii_graphic() || c == ' ' {
                join_input.push(c);
                *join_error = None;
            }
            JoinInputIntent::None
        }
        _ => JoinInputIntent::None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::flow::{
        SettingItem, BLITZ_DEFAULT_SECS, CHEESE_DEFAULT_TARGET, FORTY_LINES_DEFAULT_TARGET,
    };
    use super::*;

    #[test]
    fn menu_enter_new_game_starts_endless() {
        let mut selected = 0;
        let mut blitz = BLITZ_DEFAULT_SECS;
        let mut forty = FORTY_LINES_DEFAULT_TARGET;
        let mut cheese = CHEESE_DEFAULT_TARGET;
        let intent = reduce_menu_input(
            &mut selected,
            UiKey::Enter,
            &mut blitz,
            &mut forty,
            &mut cheese,
        );
        assert_eq!(intent, MenuIntent::StartMode(GameMode::Endless));
    }

    #[test]
    fn menu_adjusts_blitz_with_arrows() {
        let mut selected = MENU_BLITZ;
        let mut blitz = BLITZ_DEFAULT_SECS;
        let mut forty = FORTY_LINES_DEFAULT_TARGET;
        let mut cheese = CHEESE_DEFAULT_TARGET;
        let _ = reduce_menu_input(
            &mut selected,
            UiKey::Right,
            &mut blitz,
            &mut forty,
            &mut cheese,
        );
        assert!(blitz > BLITZ_DEFAULT_SECS);
    }

    #[test]
    fn menu_quit_keys_trigger_quit_intent() {
        let mut selected = 0;
        let mut blitz = BLITZ_DEFAULT_SECS;
        let mut forty = FORTY_LINES_DEFAULT_TARGET;
        let mut cheese = CHEESE_DEFAULT_TARGET;
        let intent_esc = reduce_menu_input(
            &mut selected,
            UiKey::Esc,
            &mut blitz,
            &mut forty,
            &mut cheese,
        );
        let intent_q = reduce_menu_input(
            &mut selected,
            UiKey::Char('q'),
            &mut blitz,
            &mut forty,
            &mut cheese,
        );
        assert_eq!(intent_esc, MenuIntent::Quit);
        assert_eq!(intent_q, MenuIntent::Quit);
    }

    #[test]
    fn pause_enter_resume_intent() {
        let mut selected = 0;
        let intent = reduce_paused_input(&mut selected, UiKey::Enter, 3);
        assert_eq!(intent, PauseIntent::Resume);
    }

    #[test]
    fn game_over_enter_menu_when_selected_second() {
        let mut selected = 1;
        let intent = reduce_game_over_input(&mut selected, UiKey::Enter, 3);
        assert_eq!(intent, GameOverIntent::Menu);
    }

    #[test]
    fn settings_enter_back_returns_back_intent() {
        let mut selected = super::super::flow::SETTINGS_ITEMS
            .iter()
            .position(|item| *item == SettingItem::Back)
            .unwrap_or(0);
        let mut capture = None;
        let intent = reduce_settings_input(&mut selected, &mut capture, UiKey::Enter);
        assert_eq!(intent, SettingsIntent::BackToMenu);
    }

    #[test]
    fn name_entry_backspace_edits_text() {
        let mut name = "Alex".to_string();
        let intent = reduce_name_entry_input(&mut name, UiKey::Backspace, 12);
        assert_eq!(intent, NameEntryIntent::None);
        assert_eq!(name, "Ale");
    }

    #[test]
    fn join_input_escape_returns_back() {
        let mut input = "127.0.0.1:4000".to_string();
        let mut err = Some("x".to_string());
        let intent = reduce_join_input(&mut input, &mut err, UiKey::Esc);
        assert_eq!(intent, JoinInputIntent::BackToMenu);
        assert_eq!(err, None);
    }
}
