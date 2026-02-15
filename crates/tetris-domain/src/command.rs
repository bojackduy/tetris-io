#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    MoveLeft,
    MoveRight,
    SoftDrop,
    HardDrop,
    RotateLeft,
    RotateRight,
    Rotate180,
    Hold,
    Quit,
}
