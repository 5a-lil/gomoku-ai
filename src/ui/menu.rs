//! Écran de configuration de partie (section 4.1 du sujet) : mode, couleur
//! humaine, règle d'ouverture, budget de temps IA et nombre de threads.

use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

const ROW_LABELS: [&str; 7] = [
    "Mode",
    "Couleur humaine",
    "Règle d'ouverture",
    "Budget IA (soft, ms)",
    "Budget IA (hard, ms)",
    "Threads",
    "» Commencer la partie «",
];

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Configuration — Gomoku (projet 42) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Flèches haut/bas : choisir une ligne — gauche/droite : changer la valeur — Entrée : démarrer",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    for (row, label) in ROW_LABELS.iter().enumerate() {
        let selected = app.menu.row == row;
        let marker = if selected { "▶ " } else { "  " };
        let value = value_text(app, row);
        let label_style = if selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw(marker),
            Span::styled(format!("{label:<22}"), label_style),
            Span::styled(value, Style::default().fg(Color::Cyan)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Règles implémentées : alignement de 5+, captures de paires, victoire à 10 pierres capturées,",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "interdiction du double-trois (sauf capture ou coup gagnant), fin de partie par capture (section 3.4).",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled("[q] quitter", Style::default().fg(Color::DarkGray))));

    let p = Paragraph::new(lines).alignment(Alignment::Left).wrap(Wrap { trim: false });
    frame.render_widget(p, inner);
}

fn value_text(app: &App, row: usize) -> String {
    match row {
        0 => if app.menu.vs_ai { "Humain vs IA".into() } else { "Humain vs Humain (hotseat)".into() },
        1 => {
            if app.menu.vs_ai {
                if app.menu.human_black { "Noir".into() } else { "Blanc".into() }
            } else {
                "(sans effet en hotseat)".into()
            }
        }
        2 => {
            let rule = app.menu.rule();
            format!("{} — {}", rule.name(), rule.description())
        }
        3 => format!("{} ms (ne pas dépasser cette échéance pour lancer une itération de plus)", app.menu.soft_ms),
        4 => format!("{} ms (interruption immédiate)", app.menu.hard_ms),
        5 => format!("{} (max matériel : {})", app.menu.threads, crate::ai::search::default_thread_count()),
        _ => String::new(),
    }
}
