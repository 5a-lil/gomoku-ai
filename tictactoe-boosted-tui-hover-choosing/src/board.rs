use std::{rc::Rc, thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer, Cell}, layout::{self, Alignment, Constraint, Direction, Layout, Position, Rect}, style::{Color, Style, Stylize}, symbols::block, widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Widget},
};

use crossterm::event::{self, KeyCode};

pub const BOARD_SIZE: u16 = 19;
pub const HOR_SIZE: u16 = 4;
pub const VER_SIZE: u16 = 2;

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
        self._mouse_position = (col, row)
    }

    pub fn handle_mouse_left_click(&mut self) {
        for (i, a) in self._board_areas.iter().enumerate() {
            if a.contains(Position::new(self._mouse_position.0, self._mouse_position.1)) {
                self._board_states[i] = self.play();
                return;
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