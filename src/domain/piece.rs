#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PieceType {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl PieceType {
    pub const ALL: [PieceType; 7] = [
        PieceType::I,
        PieceType::O,
        PieceType::T,
        PieceType::S,
        PieceType::Z,
        PieceType::J,
        PieceType::L,
    ];

    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation(pub u8);

impl Rotation {
    pub fn cw(self) -> Self {
        Rotation((self.0 + 1) % 4)
    }

    pub fn ccw(self) -> Self {
        Rotation((self.0 + 3) % 4)
    }

    pub fn half(self) -> Self {
        Rotation((self.0 + 2) % 4)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub kind: PieceType,
    pub rotation: Rotation,
    pub x: i32,
    pub y: i32,
}

impl Piece {
    pub fn new(kind: PieceType, x: i32, y: i32) -> Self {
        Self {
            kind,
            rotation: Rotation(0),
            x,
            y,
        }
    }

    pub fn blocks(self) -> [(i32, i32); 4] {
        let shape = SHAPES[self.kind.index()][self.rotation.0 as usize];
        [
            (self.x + shape[0].0, self.y + shape[0].1),
            (self.x + shape[1].0, self.y + shape[1].1),
            (self.x + shape[2].0, self.y + shape[2].1),
            (self.x + shape[3].0, self.y + shape[3].1),
        ]
    }
}

const SHAPES: [[[(i32, i32); 4]; 4]; 7] = [
    // I
    [
        [(0, 1), (1, 1), (2, 1), (3, 1)],
        [(2, 0), (2, 1), (2, 2), (2, 3)],
        [(0, 2), (1, 2), (2, 2), (3, 2)],
        [(1, 0), (1, 1), (1, 2), (1, 3)],
    ],
    // O
    [
        [(1, 0), (2, 0), (1, 1), (2, 1)],
        [(1, 0), (2, 0), (1, 1), (2, 1)],
        [(1, 0), (2, 0), (1, 1), (2, 1)],
        [(1, 0), (2, 0), (1, 1), (2, 1)],
    ],
    // T
    [
        [(1, 0), (0, 1), (1, 1), (2, 1)],
        [(1, 0), (1, 1), (2, 1), (1, 2)],
        [(0, 1), (1, 1), (2, 1), (1, 2)],
        [(1, 0), (0, 1), (1, 1), (1, 2)],
    ],
    // S
    [
        [(1, 0), (2, 0), (0, 1), (1, 1)],
        [(1, 0), (1, 1), (2, 1), (2, 2)],
        [(1, 1), (2, 1), (0, 2), (1, 2)],
        [(0, 0), (0, 1), (1, 1), (1, 2)],
    ],
    // Z
    [
        [(0, 0), (1, 0), (1, 1), (2, 1)],
        [(2, 0), (1, 1), (2, 1), (1, 2)],
        [(0, 1), (1, 1), (1, 2), (2, 2)],
        [(1, 0), (0, 1), (1, 1), (0, 2)],
    ],
    // J
    [
        [(0, 0), (0, 1), (1, 1), (2, 1)],
        [(1, 0), (2, 0), (1, 1), (1, 2)],
        [(0, 1), (1, 1), (2, 1), (2, 2)],
        [(1, 0), (1, 1), (0, 2), (1, 2)],
    ],
    // L
    [
        [(2, 0), (0, 1), (1, 1), (2, 1)],
        [(1, 0), (1, 1), (1, 2), (2, 2)],
        [(0, 1), (1, 1), (2, 1), (0, 2)],
        [(0, 0), (1, 0), (1, 1), (1, 2)],
    ],
];
