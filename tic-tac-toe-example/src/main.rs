use std::io::{stdout, Write, stdin};
use std::io;
use std::cmp::{max, min};
use std::time::Instant;

const X_PLAYING: char = 'X';
const O_PLAYING: char = 'O';
const RESET: char = '.';

const WIN: i8 = 10;
const LOSE: i8 = -10;
const TIE: i8 = 0;
const NOT_FINISHED: i8 = 1;

fn ttt_is_ended(board: &mut Vec<char>) -> i8 {

    let len: f32 = board.len() as f32;
    let loop_max = len.sqrt() as usize;

    // lines
    'outer: for i in 0..loop_max {
        let init_char = board[0 + loop_max * i];
        for j in 0..loop_max - 1 {
            if board[j + loop_max * i] != board[j + 1 + loop_max * i] {
                continue 'outer;
            } 
        }
        if init_char == O_PLAYING {
            return WIN;
        } else if init_char == X_PLAYING {
            return LOSE;
        }
    }

    // columns
    'outer: for i in 0..loop_max {
        let init_char = board[0 + 1 * i];
        for j in 0..loop_max - 1 {
            if board[loop_max * j + 1 * i] != board[loop_max * (j + 1) + 1 * i] {
                continue 'outer;
            }
        }
        if init_char == O_PLAYING {
            return WIN;
        } else if init_char == X_PLAYING {
            return LOSE;
        }
    }

    // diags
    {
        let init_char = board[0];
        let mut all_good = true;
        for j in 0..loop_max - 1 {
            if board[(loop_max + 1) * j] != board[(loop_max + 1) * (j + 1)] {
                all_good = false;
                break;
            }
        }
        if all_good && init_char == O_PLAYING {
            return WIN;
        } else if all_good && init_char == X_PLAYING {
            return LOSE;
        }
    }
    {
        let init_char = board[loop_max - 1];
        let mut all_good = true;
        for j in 0..loop_max - 1 {
            if board[loop_max - 1 + j * (loop_max - 1)] != board[loop_max - 1 + (j + 1) * (loop_max - 1)] {
                all_good = false;
                break;
            }
        }
        if all_good && init_char == O_PLAYING {
            return WIN;
        } else if all_good && init_char == X_PLAYING {
            return LOSE;
        }
    }

    if let None = board.iter().find(|&elem| *elem == '.') {
        return TIE;
    }

    NOT_FINISHED

}

fn minimax(board: &mut Vec<char>, depth: i8, mut alpha: i8, mut beta: i8, maximizing: bool, metrics: &mut i64) -> (i8, usize) {
    *metrics += 1;
    if depth == 10 || ttt_is_ended(board) != NOT_FINISHED {
        let ret = ttt_is_ended(board);
        if ret == WIN {
            return (ret - depth, 0);
        }
        return (ret, 0);
    }

    let mut value: i8;
    let mut pos: usize = 92;

    if maximizing {
        value = -128;
        for i in 0..board.len() {
            if board[i] != '.' {
                continue;
            }

            board[i] = O_PLAYING;
            let mm = minimax(board, depth + 1, alpha, beta ,false, metrics);
            if mm.0 > value {
                value = mm.0;
                pos = i;
            }
            board[i] = RESET;

            alpha = max(alpha, mm.0);
            if beta <= alpha {
                break;
            }
        }
    }
    else {
        value = 127;
        for i in 0..board.len() {
            if board[i] != '.' {
                continue;
            }

            board[i] = X_PLAYING;
            let mm = minimax(board, depth + 1, alpha, beta, true, metrics);
            if mm.0 < value {
                value = mm.0;
                pos = i;
            }
            board[i] = RESET;

            beta = min(beta, mm.0);
            if beta <= alpha {
                break;
            }
        }
    }

    (value, pos)
}

fn ai_play(board: &mut Vec<char>) {
    let mut metrics: i64 = 0;
    let start_time = Instant::now();
    let mm = minimax(board, 0, -120, 120, true, &mut metrics);
    let end_time = start_time.elapsed();
    println!("execution time: {} ms", end_time.as_millis());
    println!("number of iters: {} iters", metrics);
    board[mm.1] = O_PLAYING;
}

fn input(prompt: String) -> io::Result<String> {
    print!("{prompt}");
    stdout().flush()?;

    let mut buf: String = String::new();
    stdin().read_line(&mut buf)?;

    Ok(buf.trim_end().to_string())
}

pub struct TTTGame {
    board: Vec<char>,
    playing: char
}

impl TTTGame {
    fn new(size: usize) -> Self {
        TTTGame { 
            board: vec!['.'; size * size],
            playing: 'X'
        }
    }

    fn print_board(self: &Self) {
        let size= (self.board.len() as f32).sqrt() as usize;
        for i in 0..size {
            print!("|");
            for j in 0..size {
                print!(" {} |", self.board[j + size * i]);
            }
            println!("");
        }
    }

    fn next_turn(self: &mut Self) -> io::Result<()> {
        let input = input(format!("{}'s turn: ", self.playing))?;

        if input.len() > 2 {
            return Ok(());
        }

        if let Ok(num) = input.parse::<u8>() {
            if num == 0 {
                return Ok(());
            }
            if self.board[num as usize - 1] != '.' {
                return Ok(());
            }
            self.board[num as usize - 1] = self.playing;
        } else {
            return Ok(());
        }

        Ok(())
    }
}

fn num_cases(size: u64) -> u64 {
    let mut res: u64 = 1;
    for i in 1..=size {
        let mut to_add: u64 = size;
        for j in 1..i {
            to_add *= size - j;
        }
        res += to_add;
    }
    res
}

fn main() -> io::Result<()> {

    let mut game: TTTGame = TTTGame::new(3);
    game.print_board();
    while ttt_is_ended(&mut game.board) == NOT_FINISHED {
        game.next_turn()?;
        game.print_board();
        ai_play(&mut game.board);
        game.print_board();
    }

    Ok(())
}
