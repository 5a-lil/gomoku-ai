use std::{thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer, Cell}, layout::{self, Alignment, Constraint, Direction, Layout, Rect}, style::{Color, Style, Stylize}, symbols::block, widgets::{Block, BorderType, Borders, Clear, Paragraph, Row, Widget},
};

use crossterm::event::{self, KeyCode, Event};
use crossterm::ExecutableCommand;

mod board;

#[derive(Debug, Clone)]
pub struct App {
    _board: board::Board,
}

impl App {
    pub fn new() -> Self {
        App {
            _board: board::Board::new(),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|frame| self.render_ui(frame))?;
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == event::KeyEventKind::Release {
                        // Skip events that are not KeyEventKind::Press
                        continue;
                    }

                    if key.code == KeyCode::Char('q') {
                        break;
                    }
                },
                Event::Mouse(mouse) => {
                    match mouse.kind { 
                        event::MouseEventKind::Moved => {
                            self._board.handle_mouse_move(mouse.column, mouse.row);
                        },
                        event::MouseEventKind::Down(button) if button == event::MouseButton::Left => {
                            self._board.handle_mouse_left_click();
                        }
                        _ => {},
                    }
                }
                _ => {},
            }
        }
        Ok(())
    }

    fn render_ui(&mut self, frame: &mut Frame) {
        let halves = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ]).split(frame.area());

        // render le cadre exterieur
        let main_block = Block::bordered()
            .title_top("Haut")
            .title_bottom("Bas")
            .title_alignment(Alignment::Center);
        frame.render_widget(main_block, halves[0]);

        // render le board
        let center_area = halves[1].centered(
            Constraint::Length(self._board._ui_hor_size * self._board._cols),
             Constraint::Length(self._board._ui_ver_size * self._board._rows),
        );
        self._board.init_areas(center_area);
        frame.render_widget(self._board, center_area);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    std::io::stdout().execute(crossterm::event::EnableMouseCapture).unwrap();
    let _ = App::new().run(&mut terminal);
    std::io::stdout().execute(crossterm::event::DisableMouseCapture).unwrap();
    ratatui::restore();
    
    Ok(())
}

