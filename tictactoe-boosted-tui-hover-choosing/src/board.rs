use std::{cell, rc::Rc, thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer}, layout::{self, Alignment, Constraint, Direction, Layout, Position, Rect}, style::{Color, Style, Stylize}, symbols::block, widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Widget},
};

use crossterm::event::{self, KeyCode};

const BOARD_SIZE: u16 = 19;
const HOR_SIZE: u16 = 4;
const VER_SIZE: u16 = 2;
const WIN_COND: i16 = 5;
const NO_CAPTURE: usize = 9250;

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Capture {
    other: usize,
    first: usize,
    second: usize,
}

impl Capture {
    pub fn new(other: usize, first: usize, second: usize) -> Self {
        Capture {
            other,
            first,
            second,
        }
    }

    pub fn end_capture(&mut self) {
        self.other = NO_CAPTURE;
        self.first = NO_CAPTURE;
        self.second = NO_CAPTURE;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct Captured {
    whites: u8,
    blacks: u8,
}

impl Captured {
    pub fn decr(&mut self, state: State) {
        match state {
            State::Black => {
                self.blacks -= 1
            },
            State::White => {
                self.whites -= 1
            },
            _ => { panic!("Captured decr panic") }
        }
    }

    pub fn incr(&mut self, state: State) {
        match state {
            State::Black => {
                self.blacks += 1
            },
            State::White => {
                self.whites += 1
            },
            _ => { panic!("Captured incr panic") }
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Cell {
    pub state: State,
    pub captured: Captured,
    pub captures: [Capture; 8],
}

impl Cell {
    pub fn new() -> Self {
        Cell {
            state: State::Empty,
            captured: Captured::default(),
            captures: [Capture {other: NO_CAPTURE, first: NO_CAPTURE, second: NO_CAPTURE}; 8],
        }
    }

    pub fn playable(&self, player: Playing) -> bool {
        self.state == State::Empty && match player {
            Playing::Black => {
                self.captured.whites == 0
            },
            Playing::White => {
                self.captured.blacks == 0
            },
            _ => { true }
        }
    }

    pub fn delete_capture(&mut self, other: usize) -> (usize, usize) {
        let capture = self.captures.iter_mut().find(|elem| elem.other == other).unwrap();
        let to_release: (usize, usize) = (capture.first, capture.second);
        capture.end_capture();
        to_release
    }

    pub fn clear_all_captures(&mut self) {
        for capture in self.captures.iter_mut() {
            capture.end_capture()
        }
    }

    pub fn capture(&mut self, other: usize, first: usize, second: usize) {
        *(self.captures.iter_mut().find(|elem| elem.other == NO_CAPTURE).unwrap()) = Capture::new(other, first, second);
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum State {
    Empty,
    Black,
    White
}

impl State {
    pub fn opposite(&self) -> State {
        match self {
            State::Black => {
                State::White
            },
            State::White => {
                State::Black
            },
            State::Empty => {
                State::Empty
            },
        }
    }
}

type Playing = State;

#[derive(Debug, Copy, Clone)]
pub struct Board {
    _board_states: [Cell; (BOARD_SIZE * BOARD_SIZE) as usize],
    _board_areas: [Rect; (BOARD_SIZE * BOARD_SIZE) as usize],
    pub _cols: u16,
    pub _rows: u16,
    pub _ui_hor_size: u16,
    pub _ui_ver_size: u16,
    pub _area: u16,
    _mouse_position: (u16, u16),
    _playing: State,
} 

impl Board {
    pub fn new() -> Self {
        Board {
            _board_states: [Cell::new(); (BOARD_SIZE * BOARD_SIZE) as usize],
            _board_areas: [Rect::ZERO; (BOARD_SIZE * BOARD_SIZE) as usize],
            _cols: BOARD_SIZE,
            _rows: BOARD_SIZE,
            _ui_hor_size: HOR_SIZE,
            _ui_ver_size: VER_SIZE,
            _area: BOARD_SIZE * BOARD_SIZE,
            _mouse_position: (0, 0),
            _playing: Playing::Black,
        }
    }

    pub fn handle_mouse_move(&mut self, col: u16, row: u16) {
        // Temporarily blocking play when there is a win or draw
        // if self._playing == State::Empty {
        //     return;
        // }

        self._mouse_position = (col, row)
    }

    pub fn handle_mouse_left_click(&mut self) {
        // Temporarily blocking play when there is a win or draw
        if self._playing == Playing::Empty {
            return;
        }

        for (i, a) in self._board_areas.iter().enumerate() {
            if a.contains(Position::new(self._mouse_position.0, self._mouse_position.1)) && self._board_states[i].playable(self._playing) {
                if self.double_free_three(i) {
                    return;
                }
                self._board_states[i].state = self.play();
                self.check_game(i);
                return;
            }
        }
    }

    fn double_free_three(&self, played_index: usize) -> bool {
        let mut found: u8 = 0;

        // left
        {
            let mut tester = played_index as i16;
            let mut count = 0;

            for _ in 0..3 {
                tester -= 1;
                if (tester + 1) % self._cols as i16 == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // right
        {
            let mut tester = played_index as u16;
            let mut count = 0;

            for _ in 0..3 {
                tester += 1;
                if tester % self._cols == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // up
        {
            let mut tester = played_index as i16;
            let mut count = 0;

            for _ in 0..3 {
                tester -= self._cols as i16;
                if tester < 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // down
        {
            let mut tester = played_index as i16;
            let mut count = 0;

            for _ in 0..3 {
                tester += self._cols as i16;
                if tester >= self._area as i16 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // up right
        {
            let mut tester = played_index as i16;
            let mut count = 0;

            for _ in 0..3 {
                tester -= self._cols as i16;
                tester += 1;
                if tester < 0 || tester % self._cols as i16 == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // up left
        {
            let mut tester = played_index as i16;
            let mut count = 0;

            for _ in 0..3 {
                tester -= self._cols as i16;
                tester -= 1;
                if tester < 0 || (tester + 1) % self._cols as i16 == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // down right
        {
            let mut tester = played_index as u16;
            let mut count = 0;

            for _ in 0..3 {
                tester += self._cols;
                tester += 1;
                if tester >= self._area || tester % self._cols == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        // down left
        {
            let mut tester = played_index as u16;
            let mut count = 0;

            for _ in 0..3 {
                tester += self._cols;
                tester -= 1;
                if tester >= self._area || (tester + 1) % self._cols == 0 {
                    break;
                }
                if self._board_states[tester as usize].state == self._playing {
                    count += 1;
                }
            }

            if count == 2 {
                found += 1;
            }
        }

        found == 2
    }

    fn check_game(&mut self, played_index: usize) {
        self.check_lines(played_index);
        self.check_captures(played_index);
    }

    fn check_captures(&mut self, played_index: usize) {
        let played: State = self._board_states[played_index].state;

        fn process_capture(board: &mut Board, played_index: usize, played: State, tester: usize, diff: i16) {
            let first = (tester as i16 + (diff / 2)) as usize;
            let second = (tester as i16 + diff) as usize;
            // println!("{} {}", first, second);

            board._board_states[played_index].capture(tester, first, second);
            board._board_states[tester].capture(played_index, first, second);
            board._board_states[first].captured.incr(played);
            board._board_states[second].captured.incr(played);
            if board._board_states[first].state == board._board_states[second].state && board._board_states[first].state != played {
                println!("release");
                board.release_capture(first);
                board.release_capture(second);
                board._board_states[first].state = State::Empty;
                board._board_states[second].state = State::Empty;
            }
            println!("captured !!");
        }

        //left
        (|| {
            let mut tester = played_index as i32;
            for _ in 0..3 {
                tester -= 1;
                if (tester + 1) % self._cols as i32 == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }


            process_capture(self, played_index, played, tester, 2);
        })();

        //right
        (|| {
            let mut tester = played_index as u16;
            for _ in 0..3 {
                tester += 1;
                if tester % self._cols == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, -2);
        })();

        //down
        (|| {
            let mut tester = played_index as u16;
            for _ in 0..3 {
                tester += self._cols;
                if tester >= self._cols * self._cols {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, self._cols as i16 * -2);
        })();

        //up
        (|| {
            let mut tester = played_index as i16;
            for _ in 0..3 {
                tester -= self._cols as i16;
                if tester < 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, self._cols as i16 * 2);
        })();

        //diag up right
        (|| {
            let mut tester = played_index as i16;
            for _ in 0..3 {
                tester -= self._cols as i16;
                tester += 1;
                if tester < 0 || tester % self._cols as i16 == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, (self._cols as i16 - 1) * 2);
        })();

        //diag up left
        (|| {
            let mut tester = played_index as i16;
            for _ in 0..3 {
                tester -= self._cols as i16;
                tester -= 1;
                if tester < 0 || (tester + 1) % self._cols as i16 == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, (self._cols as i16 + 1) * 2);
        })();

        //diag down right
        (|| {
            let mut tester = played_index as u16;
            for _ in 0..3 {
                tester += self._cols;
                tester += 1;
                if tester >= self._cols * self._cols || tester % self._cols == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, (self._cols as i16 + 1) * -2);
        })();

        //diag down left
        (|| {
            let mut tester = played_index as i16;
            for _ in 0..3 {
                tester += self._cols as i16;
                tester -= 1;
                if tester >= (self._cols * self._cols) as i16 || (tester + 1) % self._cols as i16 == 0 {
                    return;
                }
            }

            let tester: usize = tester as usize;
            if self._board_states[tester].state != played {
                return;
            }

            process_capture(self, played_index, played, tester, (self._cols as i16 - 1) * -2);
        })();
    }

    fn release_capture(&mut self, origin_index: usize) {
        let mut origin = self._board_states[origin_index];
        for capture in origin.captures.iter_mut() {
            // trouver la deuxieme cell qui capture
            let scnd_origin_index = capture.other;
            if scnd_origin_index == NO_CAPTURE {
                continue;
            }
            println!("RELEASE");
            // enlever les captures des listes des deux cells
            let to_release = self._board_states[scnd_origin_index].delete_capture(origin_index);
            // diminuer la valeur de captured pour les deux cells capture
            self._board_states[to_release.0].captured.decr(origin.state);
            self._board_states[to_release.1].captured.decr(origin.state);
        }
        self._board_states[origin_index].clear_all_captures()
    }

    fn is_capturable(&self, played_index: usize) -> bool {
        let mut possibles = 0;

        fn process<F1, F2, F3, F4>(
                board: &Board, 
                played_index: usize, 
                find_behind_index: F1,
                phase_out: F2,
                behind_out: F3,
                diff: F4,
            ) -> u8 

            where 
                F1: FnOnce(i16) -> i16,
                F2: Fn(i16) -> bool,
                F3: Fn(i16) -> bool,
                F4: Fn(i16, i16) -> i16,
        {
            let played_state = board._board_states[played_index].state;
            let mut possibles: u8 = 0;
            let played_index: i16 = played_index as i16;
            let behind_index = find_behind_index(played_index);

            for phase in 1..3 {
                let phase_index = diff(played_index, phase);
                // println!("{}", phase);
                match phase {
                    1 => {
                        if phase_out(phase_index) || board._board_states[phase_index as usize].state != played_state {
                            break;
                        }
                    },
                    2 => {
                        if phase_out(phase_index) || board._board_states[phase_index as usize].state == played_state {
                            break;
                        }
                        if board._board_states[phase_index as usize].state == played_state.opposite() {
                            if behind_out(behind_index) {
                                break;
                            }
                            if board._board_states[behind_index as usize].state == State::Empty {
                                possibles += 1;
                            }
                        } else {
                            if behind_out(behind_index) {
                                break;
                            }
                            if board._board_states[behind_index as usize].state == played_state.opposite() {
                                possibles += 1;
                            }
                        }
                    },
                    _ => {},
                }
            }
            possibles
        }

        //left
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index + 1
            },
            |index| -> bool {
                (index + 1) % self._cols as i16 == 0
            },
            |index|-> bool {
                index % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index - phase
            }
        );

        //right
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index - 1
            },
            |index| -> bool {
                index % self._cols as i16 == 0
            },
            |index|-> bool {
                (index + 1) % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index + phase
            }
        );

        //up
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index + self._cols as i16
            },
            |index| -> bool {
                index < 0
            },
            |index|-> bool {
                index >= self._area as i16
            },
            |index, phase| -> i16 {
                index - phase * self._cols as i16
            }
        );

        //down
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index - self._cols as i16
            },
            |index| -> bool {
                index >= self._area as i16
            },
            |index|-> bool {
                index < 0
            },
            |index, phase| -> i16 {
                index + phase * self._cols as i16
            }
        );

        //up right
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index + self._cols as i16 - 1
            },
            |index| -> bool {
                index < 0 || index % self._cols as i16 == 0
            },
            |index|-> bool {
                index >= self._area as i16 || (index + 1) % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index - phase * self._cols as i16 + phase * 1
            }
        );

        //up left
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index + self._cols as i16 + 1
            },
            |index| -> bool {
                index < 0 || (index + 1) % self._cols as i16 == 0
            },
            |index|-> bool {
                index >= self._area as i16 || index % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index - phase * self._cols as i16 - phase * 1
            }
        );

        //down right
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index - self._cols as i16 - 1
            },
            |index| -> bool {
                index >= self._area as i16 || index % self._cols as i16 == 0
            },
            |index|-> bool {
                index < 0 || (index + 1) % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index + phase * self._cols as i16 + phase * 1
            }
        );

        //down left
        possibles += process(
            self, 
            played_index,
            |index| -> i16 {
                index - self._cols as i16 + 1
            },
            |index| -> bool {
                index >= self._area as i16 || (index + 1) % self._cols as i16 == 0
            },
            |index|-> bool {
                index < 0 || index % self._cols as i16 == 0
            },
            |index, phase| -> i16 {
                index + phase * self._cols as i16 - phase * 1
            }
        );
        
        if possibles > 0 {
            println!("c CAPTURABLE");
        }

        possibles > 0
    }

    fn check_lines(&mut self, played_index: usize) {
        let played: State = self._board_states[played_index].state;

        // horizontal
        let mut hor_count: i16 = 0;
        {
            let mut left: i16 = played_index as i16;
            while self._board_states[left as usize].state == played {
                if self.is_capturable(left as usize) || (left + 1) % self._cols as i16 == 0 {
                    break;
                }
                hor_count += 1;
                left -= 1;
            }

            let mut right: i16 = played_index as i16;
            while self._board_states[right as usize].state == played {
                if self.is_capturable(right as usize) || right % self._cols as i16 == 0 {
                    break;
                }
                hor_count += 1;
                right += 1;
            }

            if hor_count - 1 >= WIN_COND {
                self._playing = Playing::Empty;
                return
            }
        }

        // vertical
        let mut ver_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self._board_states[up as usize].state == played {
                if self.is_capturable(up as usize) || up < 0 {
                    break;
                }
                ver_count += 1;
                up -= self._cols as i16;
            }

            let mut down: usize = played_index;
            while self._board_states[down].state == played {
                if self.is_capturable(down) || down as u16 >= self._cols * self._cols {
                    break;
                }
                ver_count += 1;
                down += self._cols as usize;
            }

            if ver_count - 1 >= WIN_COND {
                self._playing = Playing::Empty;
                return
            }
        }

        // up left going diags
        let mut up_left_diag_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self._board_states[up as usize].state == played {
                if self.is_capturable(up as usize) || up < 0 || (up + 1) % self._cols as i16 == 0 {
                    break;
                }
                up_left_diag_count += 1;
                let futur_move = up as i16 - self._cols as i16 - 1;
                up = futur_move as i16;
            }

            let mut down: i16 = played_index as i16;
            while self._board_states[down as usize].state == played {
                if self.is_capturable(down as usize) || down >= self._area as i16 || down % self._cols as i16 == 0 {
                    break;
                }
                up_left_diag_count += 1;
                let futur_move = down as u16 + self._cols + 1;
                down = futur_move as i16;
            }

            if up_left_diag_count - 1 >= WIN_COND {
                self._playing = Playing::Empty;
                return
            }
        }

        // up right going diags
        let mut up_right_diag_count: i16 = 0;
        {
            let mut up: i16 = played_index as i16;
            while self._board_states[up as usize].state == played {
                if self.is_capturable(up as usize) || up < 0 || up % self._cols as i16 == 0 {
                    break;
                }
                up_right_diag_count += 1;
                let futur_move = up as i16 - self._cols as i16 + 1;
                up = futur_move as i16;
            }

            let mut down: i16 = played_index as i16;
            while self._board_states[down as usize].state == played {
                if self.is_capturable(down as usize) || down >= self._area as i16 || (down + 1) % self._cols as i16 == 0 {
                    break;
                }
                up_right_diag_count += 1;
                let futur_move = down as u16 + self._cols - 1;
                down = futur_move as i16;
            }

            if up_right_diag_count - 1 >= WIN_COND {
                self._playing = Playing::Empty;
                return
            }
        }

    }

    fn play(&mut self) -> State {
        if self._playing == Playing::White {
            self._playing = Playing::Black;
            State::White
        } else {
            self._playing = Playing::White;
            State::Black
        }
    }

    pub fn init_areas(&mut self, area: Rect) {
        let ver_constr = (0..self._rows).map(|_| Constraint::Length(self._ui_ver_size as u16));
        let hor_constr = (0..self._cols).map(|_| Constraint::Length(self._ui_hor_size as u16));
        let ver_layout = Layout::vertical(ver_constr);
        let hor_layout = Layout::horizontal(hor_constr);

        let ver_chunks = ver_layout.split(area);
        let chunks = ver_chunks.iter().flat_map(|row| {
            hor_layout.split(*row).to_vec()
        });
        for (i, elem) in chunks.enumerate() {
            self._board_areas[i] = elem;
        }
    }
}

impl Widget for Board {
    fn render(self, _: Rect, buf: &mut Buffer) {
        for (i, c) in self._board_areas.iter().enumerate() {

            match self._board_states[i].state {
                State::Black => {
                    Block::bordered().border_type(BorderType::Rounded).bg(Color::Blue).render(*c, buf);
                },
                State::White => {
                    Block::bordered().border_type(BorderType::Rounded).bg(Color::Red).render(*c, buf);
                },
                _ => {
                    if c.contains(ratatui::prelude::Position::new(self._mouse_position.0, self._mouse_position.1)) {
                        Block::bordered().border_type(BorderType::Rounded).bg(Color::Yellow).render(*c, buf);
                        continue;
                    }
                    Block::bordered().border_type(BorderType::Rounded).render(*c, buf);
                }
            }
        }
    }
}