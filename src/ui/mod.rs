//! Assemblage de l'interface : découpe l'écran en zones et délègue le
//! rendu de chacune aux sous-modules spécialisés.

pub mod goban;
pub mod menu;
pub mod panels;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Frame;

use crate::app::{App, Screen, MIN_HEIGHT, MIN_WIDTH};

/// Point d'entrée du rendu, appelé une fois par tour de boucle. Vérifie
/// d'abord la taille du terminal (section 6 du sujet : un terminal trop
/// petit doit afficher un message clair, jamais paniquer sur un calcul de
/// layout ni dessiner n'importe quoi).
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        panels::render_too_small(frame, area);
        return;
    }

    match app.screen {
        Screen::Menu => menu::draw(frame, area, app),
        Screen::Playing => draw_playing(frame, area, app),
    }
}

fn draw_playing(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    panels::render_title(frame, outer[0]);
    panels::render_help(frame, outer[2]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Length(43), Constraint::Min(30)])
        .split(outer[1]);

    panels::render_log(frame, main[0], app);
    goban::render(frame, main[1], app);
    panels::render_side(frame, main[2], app);
}
