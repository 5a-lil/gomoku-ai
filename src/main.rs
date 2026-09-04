use std::{rc::Rc, thread::sleep, time::Duration};

use ratatui::{
    DefaultTerminal, Frame, buffer::{Buffer, Cell}, layout::{self, Alignment, Constraint, Direction, Layout, Position, Rect}, style::{Color::{self, Indexed}, Style, Stylize}, symbols::block, text::{Line, Text}, widgets::{Block, BorderType, Padding, Paragraph, Scrollbar, ScrollbarState},
};

use crossterm::event::{self, KeyCode};
use crossterm::ExecutableCommand;

mod board;
mod event_thread;

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

#[derive(Debug)]
pub struct App<'a> {
    _board: board::Board<'a>,
    _event_thread: event_thread::EventThread,
    _scrollbar: AppScrollBar,
    _mouse_pos: (u16, u16),
    _chunks: Rc<[Rect]>,
}

impl App<'_> {
    pub fn new() -> Self {
        App {
            _board: board::Board::new(),
            _event_thread: event_thread::EventThread::new(250),
            _scrollbar: AppScrollBar::new(),
            _mouse_pos: (0, 0),
            _chunks: Rc::new([]),
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), Box<dyn std::error::Error>> {
        self._board._playing = board::State::Empty; // to start the board with no game launched
        loop {
            terminal.draw(|frame| self.render_ui(frame))?;
            match self._event_thread.read()? {
                event_thread::Event::Key(key) => {
                    if key.kind == event::KeyEventKind::Release {
                        // Skip events that are not KeyEventKind::Press
                        continue
                    }

                    if key.code == KeyCode::Char('q') {
                        break
                    }

                    if key.code == KeyCode::Char('r') {
                        self._board = board::Board::new();
                        self._board._mouse_position = self._mouse_pos; // je fais ca sinon petit bug on peut pas cliquer et placer le pion sans bouger un petit peu la souris pour re actualiser
                        self._board._log_lines.insert(0, Line::from("O== 1v1 game started. It's time for black to play! ==O").alignment(Alignment::Center));
                        self._board._log_lines.insert(0, Line::from(""))
                    }

                    if key.code == KeyCode::Char('a') {
                        self._board = board::Board::new();
                        self._board._mouse_position = self._mouse_pos; // je fais ca sinon petit bug on peut pas cliquer et placer le pion sans bouger un petit peu la souris pour re actualiser
                        self._board._ai = true;
                        self._board._log_lines.insert(0, Line::from("O== 1vAI game started. It's time for black to play! ==O").alignment(Alignment::Center));
                        self._board._log_lines.insert(0, Line::from(""))
                    }
                },
                event_thread::Event::Mouse(mouse) => {
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
                },
                event_thread::Event::Tick => {},
            }
        }
        Ok(())
    }

    fn render_ui(&mut self, frame: &mut Frame) {
        self._chunks = Layout::horizontal([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ]).split(frame.area());

        // render le block gauche
        let left_block = Block::bordered().padding(Padding::left(1)).border_type(BorderType::Rounded).bg(Color::Indexed(233));
        frame.render_widget(Paragraph::new(self._board._log_lines.clone()).scroll((
            self._scrollbar._scrollbar_state.get_position() as u16,
            0,
        )).block(left_block), self._chunks[0]);

        // render le jeu au milieu
        let center_area = self._chunks[1].centered(
            Constraint::Length(self._board._ui_hor_size * self._board._cols),
             Constraint::Length(self._board._ui_ver_size * self._board._rows),
        );
        self._board.init_areas(center_area);
        frame.render_widget(self._board.clone(), center_area);

        // render le block de droite
        let right_sub_chunks = Layout::vertical([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ]).split(self._chunks[2]);
        let right_block = Block::bordered().padding(Padding::left(1)).border_type(BorderType::Rounded);

        frame.render_widget(Paragraph::new(vec![
            Line::from(">- General Metrics -<").white().bold(),
            Line::from(""),
            Line::from(format!("Computation time: {} ms", self._board._computation_time)),
        ]).alignment(Alignment::Center).block(right_block), right_sub_chunks[1].centered(Constraint::Percentage(50), Constraint::Length(5)));
        frame.render_widget(Paragraph::new(vec![
            Line::from(" ▄▄▄▄▄▄▄                                    ").alignment(Alignment::Center),
            Line::from("███▀▀▀▀▀                       ▄▄           ").alignment(Alignment::Center),
            Line::from("███       ▄███▄ ███▄███▄ ▄███▄ ██ ▄█▀ ██ ██ ").alignment(Alignment::Center),
            Line::from("███  ███▀ ██ ██ ██ ██ ██ ██ ██ ████   ██ ██ ").alignment(Alignment::Center),
            Line::from("▀██████▀  ▀███▀ ██ ██ ██ ▀███▀ ██ ▀█▄ ▀██▀█ ").alignment(Alignment::Center),
            Line::from(""),
            Line::from(">- How to play -<").white().bold().alignment(Alignment::Center),
            Line::from(""),
            Line::from("o- [q] to quit at any time").white().bold(),
            Line::from("o- [r] to re/start 1v1 at any time".white().bold()),
            Line::from("o- [a] to re/start 1vsAI at any time".white().bold()),
            Line::from("o- Click anywhere on board to place stone".white().bold()),
            Line::from("o- Left logger for game logs (ex: [<player-color>|<captures>])".white().bold()),
        ]).alignment(Alignment::Left), right_sub_chunks[0].centered(Constraint::Length(63), Constraint::Length(12)));
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

