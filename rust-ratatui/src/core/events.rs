#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    LinesCleared(u8),
    Locked,
    GameOver,
}
