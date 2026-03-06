use super::piece::PieceType;

pub const BOARD_WIDTH: usize = 10;
pub const PLAY_HEIGHT: usize = 20;
pub const BUFFER_HEIGHT: usize = 10;
pub const BOARD_HEIGHT: usize = PLAY_HEIGHT + BUFFER_HEIGHT;
pub const VISIBLE_HEIGHT: usize = BOARD_HEIGHT;
pub const VISIBLE_START_Y: usize = 0;

pub type Cell = Option<PieceType>;

#[derive(Debug, Clone)]
pub struct Board {
    cells: [[Cell; BOARD_WIDTH]; BOARD_HEIGHT],
    garbage: [[bool; BOARD_WIDTH]; BOARD_HEIGHT],
}

impl Default for Board {
    fn default() -> Self {
        Self {
            cells: [[None; BOARD_WIDTH]; BOARD_HEIGHT],
            garbage: [[false; BOARD_WIDTH]; BOARD_HEIGHT],
        }
    }
}

impl Board {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, x: i32, y: i32) -> Cell {
        if !self.in_bounds(x, y) {
            return None;
        }
        self.cells[y as usize][x as usize]
    }

    pub fn set(&mut self, x: i32, y: i32, cell: Cell) {
        if !self.in_bounds(x, y) {
            return;
        }
        let yi = y as usize;
        let xi = x as usize;
        self.cells[yi][xi] = cell;
        self.garbage[yi][xi] = false;
    }

    pub fn set_garbage(&mut self, x: i32, y: i32, cell: Cell) {
        if !self.in_bounds(x, y) {
            return;
        }
        let yi = y as usize;
        let xi = x as usize;
        self.cells[yi][xi] = cell;
        self.garbage[yi][xi] = cell.is_some();
    }

    pub fn in_bounds(&self, x: i32, y: i32) -> bool {
        x >= 0 && x < BOARD_WIDTH as i32 && y >= 0 && y < BOARD_HEIGHT as i32
    }

    pub fn clear_full_rows(&mut self) -> usize {
        let mut cleared = 0usize;
        let mut dst = BOARD_HEIGHT as i32 - 1;
        for src in (0..BOARD_HEIGHT as i32).rev() {
            let full = self.is_row_full(src as usize);
            if full {
                cleared += 1;
            } else {
                if dst != src {
                    self.cells[dst as usize] = self.cells[src as usize];
                    self.garbage[dst as usize] = self.garbage[src as usize];
                }
                dst -= 1;
            }
        }
        while dst >= 0 {
            self.cells[dst as usize] = [None; BOARD_WIDTH];
            self.garbage[dst as usize] = [false; BOARD_WIDTH];
            dst -= 1;
        }
        cleared
    }

    pub fn insert_garbage(&mut self, lines: u32, holes: &[usize]) -> bool {
        let lines = lines.min(BOARD_HEIGHT as u32);
        if lines == 0 {
            return false;
        }
        let l = lines as usize;

        let mut top_out = false;
        for y in 0..l {
            if self.cells[y].iter().any(|c| c.is_some()) {
                top_out = true;
                break;
            }
        }

        let mut next = [[None; BOARD_WIDTH]; BOARD_HEIGHT];
        let mut next_garbage = [[false; BOARD_WIDTH]; BOARD_HEIGHT];
        next[..(BOARD_HEIGHT - l)].copy_from_slice(&self.cells[l..(BOARD_HEIGHT - l) + l]);
        next_garbage[..(BOARD_HEIGHT - l)]
            .copy_from_slice(&self.garbage[l..(BOARD_HEIGHT - l) + l]);
        for i in 0..l {
            let row = BOARD_HEIGHT - l + i;
            let hole = holes.get(i).copied().unwrap_or(0) % BOARD_WIDTH;
            let mut new_row = [None; BOARD_WIDTH];
            let mut new_garbage = [false; BOARD_WIDTH];
            for x in 0..BOARD_WIDTH {
                if x != hole {
                    new_row[x] = Some(PieceType::O);
                    new_garbage[x] = true;
                }
            }
            next[row] = new_row;
            next_garbage[row] = new_garbage;
        }

        self.cells = next;
        self.garbage = next_garbage;
        top_out
    }

    fn is_row_full(&self, row: usize) -> bool {
        self.cells[row].iter().all(|c| c.is_some())
    }

    pub fn cells(&self) -> &[[Cell; BOARD_WIDTH]; BOARD_HEIGHT] {
        &self.cells
    }

    pub fn is_garbage_cell(&self, x: i32, y: i32) -> bool {
        if !self.in_bounds(x, y) {
            return false;
        }
        self.garbage[y as usize][x as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_rows_shifts_down() {
        let mut board = Board::new();
        for x in 0..BOARD_WIDTH as i32 {
            board.set(x, (BOARD_HEIGHT - 1) as i32, Some(PieceType::I));
        }
        board.set(0, (BOARD_HEIGHT - 2) as i32, Some(PieceType::O));
        let cleared = board.clear_full_rows();
        assert_eq!(cleared, 1);
        assert_eq!(board.get(0, (BOARD_HEIGHT - 1) as i32), Some(PieceType::O));
    }

    #[test]
    fn insert_garbage_shifts_up_and_fills() {
        let mut board = Board::new();
        board.set(0, (BOARD_HEIGHT - 1) as i32, Some(PieceType::I));
        let top_out = board.insert_garbage(1, &[0]);
        assert!(!top_out);
        assert_eq!(board.get(0, (BOARD_HEIGHT - 2) as i32), Some(PieceType::I));
        assert_eq!(board.get(0, (BOARD_HEIGHT - 1) as i32), None);
        assert_eq!(board.get(1, (BOARD_HEIGHT - 1) as i32), Some(PieceType::O));
    }

    #[test]
    fn insert_garbage_top_out_when_blocks_pushed_off() {
        let mut board = Board::new();
        board.set(0, 0, Some(PieceType::I));
        let top_out = board.insert_garbage(1, &[0]);
        assert!(top_out);
    }
}
