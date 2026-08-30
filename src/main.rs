use std::{rc::Rc, thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer, Cell}, layout::{self, Alignment, Constraint, Direction, Layout, Position, Rect}, style::{Color::{self, Indexed}, Style, Stylize}, symbols::block, widgets::{Block, BorderType, Padding, Paragraph, Scrollbar, ScrollbarState},
};

use crossterm::event::{self, KeyCode, Event};
use crossterm::ExecutableCommand;

mod board;

#[derive(Debug, Clone)]
struct AppScrollBar {
    _scrollbar_state: ScrollbarState,
    _scrollbar_length: usize,
}

impl AppScrollBar {
    fn new() -> Self {
        AppScrollBar {
            _scrollbar_state: ScrollbarState::new(0),
            _scrollbar_length: 0,
        }
    }

    fn update(&mut self) {
        self._scrollbar_state = self._scrollbar_state.content_length(self._scrollbar_length + 1);
        self._scrollbar_length += 1;
    }

    fn scroll_down(&mut self) {
        self._scrollbar_state.next();
    }

    fn scroll_up(&mut self) {
        self._scrollbar_state.prev();
    }
}

#[derive(Debug, Clone)]
pub struct App<'a> {
    _board: board::Board<'a>,
    _scrollbar: AppScrollBar,
    _mouse_pos: (u16, u16),
    _chunks: Rc<[Rect]>,
}

impl App<'_> {
    pub fn new() -> Self {
        App {
            _board: board::Board::new(),
            _scrollbar: AppScrollBar::new(),
            _mouse_pos: (0, 0),
            _chunks: Rc::new([]),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|frame| self.render_ui(frame))?;
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == event::KeyEventKind::Release {
                        // Skip events that are not KeyEventKind::Press
                        continue
                    }

                    if key.code == KeyCode::Char('q') {
                        break
                    }

                    if key.code == KeyCode::Char('r') {
                        self._board = board::Board::new()
                    }
                },
                Event::Mouse(mouse) => {
                    match mouse.kind { 
                        event::MouseEventKind::Moved => {
                            self._mouse_pos = (mouse.column, mouse.row);
                            self._board.handle_mouse_move(mouse.column, mouse.row);
                            // self._scrollbar.update();
                        },
                        event::MouseEventKind::Down(button) if button == event::MouseButton::Left => {
                            self._board.handle_mouse_left_click();
                            self._scrollbar._scrollbar_state.first();
                        },
                        event::MouseEventKind::ScrollDown => {
                            if self._chunks[0].contains(Position::new(self._mouse_pos.0, self._mouse_pos.1)) {
                                self._scrollbar.scroll_down();
                            }
                        },
                        event::MouseEventKind::ScrollUp => {
                            if self._chunks[0].contains(Position::new(self._mouse_pos.0, self._mouse_pos.1)) {
                                self._scrollbar.scroll_up();
                            }
                        },
                        _ => { continue; },
                    }
                    self._scrollbar.update();
                }
                _ => {},
            }
        }
        Ok(())
    }

    fn render_ui(&mut self, frame: &mut Frame) {
        self._chunks = Layout::horizontal([
            Constraint::Percentage(25),
            Constraint::Percentage(50),
            Constraint::Percentage(25),
        ]).split(frame.area());

        // render le cadre exterieur premiere moitie
        frame.render_widget(Paragraph::new(self._board._log_lines.clone()).scroll((
            self._scrollbar._scrollbar_state.get_position() as u16,
            0,
        )).block(Block::bordered().padding(Padding::left(1)).border_type(BorderType::Rounded).bg(Color::Indexed(233))), self._chunks[0]);

        frame.render_widget(Block::bordered().padding(Padding::left(1)).border_type(BorderType::Rounded), self._chunks[2]);

        // render le board seconde moitie
        let center_area = self._chunks[1].centered(
            Constraint::Length(self._board._ui_hor_size * self._board._cols),
             Constraint::Length(self._board._ui_ver_size * self._board._rows),
        );
        self._board.init_areas(center_area);
        frame.render_widget(self._board.clone(), center_area);
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

