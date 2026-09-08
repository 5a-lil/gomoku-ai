//! Rendu du plateau et correspondance écran <-> coordonnées de jeu pour la
//! souris. La grille est dessinée comme un simple `Paragraph` de lignes
//! stylées : pas de `Buffer::set_string` manuel, ce qui évite tout calcul
//! d'index de bas niveau côté interface (la seule arithmétique de position
//! reste dans `game::position`).

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::game::position::{xy_to_index, BLACK, BOARD_SIZE, WHITE};

const COL_LETTERS: &[u8] = b"ABCDEFGHJKLMNOPQRST"; // le 'I' est sauté, comme au Go.

/// Largeur d'une cellule à l'écran : le symbole plus une espace de
/// séparation. Doit rester cohérent entre `render` et `screen_to_cell`.
const CELL_W: u16 = 2;
/// Largeur de la colonne d'étiquettes de ligne à gauche ("19 ", " 1 ", ...).
const LABEL_W: u16 = 3;

/// Convertit une position écran absolue (colonne, ligne du terminal) en
/// coordonnées de plateau `(x, y)`, si le clic tombe dans la grille. `area`
/// doit être la zone intérieure du bloc du goban (après bordure), telle que
/// stockée par `App` après le dernier rendu (voir `render` ci-dessous).
pub fn screen_to_cell(area: Rect, column: u16, row: u16) -> Option<(usize, usize)> {
    if area.width <= LABEL_W || area.height <= 1 {
        return None;
    }
    let origin_x = area.x + LABEL_W;
    let origin_y = area.y + 1; // ligne d'en-tête des lettres de colonnes
    if column < origin_x || row < origin_y {
        return None;
    }
    let cx = (column - origin_x) / CELL_W;
    let cy = row - origin_y;
    if (cx as usize) < BOARD_SIZE && (cy as usize) < BOARD_SIZE {
        Some((cx as usize, cy as usize))
    } else {
        None
    }
}

/// Dessine le plateau dans `area` et enregistre la zone intérieure dans
/// `app.goban_area`, pour que les clics souris suivants puissent être
/// traduits en coordonnées via `screen_to_cell` avec exactement la même
/// géométrie que celle utilisée pour le rendu.
pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    let block = Block::default().borders(Borders::ALL).title(" Goban 19x19 ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.goban_area = inner;

    let mut lines: Vec<Line> = Vec::with_capacity(BOARD_SIZE + 1);

    let mut header_spans = vec![Span::raw(" ".repeat(LABEL_W as usize))];
    for x in 0..BOARD_SIZE {
        let letter = COL_LETTERS.get(x).copied().unwrap_or(b'?') as char;
        header_spans.push(Span::styled(format!("{letter} "), Style::default().fg(Color::DarkGray)));
    }
    lines.push(Line::from(header_spans));

    for y in 0..BOARD_SIZE {
        let mut spans = vec![Span::styled(format!("{:>2} ", y + 1), Style::default().fg(Color::DarkGray))];
        for x in 0..BOARD_SIZE {
            let index = xy_to_index(x, y);
            let (symbol, style) = cell_appearance(app, index);
            spans.push(Span::styled(format!("{symbol} "), style));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn cell_appearance(app: &App, index: usize) -> (char, Style) {
    let Some(game) = app.game.as_ref() else {
        return ('.', Style::default().fg(Color::DarkGray));
    };
    let cell = game.position.cells[index];

    let (mut symbol, mut style) = match cell {
        BLACK => ('X', Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        WHITE => ('O', Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        _ => ('.', Style::default().fg(Color::DarkGray)),
    };

    let is_empty = cell != BLACK && cell != WHITE;

    // Grisage des cases interdites par la règle d'ouverture (section 5) :
    // seulement pertinent sur une case vide, évalué en dernier recours pour
    // ne pas masquer les autres surbrillances plus importantes.
    if is_empty && app.opening_forbids(index) {
        style = Style::default().fg(Color::Rgb(70, 70, 70));
    }

    // Carte de chaleur des scores de la racine (touche `h`), sous les
    // surbrillances de coup/capture qui restent prioritaires visuellement.
    if is_empty
        && app.show_heatmap
        && let Some(bg) = app.heatmap_color(index)
    {
        style = style.bg(bg).fg(Color::Black);
    }

    if app.just_captured.contains(&index) {
        style = Style::default().bg(Color::Red).fg(Color::White).add_modifier(Modifier::BOLD);
    } else if app.last_move_highlight == Some(index) {
        style = style.bg(Color::Yellow).fg(Color::Black).add_modifier(Modifier::BOLD);
    }

    if app.suggestion == Some(index) {
        style = Style::default().bg(Color::Green).fg(Color::Black).add_modifier(Modifier::BOLD);
        if is_empty {
            symbol = '*';
        }
    }

    if app.cursor == index {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    (symbol, style)
}
