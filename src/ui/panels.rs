//! Panneaux annexes de l'écran de jeu : journal (gauche), bandeau de titre,
//! panneau d'état/débogage IA (droite) et barre d'aide (bas). Séparé de
//! `goban.rs` pour que chaque fichier reste focalisé sur une seule
//! responsabilité d'affichage.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::game::openings::PendingChoice;
use crate::game::position::{coord_name, BLACK, WHITE};
use crate::game::{GameMode, Outcome};

/// Bandeau de titre compact (une ligne), affiché en haut de l'écran de jeu.
/// Un vrai logo ASCII en gros caractères a été essayé puis abandonné : il
/// ne rentre pas dans un terminal 96 colonnes (la taille minimale garantie,
/// voir `app::MIN_WIDTH`) sans déborder sur les autres panneaux.
pub fn render_title(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" ◆ GOMOKU ◆ ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled("negamax + alpha-bêta + PVS + TT — projet 42", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

pub fn render_log(frame: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = match &app.game {
        Some(game) => game
            .log
            .iter()
            .take(area.height.saturating_sub(2) as usize)
            .map(|line| ListItem::new(line.as_str()))
            .collect(),
        None => Vec::new(),
    };
    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Journal "));
    frame.render_widget(list, area);
}

fn player_label(player: u8) -> &'static str {
    if player == BLACK { "Noir" } else { "Blanc" }
}

fn fmt_ms(ms: f64) -> String {
    format!("{ms:.1} ms")
}

/// Panneau de droite : état de la partie, timer (exigence de validation du
/// sujet, section 4.2), et panneau de débogage détaillé de l'IA (section
/// 4.3), affiché ou masqué avec la touche `d`.
pub fn render_side(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" État & IA ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    match &app.game {
        None => lines.push(Line::from("aucune partie en cours")),
        Some(game) => {
            let mode_txt = match game.mode {
                GameMode::HumanVsAi { human } => {
                    format!("Humain vs IA (humain = {})", player_label(human))
                }
                GameMode::HumanVsHuman => "Humain vs Humain (hotseat)".to_string(),
            };
            lines.push(Line::from(vec![Span::styled("Mode : ", Style::default().fg(Color::DarkGray)), Span::raw(mode_txt)]));
            lines.push(Line::from(vec![
                Span::styled("Ouverture : ", Style::default().fg(Color::DarkGray)),
                Span::raw(game.opening.rule.name()),
            ]));

            let (b, w) = (game.position.pairs_captured[BLACK as usize], game.position.pairs_captured[WHITE as usize]);
            lines.push(Line::from(vec![
                Span::styled("Paires capturées : ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("Noir {b}/5  "), Style::default().fg(Color::White)),
                Span::styled(format!("Blanc {w}/5"), Style::default().fg(Color::Cyan)),
            ]));

            match game.outcome {
                Outcome::Ongoing => {
                    let turn = format!("Trait à : {}", player_label(game.position.to_move));
                    lines.push(Line::from(Span::styled(turn, Style::default().add_modifier(Modifier::BOLD))));
                }
                Outcome::Win(p) => {
                    lines.push(Line::from(Span::styled(
                        format!("{} gagne par alignement !", player_label(p)),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    )));
                }
                Outcome::WinByCapture(p) => {
                    lines.push(Line::from(Span::styled(
                        format!("{} gagne par capture !", player_label(p)),
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    )));
                }
                Outcome::Draw => {
                    lines.push(Line::from(Span::styled("Match nul.", Style::default().fg(Color::Magenta))));
                }
            }

            if let Some(pending) = game.pending_choice() {
                let msg = match pending {
                    PendingChoice::PickColor => "Décision Swap : [1] Noir  [2] Blanc",
                    PendingChoice::Swap2FirstDecision => "Décision Swap2 : [1] Noir  [2] Blanc  [3] Placer 2 pierres de plus",
                    PendingChoice::Swap2FinalPick => "Décision Swap2 finale : [1] Noir  [2] Blanc",
                };
                if game.pending_choice_is_ai_turn() {
                    lines.push(Line::from(Span::styled("L'IA évalue son choix de couleur...", Style::default().fg(Color::Yellow))));
                } else {
                    lines.push(Line::from(Span::styled(msg, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
                }
            }
        }
    }

    lines.push(Line::from(""));

    // Timer : exigence de validation du sujet (section 4.2). Toujours
    // visible, jamais masqué par la bascule du panneau de débogage.
    let last = app.timer_last_ms();
    let avg = app.timer_avg_ms();
    let max = app.timer_max_ms();
    let over_budget = last > 500.0 || avg > 500.0;
    let timer_style = if over_budget { Style::default().fg(Color::Red).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::Green) };
    lines.push(Line::from(Span::styled("Temps de réflexion IA", Style::default().add_modifier(Modifier::UNDERLINED))));
    lines.push(Line::from(vec![
        Span::raw("  dernier : "),
        Span::styled(fmt_ms(last), timer_style),
    ]));
    lines.push(Line::from(vec![Span::raw("  moyenne : "), Span::styled(fmt_ms(avg), timer_style)]));
    lines.push(Line::from(vec![Span::raw("  max     : "), Span::styled(fmt_ms(max), Style::default().fg(Color::Yellow))]));

    if app.ai_thinking() {
        let elapsed = app.ai_thinking_elapsed().unwrap_or_default();
        lines.push(Line::from(Span::styled(
            format!("  IA en réflexion... ({:.0} ms)", elapsed.as_secs_f64() * 1000.0),
            Style::default().fg(Color::Yellow),
        )));
    }

    if let Some(status) = &app.status {
        lines.push(Line::from(Span::styled(format!("» {status}"), Style::default().fg(Color::Red))));
    }

    if app.show_debug {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Débogage IA (dernière recherche)", Style::default().add_modifier(Modifier::UNDERLINED))));
        match &app.last_search_stats {
            None => lines.push(Line::from("  (aucune recherche encore effectuée)")),
            Some(stats) => {
                let nps = if stats.elapsed.as_secs_f64() > 0.0 {
                    stats.nodes as f64 / stats.elapsed.as_secs_f64()
                } else {
                    0.0
                };
                let hit_rate = if stats.tt_probes > 0 { stats.tt_hits as f64 * 100.0 / stats.tt_probes as f64 } else { 0.0 };
                lines.push(Line::from(format!("  profondeur atteinte : {}", stats.depth_reached)));
                lines.push(Line::from(format!("  nœuds : {} ({nps:.0} nœuds/s)", stats.nodes)));
                lines.push(Line::from(format!("  score : {:+} (positif = favorable au joueur qui a joué)", stats.score)));
                lines.push(Line::from(format!("  table de transposition : {:.1}% de succès ({}/{})", hit_rate, stats.tt_hits, stats.tt_probes)));
                lines.push(Line::from(format!("  candidats à la racine : {}", stats.root_candidates)));

                if !stats.pv.is_empty() {
                    let pv_str = stats.pv.iter().map(|&i| coord_name(i)).collect::<Vec<_>>().join(" ");
                    lines.push(Line::from(format!("  variante principale : {pv_str}")));
                }

                if !stats.root_scores.is_empty() {
                    lines.push(Line::from("  meilleurs coups à la racine :"));
                    for &(idx, score) in stats.root_scores.iter().take(5) {
                        lines.push(Line::from(format!("    {} : {:+}", coord_name(idx), score)));
                    }
                }
            }
        }

        if app.show_breakdown {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Détail du score (point de vue Noir)", Style::default().add_modifier(Modifier::UNDERLINED))));
            if let Some(b) = app.score_breakdown() {
                lines.push(Line::from(format!("  positionnel (motifs) : {:+}", b.positional_black)));
                lines.push(Line::from(format!("  capture Noir : {:+}   capture Blanc : {:+}", b.capture_black, b.capture_white)));
                lines.push(Line::from(format!(
                    "  vulnérabilité Noir : {:+}   vulnérabilité Blanc : {:+}",
                    b.vulnerability_black, b.vulnerability_white
                )));
                lines.push(Line::from(format!("  total (Noir) : {:+}", b.total_black)));
                lines.push(Line::from(format!("  total (joueur au trait) : {:+}", b.total_mover)));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(format!("Threads : {}   Budget : {}/{} ms", app.menu.threads, app.menu.soft_ms, app.menu.hard_ms)));
        let tt_status = if app.tt_is_empty() {
            " (dégradée : échec d'allocation)".to_string()
        } else {
            String::new()
        };
        lines.push(Line::from(format!("Table de transposition : {} entrées{tt_status}", app.tt_entries())));
        lines.push(Line::from(format!("Heatmap (h) : {}   Détail (b) : {}", on_off(app.show_heatmap), on_off(app.show_breakdown))));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}

fn on_off(b: bool) -> &'static str {
    if b { "activé" } else { "désactivé" }
}

pub fn render_help(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw(" "),
        "[clic/flèches+entrée]".bold(),
        Span::raw(" jouer  "),
        "[s]".bold(),
        Span::raw(" suggestion  "),
        "[d]".bold(),
        Span::raw(" débogage  "),
        "[h]".bold(),
        Span::raw(" heatmap  "),
        "[b]".bold(),
        Span::raw(" détail score  "),
        "[r]".bold(),
        Span::raw(" 1v1  "),
        "[a]".bold(),
        Span::raw(" 1vIA  "),
        "[n]".bold(),
        Span::raw(" menu  "),
        "[q]".bold(),
        Span::raw(" quitter"),
    ]);
    frame.render_widget(Paragraph::new(line).style(Style::default().fg(Color::DarkGray)), area);
}

pub fn render_too_small(frame: &mut Frame, area: Rect) {
    let text = format!(
        "Terminal trop petit ({}x{}).\nAgrandissez la fenêtre (au moins {}x{}) pour afficher le plateau.",
        area.width, area.height, crate::app::MIN_WIDTH, crate::app::MIN_HEIGHT
    );
    let p = Paragraph::new(text)
        .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}
