
use crate::board::{Board, BOARD_SIZE, Cell, State, WIN_COND};
mod evaluation;

fn check_up(ind: usize) -> Result<usize, ()> {
    if (ind as i16 - BOARD_SIZE as i16) < 0 {
        return Err(())
    }

    Ok(ind - BOARD_SIZE as usize)
}

fn check_down(ind: usize) -> Result<usize, ()> {
    if (ind as u16 + BOARD_SIZE) >= BOARD_SIZE * BOARD_SIZE {
        return Err(())
    }

    Ok(ind + BOARD_SIZE as usize)
}

fn check_right(ind: usize) -> Result<usize, ()> {
    if (ind + 1) % BOARD_SIZE as usize == 0 {
        return Err(())
    }

    Ok(ind + 1)
}

fn check_left(ind: usize) -> Result<usize, ()> {
    if ind % BOARD_SIZE as usize == 0 {
        return Err(())
    }

    Ok(ind - 1)
}

fn check_down_right(ind: usize) -> Result<usize, ()> {
    if (ind + 1) % BOARD_SIZE as usize == 0  || (ind as i16 + BOARD_SIZE as i16) >= AREA as i16 {
        return Err(())
    }

    Ok(ind + BOARD_SIZE as usize + 1)
}

fn check_down_left(ind: usize) -> Result<usize, ()> {
    if ind % BOARD_SIZE as usize == 0  || (ind as i16 + BOARD_SIZE as i16) >= AREA as i16 {
        return Err(())
    }

    Ok(ind + BOARD_SIZE as usize - 1)
}

fn check_up_right(ind: usize) -> Result<usize, ()> {
    if (ind + 1) % BOARD_SIZE as usize == 0  || (ind as i16 - BOARD_SIZE as i16) < 0 as i16 {
        return Err(())
    }

    Ok(ind - BOARD_SIZE as usize + 1)
}

fn check_up_left(ind: usize) -> Result<usize, ()> {
    if ind % BOARD_SIZE as usize == 0  || (ind as i16 - BOARD_SIZE as i16) < 0 as i16 {
        return Err(())
    }

    Ok(ind - BOARD_SIZE as usize - 1)
}

pub struct Ai {
    board: [Cell; (BOARD_SIZE * BOARD_SIZE) as usize],
    pub num_iters: u32,
    checks: [fn(usize) -> Result<usize, ()>; 8],
    pub best_index: usize,
}

const START_INDEX: usize = 180;
const AREA: u16 = BOARD_SIZE * BOARD_SIZE;

impl Ai {
    const MIN_START: i64 = i64::MIN;
    const MAX_START: i64 = i64::MAX;
    const NO_INDEX: i16 = 925;
    const ALPHA_START: i64 = i64::MIN;
    const BETA_START: i64 = i64::MAX;

    pub fn new(to_copy: &Board) -> Self {
        Self { 
            board: to_copy._board_states.clone(),
            num_iters: 0,
            checks: [
                check_up,
                check_down,
                check_left,
                check_right,
                check_down_right,
                check_down_left,
                check_up_right,
                check_up_left,
                // FAIRE LES DIAGS
            ],
            best_index: 0,
        }
    }

    pub fn play(&mut self, last_played_index: usize) -> i64 {
        let e = self.minimax(Self::NO_INDEX, last_played_index, 0, true, Self::ALPHA_START, Self::BETA_START);
        // println!("wth {}", e);
        e
    }

    pub fn minimax(&mut self, last_played_index: i16, played_index: usize, depth: i64, maximizing: bool, mut alpha: i64, mut beta: i64) -> i64 {
        //game condition if won drow or somethin
        self.num_iters += 1;
        if depth == 4 || self.win_with_lines(played_index) {
            return self.evaluation() /* static evaluation */;
        }

        let mut best_score;
        let mut best_index = 0;

        if maximizing {
            best_score = Self::MIN_START;
            // boucle for pour placer le pion sur chaque case et lance le minimax recurs
            for check in self.checks.into_iter() {
                if let Ok(index) = check(played_index) {
                    if !self.board[index].playable(State::White) {
                        continue;
                    }
                    // place stone
                    self.board[index].state = State::White;
                    let score = self.minimax(played_index as i16, index, depth + 1, false, alpha, beta);
                    // rollback the move 
                    self.board[index].state = State::Empty;
                    if score > best_score {
                        best_index = index;
                        best_score = score;
                    }

                    alpha = std::cmp::max(alpha, score);
                    if beta <= alpha {
                        break;
                    }
                }
            }

            if last_played_index != Self::NO_INDEX {
            for check in self.checks.into_iter() {
                if let Ok(index) = check(last_played_index as usize) {
                    if !self.board[index].playable(State::White) {
                        continue;
                    }
                    // place stone
                    self.board[index].state = State::White;
                    let score = self.minimax(Self::NO_INDEX, index, depth + 1, false, alpha, beta);
                    // rollback the move 
                    self.board[index].state = State::Empty;
                    if score > best_score {
                        best_index = index;
                        best_score = score;
                    }

                    alpha = std::cmp::max(alpha, score);
                    if beta <= alpha {
                        break;
                    }
                }
            }
            }
        }
        else {
            best_score = Self::MAX_START;
            // boucle for pour placer le pion sur chaque case et lance le minimax recurs

            for check in self.checks.into_iter() {
                if let Ok(index) = check(played_index) {
                    if !self.board[index].playable(State::Black) {
                        continue;
                    }
                    // place stone
                    self.board[index].state = State::Black;
                    let score = self.minimax(played_index as i16, index, depth + 1, true, alpha, beta);
                    // rollback the move
                    self.board[index].state = State::Empty;
                    if score < best_score {
                        best_index = index;
                        best_score = score;
                    }

                    beta = std::cmp::min(beta, score);
                    if beta <= alpha {
                        break;
                    }
                }
            }

            if last_played_index != Self::NO_INDEX {
            for check in self.checks.into_iter() {
                if let Ok(index) = check(last_played_index as usize) {
                    if !self.board[index].playable(State::Black) {
                        continue;
                    }
                    // place stone
                    self.board[index].state = State::Black;
                    let score = self.minimax(Self::NO_INDEX, index, depth + 1, true, alpha, beta);
                    // rollback the move
                    self.board[index].state = State::Empty;
                    if score < best_score {
                        best_index = index;
                        best_score = score;
                    }

                    beta = std::cmp::min(beta, score);
                    if beta <= alpha {
                        break;
                    }
                }
            }
            }
        }
        self.best_index = best_index;
        best_score
    }

    fn win_with_lines(&mut self, played_index: usize) -> bool {
        let played: State = self.board[played_index].state;

        // horizontal
        let mut hor_count: i16 = 0;
        {
            let mut left: i16 = played_index as i16;
            while self.board[left as usize].state == played {
                hor_count += 1;
                left -= 1;
                if (left + 1) % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            let mut right: i16 = played_index as i16;
            while self.board[right as usize].state == played {
                hor_count += 1;
                right += 1;
                if right % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            if hor_count - 1 >= WIN_COND {
                // println!("LINE WIN !");
                return true;
            }
        }

        // vertical
        let mut ver_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self.board[up as usize].state == played {
                ver_count += 1;
                up -= BOARD_SIZE as i16;
                if up < 0 {
                    break
                }
            }

            let mut down: usize = played_index;
            while self.board[down].state == played {
                ver_count += 1;
                down += BOARD_SIZE as usize;
                if down as u16 >= BOARD_SIZE * BOARD_SIZE {
                    break
                }
            }

            if ver_count - 1 >= WIN_COND {
                // println!("LINE WIN !");
                return true;
            }
        }

        // up left going diags
        let mut up_left_diag_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self.board[up as usize].state == played {
                up_left_diag_count += 1;
                let futur_move = up as i16 - BOARD_SIZE as i16 - 1;
                up = futur_move as i16;
                if up < 0 || (up + 1) % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            let mut down: i16 = played_index as i16;
            while self.board[down as usize].state == played {
                up_left_diag_count += 1;
                let futur_move = down as u16 + BOARD_SIZE + 1;
                down = futur_move as i16;
                if down >= AREA as i16 || down % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            if up_left_diag_count - 1 >= WIN_COND {
                // println!("LINE WIN !");
                return true;
            }
        }

        // // up right going diags
        let mut up_right_diag_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self.board[up as usize].state == played {
                up_right_diag_count += 1;
                let futur_move = up as i16 - BOARD_SIZE as i16 + 1;
                up = futur_move as i16;
                if up < 0 || up % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            let mut down: i16 = played_index as i16;
            while self.board[down as usize].state == played {
                up_right_diag_count += 1;
                let futur_move = down as u16 + BOARD_SIZE - 1;
                down = futur_move as i16;
                if down >= AREA as i16 || (down + 1) % BOARD_SIZE as i16 == 0 {
                    break
                }
            }

            if up_right_diag_count - 1 >= WIN_COND {
                // println!("LINE WIN !");
                return true;
            }
        }
        false
    }

    // fn static_evaluation(&self, played_index: usize) -> i64 {
    //     let mut final_score: i64 = 10;
    //     let played: State = self.board[played_index].state;

    //     // horizontal
    //     let mut hor_count: i16 = 0;
    //     {
    //         let mut left: i16 = played_index as i16;
    //         while self.board[left as usize].state == played {
    //             hor_count += 1;
    //             left -= 1;
    //             if (left + 1) % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }

    //         let mut right: i16 = played_index as i16;
    //         while self.board[right as usize].state == played {
    //             hor_count += 1;
    //             right += 1;
    //             if right % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }
    //         final_score += hor_count as i64 - 2
    //     }

    //     // vertical
    //     let mut ver_count: i16 = 0;
    //     {
    //         let mut up: i16 = played_index as i16;
    //         while self.board[up as usize].state == played {
    //             ver_count += 1;
    //             up -= BOARD_SIZE as i16;
    //             if up < 0 {
    //                 break
    //             }
    //         }

    //         let mut down: usize = played_index;
    //         while self.board[down].state == played {
    //             ver_count += 1;
    //             down += BOARD_SIZE as usize;
    //             if down as u16 >= BOARD_SIZE * BOARD_SIZE {
    //                 break
    //             }
    //         }

    //         final_score += ver_count as i64 - 2
    //     }

    //     // up left going diags
    //     let mut up_left_diag_count: i16 = 0;
    //     {
    //         let mut up: i16 = played_index as i16;
    //         while self.board[up as usize].state == played {
    //             up_left_diag_count += 1;
    //             let futur_move = up as i16 - BOARD_SIZE as i16 - 1;
    //             up = futur_move as i16;
    //             if up < 0 || (up + 1) % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }

    //         let mut down: i16 = played_index as i16;
    //         while self.board[down as usize].state == played {
    //             up_left_diag_count += 1;
    //             let futur_move = down as u16 + BOARD_SIZE + 1;
    //             down = futur_move as i16;
    //             if down >= AREA as i16 || down % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }

    //         final_score += up_left_diag_count as i64 - 2
    //     }

    //     // // up right going diags
    //     let mut up_right_diag_count: i16 = 0;
    //     {
    //         let mut up: i16 = played_index as i16;
    //         while self.board[up as usize].state == played {
    //             up_right_diag_count += 1;
    //             let futur_move = up as i16 - BOARD_SIZE as i16 + 1;
    //             up = futur_move as i16;
    //             if up < 0 || up % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }

    //         let mut down: i16 = played_index as i16;
    //         while self.board[down as usize].state == played {
    //             up_right_diag_count += 1;
    //             let futur_move = down as u16 + BOARD_SIZE - 1;
    //             down = futur_move as i16;
    //             if down >= AREA as i16 || (down + 1) % BOARD_SIZE as i16 == 0 {
    //                 break
    //             }
    //         }

    //         final_score += up_right_diag_count as i64 - 2
    //     }
        
    //     final_score + 1
    // }
}