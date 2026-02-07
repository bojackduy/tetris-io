use crate::core::piece::PieceType;

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
}

impl Board {
    pub fn new() -> Self {
        Self {
            cells: [[None; BOARD_WIDTH]; BOARD_HEIGHT],
        }
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
        self.cells[y as usize][x as usize] = cell;
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
                }
                dst -= 1;
            }
        }
        while dst >= 0 {
            self.cells[dst as usize] = [None; BOARD_WIDTH];
            dst -= 1;
        }
        cleared
    }

    fn is_row_full(&self, row: usize) -> bool {
        self.cells[row].iter().all(|c| c.is_some())
    }

    pub fn cells(&self) -> &[[Cell; BOARD_WIDTH]; BOARD_HEIGHT] {
        &self.cells
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
}
