#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Moved,
    Rotated,
    Held,
    HardDropped,
    LinesCleared(u8),
    Locked,
    OutgoingGarbage(u32),
    GameOver,
}
