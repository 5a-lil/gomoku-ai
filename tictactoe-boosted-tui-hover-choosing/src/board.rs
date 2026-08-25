use std::{rc::Rc, thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer, Cell}, layout::{self, Alignment, Constraint, Direction, Layout, Position, Rect}, style::{Color, Style, Stylize}, symbols::block, widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Widget},
};

use crossterm::event::{self, KeyCode};

const BOARD_SIZE: u16 = 19;
const HOR_SIZE: u16 = 4;
const VER_SIZE: u16 = 2;
const WIN_COND: u16 = 5;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CellState {
    Empty,
    Black,
    White
}

#[derive(Debug, Copy, Clone)]
pub struct Board {
    _board_states: [CellState; (BOARD_SIZE * BOARD_SIZE) as usize],
    _board_areas: [Rect; (BOARD_SIZE * BOARD_SIZE) as usize],
    pub _cols: u16,
    pub _rows: u16,
    pub _ui_hor_size: u16,
    pub _ui_ver_size: u16,
    _mouse_position: (u16, u16),
    _playing: CellState,
} 

impl Board {
    pub fn new() -> Self {
        Board {
            _board_states: [CellState::Empty; (BOARD_SIZE * BOARD_SIZE) as usize],
            _board_areas: [Rect::ZERO; (BOARD_SIZE * BOARD_SIZE) as usize],
            _cols: BOARD_SIZE,
            _rows: BOARD_SIZE,
            _ui_hor_size: HOR_SIZE,
            _ui_ver_size: VER_SIZE,
            _mouse_position: (0, 0),
            _playing: CellState::Black,
        }
    }

    pub fn handle_mouse_move(&mut self, col: u16, row: u16) {
        // Temporarily blocking play when there is a win or draw
        // if self._playing == CellState::Empty {
        //     return;
        // }

        self._mouse_position = (col, row)
    }

    pub fn handle_mouse_left_click(&mut self) {
        // Temporarily blocking play when there is a win or draw
        if self._playing == CellState::Empty {
            return;
        }

        for (i, a) in self._board_areas.iter().enumerate() {
            if a.contains(Position::new(self._mouse_position.0, self._mouse_position.1)) {
                self._board_states[i] = self.play();
                self.check_game(i);
                return;
            }
        }
    }

    fn check_game(&mut self, played_index: usize) {
        let played: CellState = self._board_states[played_index];

        // horizontal
        let mut hor_count: u16 = 0;
        {
            let mut left: usize = played_index;
            while self._board_states[left] == played {
                hor_count += 1;
                if left as u16 % self._cols == 0 {
                    break;
                }
                left -= 1;
            }

            let mut right: usize = played_index;
            while self._board_states[right] == played {
                hor_count += 1;
                if (right as u16 + 1) % self._cols == 0 {
                    break;
                }
                right += 1;
            }

            if hor_count - 1 >= WIN_COND {
                self._playing = CellState::Empty;
                return
            }
        }

        // vertical
        let mut ver_count: u16 = 0;
        {
            let mut up: usize = played_index;
            while self._board_states[up] == played {
                ver_count += 1;
                if (up as i16 - self._cols as i16) < 0 {
                    break;
                }
                up -= self._cols as usize;
            }

            let mut down: usize = played_index;
            while self._board_states[down] == played {
                ver_count += 1;
                if down as u16 + self._cols >= self._cols * self._cols {
                    break;
                }
                down += self._cols as usize;
            }

            if ver_count - 1 >= WIN_COND {
                self._playing = CellState::Empty;
                return
            }
        }

        // up left going diags
        let mut up_left_diag_count: u16 = 0;
        {
            let mut up: usize = played_index;
            while self._board_states[up] == played {
                up_left_diag_count += 1;
                let futur_move = up as i16 - self._cols as i16 - 1;
                if futur_move < 0 || up % self._cols as usize == 0 {
                    break;
                }
                up = futur_move as usize;
            }

            let mut down: usize = played_index;
            while self._board_states[down] == played {
                up_left_diag_count += 1;
                let futur_move = down as u16 + self._cols + 1;
                if futur_move >= self._cols * self._cols || (down + 1) % self._cols as usize == 0 {
                    break;
                }
                down = futur_move as usize;
            }

            if up_left_diag_count - 1 >= WIN_COND {
                self._playing = CellState::Empty;
                return
            }
        }

        // down right going diags
        let mut up_right_diag_count: u16 = 0;
        {
            let mut up: usize = played_index;
            while self._board_states[up] == played {
                up_right_diag_count += 1;
                let futur_move = up as i16 - self._cols as i16 + 1;
                if futur_move < 0 || (up + 1) % self._cols as usize == 0 {
                    break;
                }
                up = futur_move as usize;
            }

            let mut down: usize = played_index;
            while self._board_states[down] == played {
                up_right_diag_count += 1;
                let futur_move = down as u16 + self._cols - 1;
                if futur_move >= self._cols * self._cols || down % self._cols as usize == 0 {
                    break;
                }
                down = futur_move as usize;
            }

            if up_right_diag_count - 1 >= WIN_COND {
                self._playing = CellState::Empty;
                return
            }
        }

    }

    fn play(&mut self) -> CellState {
        if self._playing == CellState::White {
            self._playing = CellState::Black;
            CellState::White
        } else {
            self._playing = CellState::White;
            CellState::Black
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
    fn render(self, area: Rect, buf: &mut Buffer) {
        for (i, c) in self._board_areas.iter().enumerate() {
            if c.contains(ratatui::prelude::Position::new(self._mouse_position.0, self._mouse_position.1)) {
                Block::bordered().border_type(BorderType::Rounded).bg(Color::Yellow).render(*c, buf);
                continue;
            }

            match self._board_states[i] {
                CellState::Black => {
                    Block::bordered().border_type(BorderType::Rounded).bg(Color::Blue).render(*c, buf);
                },
                CellState::White => {
                    Block::bordered().border_type(BorderType::Rounded).bg(Color::Red).render(*c, buf);
                },
                _ => {
                    Block::bordered().border_type(BorderType::Rounded).render(*c, buf);
                }
            }
        }
    }
}