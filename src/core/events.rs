#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    LinesCleared(u8),
    Locked,
    OutgoingGarbage(u32),
    GameOver,
}
