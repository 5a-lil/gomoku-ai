use crate::board::{Board, BOARD_SIZE, Cell, State};

pub struct Ai {
    board: [Cell; (BOARD_SIZE * BOARD_SIZE) as usize],
}

const START_INDEX: usize = 180;

impl Ai {
    const MIN_START: i32 = -92;
    const MAX_START: i32 = -92;

    pub fn new(to_copy: &Board) -> Self {
        Self { 
            board: to_copy._board_states.clone(),
        }
    }

    pub fn play(&mut self, last_played_index: usize) {
        self.minimax(last_played_index, 0, false);
    }

    pub fn minimax(&mut self, played_index: usize, depth: u8, maximizing: bool) -> usize {
        //game condition if won drow or somethin
        if depth == 10 {
            return played_index /* static evaluation */;
        }

        let mut best_score;

        if maximizing {
            best_score = Self::MIN_START;
            // boucle for pour placer le pion sur chaque case et lance le minimax recurs
            if played_index as i16 - BOARD_SIZE as i16 >= 0 {
                // place stone
                self.board[played_index - BOARD_SIZE as usize].state = State::Black;
                let score = self.minimax(0, depth + 1, !maximizing);
                // rollback the move
                self.board[played_index - BOARD_SIZE as usize].state = State::Empty;
                best_score = std::cmp::max(score as i32, best_score);
            }
            //
            best_score as usize
        }
        else {
            best_score = Self::MAX_START;
            // boucle for pour placer le pion sur chaque case et lance le minimax recurs
            if played_index as i16 - BOARD_SIZE as i16 >= 0 {
                // place stone
                self.board[played_index - BOARD_SIZE as usize].state = State::White;
                let score = self.minimax(0, depth + 1, !maximizing);
                // rollback the move
                self.board[played_index - BOARD_SIZE as usize].state = State::Empty;
                best_score = std::cmp::min(score as i32, best_score);
            }
            //
            best_score as usize
        }


    }
}