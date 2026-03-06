use super::flow::GameMode;
use super::reducer::UiKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppAction {
    Key(UiKey),
    StartMode(GameMode),
    Tick,
    Pause,
    Resume,
    Quit,
}
