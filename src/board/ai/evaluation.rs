use std::thread::Thread;

use ratatui::widgets::GraphType::Area;

use crate::board::ai::evaluation::Patterns::{Five, Four, NoCount, Three};
use crate::board::{Board, BOARD_SIZE, Cell, State, WIN_COND};
use crate::board::ai::{Ai, AREA};

enum Patterns {
    Five,
    Four,
    Three,
    NoCount,
}

impl Patterns {
    fn from(count: i64) -> Self {
        match count {
            5.. => Self::Five,
            4 => Self::Four,
            3 => Self::Three,
            _ => Self::NoCount,
        }
    }

    fn value(&self, playing: State) -> i64 {
        if playing == State::Empty {
            return 0;
        }

        let mut ret = match self {
            Five => {/*println!("MDRRR");*/ 9250000},
            Four => 40000,
            Three => 20000,
            NoCount => 0,
        };

        if playing == State::Black {
            ret *= -1
        }

        ret
    }
}

impl Ai {
    pub fn evaluation(&self) -> i64 {
        self.static_evaluation_horizontal() + self.static_evaluation_vertical() + self.static_evaluation_diagleft() + self.static_evaluation_diagright()
    }

    fn static_evaluation_horizontal(&self) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for (i, elem) in self.board.into_iter().enumerate() {
            if elem.state != current_state || i % BOARD_SIZE as usize == 0 {
                final_score += Patterns::from(count).value(current_state);
                current_state = elem.state;
                count = 0;
            }
            if current_state == State::Empty {
                continue;
            }
            count+=1;
        }
        final_score += Patterns::from(count).value(current_state);
        
        final_score
    }

    fn static_evaluation_vertical(&self) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 0..BOARD_SIZE {
            let mut i = i as usize;
            for _ in 0..BOARD_SIZE {
                let elem = self.board[i];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                if current_state == State::Empty {
                    continue;
                }
                count+=1;
                i += BOARD_SIZE as usize;
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        final_score += Patterns::from(count).value(current_state);
        
        final_score
    }

    fn static_evaluation_diagright(&self) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 0..BOARD_SIZE-1 {
            let mut i: i64 = i as i64;
            loop {
                let elem = self.board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + BOARD_SIZE as i64 - 1;
                if (i + 1) % BOARD_SIZE as i64 == 0 || i >= BOARD_SIZE as i64 * BOARD_SIZE as i64 {
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..BOARD_SIZE {
            let mut i: i64 = i as i64 + (BOARD_SIZE as i64 * (BOARD_SIZE as i64 - 1));
            loop {
                // println!("{i}");
                let elem = self.board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - BOARD_SIZE as i64 + 1;
                if i % BOARD_SIZE as i64 == 0 || i < 0 as i64 {
                    // println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }

    fn static_evaluation_diagleft(&self) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 1..BOARD_SIZE {
            let mut i: i64 = i as i64;
            loop {
                // println!("{i}");
                let elem = self.board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + BOARD_SIZE as i64 + 1;
                if i % BOARD_SIZE as i64 == 0 || i >= BOARD_SIZE as i64 * BOARD_SIZE as i64 {
                    // println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..BOARD_SIZE {
            let mut i: i64 = i as i64 + (BOARD_SIZE as i64 * (BOARD_SIZE as i64 - 1));
            loop {
                // println!("{i}");
                let elem = self.board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - BOARD_SIZE as i64 - 1;
                if (i + 1) % BOARD_SIZE as i64 == 0 || i < 0 as i64 {
                    // println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }

    pub fn test_evaluation(board: &Vec<Cell>, size: usize) -> i64 {
        Self::test_static_evaluation_horizontal(board, size) + Self::test_static_evaluation_vertical(board, size)
        + Self::test_static_evaluation_diagleft(board, size) + Self::test_static_evaluation_diagright(board, size)
    }

    fn test_static_evaluation_horizontal(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for (i, elem) in board.into_iter().enumerate() {
            if elem.state != current_state || i % size == 0 {
                final_score += Patterns::from(count).value(current_state);
                current_state = elem.state;
                count = 0;
            }
            if current_state == State::Empty {
                continue;
            }
            count+=1;
        }
        final_score += Patterns::from(count).value(current_state);
        
        final_score
    }

    fn test_static_evaluation_vertical(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 0..size {
            let mut i = i as usize;
            for _ in 0..size {
                let elem = board[i];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                i += size as usize;
                if current_state == State::Empty {
                    continue;
                }
                count+=1;
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        final_score += Patterns::from(count).value(current_state);
        
        final_score
    }

    fn test_static_evaluation_diagright(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 0..size-1 {
            let mut i: i64 = i as i64;
            loop {
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + size as i64 - 1;
                if (i + 1) % size as i64 == 0 || i >= size as i64 * size as i64 {
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..size {
            let mut i: i64 = i as i64 + (size as i64 * (size as i64 - 1));
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - size as i64 + 1;
                if i % size as i64 == 0 || i < 0 as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }

    fn test_static_evaluation_diagleft(board: &Vec<Cell>, size: usize) -> i64 {
        let mut final_score = 0;
        let mut count = 0;
        let mut current_state = State::Empty;
        for i in 1..size {
            let mut i: i64 = i as i64;
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i + size as i64 + 1;
                if i % size as i64 == 0 || i >= size as i64 * size as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }

        for i in 0..size {
            let mut i: i64 = i as i64 + (size as i64 * (size as i64 - 1));
            loop {
                println!("{i}");
                let elem = board[i as usize];
                if elem.state != current_state {
                    final_score += Patterns::from(count).value(current_state);
                    current_state = elem.state;
                    count = 0;
                }
                count+=1;
                i = i - size as i64 - 1;
                if (i + 1) % size as i64 == 0 || i < 0 as i64 {
                    println!("break");
                    break;
                }
            }
            final_score += Patterns::from(count).value(current_state);
            current_state = State::Empty;
            count = 0;
        }
        
        final_score
    }
}

#[cfg(test)]
mod tests {
    use crate::board::{Cell, ai::Ai, State};

    #[test]
    fn horizontal_empty_board() {
        let e = Cell::new();
        let board = vec![
            e, e, e,
            e, e, e,
            e, e, e,
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 3);
        assert_eq!(result, 0);
    }

    #[test]
    fn horizontal_equal() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, e, e,
            w, w, w,
            b, b, b,
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 3);
        assert_eq!(result, 0);
    }

    #[test]
    fn horizontal_b_0() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w,
            e, b, b,
            b, b, b,
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 3);
        assert_eq!(result, 0);
    }

    #[test]
    fn horizontal_w_0() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w,
            e, w, w,
            b, b, b,
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 3);
        assert_eq!(result, 0);
    }

    #[test]
    fn horizontal_calc_equal_60000() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w,
            w, w, w,
            b, e, e,
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 3);
        assert_eq!(result, 60000);
    }

    #[test]
    fn horizontal_quick_test() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, b,
            w, w, e, b, 
            b, e, e, w,
            b, b, w, w
        ];
        let result = Ai::test_static_evaluation_horizontal(&board, 4);
        assert_eq!(result, 30000);
    }

    #[test]
    fn vertical_first_test() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, b,
            w, w, e, b, 
            b, e, e, b,
            b, b, w, w
        ];
        let result = Ai::test_static_evaluation_vertical(&board, 4);
        assert_eq!(result, -30000);
    }

    #[test]
    fn vertical_other() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, e, b,
            w, w, w, b, 
            b, w, w, b,
            b, b, w, w
        ];
        let result = Ai::test_static_evaluation_vertical(&board, 4);
        assert_eq!(result, 30000);
    }

    #[test]
    fn diagright_base_test() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, w,
            w, w, w, b, 
            b, w, w, b,
            b, b, w, w
        ];
        let result = Ai::test_static_evaluation_diagright(&board, 4);
        assert_eq!(result, 30000);
    }

    #[test]
    fn diagright_other_equal() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, b, w, w, b,
            b, w, w, b, b,
            w, w, b, b, b,
            w, b, b, b, b,
            e, e, e, e, e,
        ];
        let result = Ai::test_static_evaluation_diagright(&board, 5);
        assert_eq!(result, 0);
    }

    #[test]
    fn diagright_other() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, b, w, w, b,
            b, w, w, b, b,
            w, w, b, b, b,
            w, b, b, b, b,
            b, b, e, e, e,
        ];
        let result = Ai::test_static_evaluation_diagright(&board, 5);
        assert_eq!(result, -170000);
    }

    #[test]
    fn diagleft_base() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, b, w, w, w,
            b, w, b, w, w,
            w, w, b, b, w,
            w, w, b, b, b,
            b, b, w, e, b,
        ];
        let result = Ai::test_static_evaluation_diagleft(&board, 5);
        assert_eq!(result, -10000);
    }

    #[test]
    fn diagleft_white_win() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, b, w, w, w,
            w, b, b, w, w,
            w, w, b, b, w,
            w, w, w, b, b,
            b, b, w, e, b,
        ];
        let result = Ai::test_static_evaluation_diagleft(&board, 5);
        assert_eq!(result, 10000);
    }

    #[test]
    fn diagleft_other() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, w, e, w, w,
            b, e, b, w, w,
            e, w, e, b, w,
            w, b, w, e, w,
            b, w, e, e, e,
        ];
        let result = Ai::test_static_evaluation_diagleft(&board, 5);
        assert_eq!(result, 0);
    }

    #[test]
    fn diagleft_other2() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, w, w, w, w,
            b, e, b, w, w,
            e, w, e, b, w,
            w, b, w, e, w,
            b, w, e, e, e,
        ];
        let result = Ai::test_static_evaluation_diagleft(&board, 5);
        assert_eq!(result, 30000);
    }

    #[test]
    fn all_other() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, b,
            w, w, w, b, 
            b, w, w, b,
            b, b, w, w
        ];
        let result = Ai::test_evaluation(&board, 4);
        assert_eq!(result, 170000);
    }

    #[test]
    fn all_without_four_white_but_four_black() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, b,
            w, w, w, b, 
            b, w, e, b,
            b, b, w, b
        ];
        let result = Ai::test_evaluation(&board, 4);
        assert_eq!(result, 80000);
    }

    #[test]
    fn all_in_game_case() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, w, w, e,
            b, b, b, b, 
            e, e, e, e,
            e, e, e, e
        ];
        let result = Ai::test_evaluation(&board, 4);
        assert_eq!(result, -10000);
    }

    #[test]
    fn all_260000() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            w, e, b, b, b,
            e, w, b, w, w,
            e, b, w, w, e,
            b, w, w, w, e,
            e, w, e, e, w,
        ];
        let result = Ai::test_evaluation(&board, 5);
        assert_eq!(result, 260000);
    }

    #[test]
    fn all_0() {
        let mut b = Cell::new();
        b.state = State::Black;
        let mut w = Cell::new();
        w.state = State::White;
        let e = Cell::new();
        let board = vec![
            e, e, e, w, e,
            b, w, e, w, b,
            b, e, w, b, e,
            b, w, b, w, e,
            e, e, e, e, e,
        ];
        let result = Ai::test_evaluation(&board, 5);
        assert_eq!(result, 0);
    }
}