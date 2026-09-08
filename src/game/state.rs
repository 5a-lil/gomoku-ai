//! `Game` orchestre une partie complète : la `Position`, la règle d'ouverture
//! active, l'alignement "en attente" éventuel (section 3.4 du sujet) et le
//! journal affiché à l'écran. C'est la seule couche qui connaît les notions
//! de mode de jeu et de qui contrôle quelle couleur ; `ai::search` ne
//! travaille, lui, que sur une `Position` nue.

use crate::game::openings::{ColorDecision, OpeningRule, OpeningState, PendingChoice};
use crate::game::position::{coord_name, opponent, Position, BLACK, WHITE};
use crate::game::rules::{self, AlignmentOutcome, IllegalReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    HumanVsAi { human: u8 },
    HumanVsHuman,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Ongoing,
    Win(u8),
    WinByCapture(u8),
    Draw,
}

impl Outcome {
    pub fn is_over(&self) -> bool {
        !matches!(self, Outcome::Ongoing)
    }
}

#[derive(Debug, Clone)]
struct PendingAlignment {
    player: u8,
    stones: Vec<usize>,
}

/// Compte-rendu d'un coup joué avec succès, pour que l'appelant (interface)
/// sache quoi journaliser/afficher sans dupliquer la logique d'arbitrage.
#[derive(Debug, Clone)]
pub struct MoveReport {
    pub player: u8,
    pub index: usize,
    pub captured_pairs: usize,
    pub captured_indices: Vec<usize>,
    pub outcome: Outcome,
    pub pending_alignment_started: bool,
    pub pending_alignment_resolved: bool,
}

pub struct Game {
    pub position: Position,
    pub mode: GameMode,
    pub opening: OpeningState,
    pending: Option<PendingAlignment>,
    pub outcome: Outcome,
    pub log: Vec<String>,
    /// Couleur jouée par l'humain. N'a de sens qu'en mode `HumanVsAi` ; pour
    /// les règles Swap/Swap2, cette valeur peut changer une fois la phase de
    /// choix résolue.
    pub human_color: u8,
}

impl Game {
    pub fn new(mode: GameMode, rule: OpeningRule) -> Self {
        let human_color = match mode {
            GameMode::HumanVsAi { human } => human,
            GameMode::HumanVsHuman => BLACK,
        };
        Game {
            position: Position::new(),
            mode,
            opening: OpeningState::new(rule),
            pending: None,
            outcome: Outcome::Ongoing,
            log: Vec::new(),
            human_color,
        }
    }

    pub fn log(&mut self, message: String) {
        self.log.insert(0, message);
        if self.log.len() > 500 {
            self.log.truncate(500);
        }
    }

    /// Décision de couleur en attente, s'il y en a une. Tant qu'elle n'est
    /// pas résolue via `apply_color_decision`, aucun coup ne peut être joué.
    pub fn pending_choice(&self) -> Option<PendingChoice> {
        if self.outcome.is_over() {
            return None;
        }
        self.opening.pending_choice(self.position.stone_count as u32)
    }

    /// Vrai si, en mode Humain vs IA, la décision en attente revient à l'IA
    /// (simplification assumée et documentée dans DEFENSE.md : en mode
    /// Humain vs IA, l'humain place toujours les pierres initiales des
    /// règles Swap/Swap2, et c'est donc l'IA qui exploite le déséquilibre en
    /// choisissant sa couleur en premier ; le choix final de Swap2, lui,
    /// revient au joueur qui a posé les pierres initiales, donc à l'humain).
    pub fn pending_choice_is_ai_turn(&self) -> bool {
        matches!(self.mode, GameMode::HumanVsAi { .. })
            && matches!(
                self.pending_choice(),
                Some(PendingChoice::PickColor) | Some(PendingChoice::Swap2FirstDecision)
            )
    }

    /// Applique une décision de couleur. `decided_by_ai` indique qui a
    /// physiquement pris la décision, pour calculer correctement la couleur
    /// finale de l'humain en mode Humain vs IA.
    pub fn apply_color_decision(&mut self, decision: ColorDecision, decided_by_ai: bool) {
        let which = self.pending_choice();

        match decision {
            ColorDecision::TakeBlack => {
                if let GameMode::HumanVsAi { .. } = self.mode {
                    self.human_color = if decided_by_ai { WHITE } else { BLACK };
                }
                self.log(format!(
                    "{} choisit de jouer les Noirs",
                    if decided_by_ai { "L'IA" } else { "Le joueur" }
                ));
            }
            ColorDecision::TakeWhite => {
                if let GameMode::HumanVsAi { .. } = self.mode {
                    self.human_color = if decided_by_ai { BLACK } else { WHITE };
                }
                self.log(format!(
                    "{} choisit de jouer les Blancs",
                    if decided_by_ai { "L'IA" } else { "Le joueur" }
                ));
            }
            ColorDecision::PlaceTwoMore => {
                self.opening.mark_swap2_extra_placed();
                self.log("2 pierres supplémentaires vont être placées avant le choix final".into());
            }
        }

        // Marque la décision comme résolue : sans cela, `pending_choice`
        // continuerait à la réclamer tant qu'aucun coup n'a fait avancer le
        // nombre de pierres sur le plateau (ces décisions n'en placent pas).
        match which {
            Some(PendingChoice::PickColor) | Some(PendingChoice::Swap2FirstDecision) => {
                self.opening.mark_initial_choice_resolved();
            }
            Some(PendingChoice::Swap2FinalPick) => {
                self.opening.mark_final_choice_resolved();
            }
            None => {}
        }
    }

    /// Joue un coup pour le joueur actuellement au trait. Vérifie dans
    /// l'ordre : partie déjà terminée, décision de couleur en attente,
    /// contraintes d'ouverture, légalité générale (occupation, double-trois).
    pub fn try_play(&mut self, index: usize) -> Result<MoveReport, String> {
        if self.outcome.is_over() {
            return Err("la partie est terminée".to_string());
        }
        if self.pending_choice().is_some() {
            return Err("une décision de couleur est en attente avant de continuer".to_string());
        }

        let moves_played = self.position.stone_count as u32;
        self.opening
            .check_placement(moves_played, index)
            .map_err(IllegalReason::RegleOuverture)
            .map_err(|e| e.to_string())?;
        rules::check_move_legal(&self.position, index).map_err(|e| e.to_string())?;

        let player = self.position.to_move;
        let undo = self.position.make_move(index);
        let captured_pairs = undo.pairs_captured_count();
        let captured_indices = undo.captured_indices();

        self.log(format!(
            "{} joue en {}{}",
            player_name(player),
            coord_name(index),
            if captured_pairs > 0 {
                format!(" et capture {captured_pairs} paire(s)")
            } else {
                String::new()
            }
        ));

        let mut report = MoveReport {
            player,
            index,
            captured_pairs,
            captured_indices,
            outcome: Outcome::Ongoing,
            pending_alignment_started: false,
            pending_alignment_resolved: false,
        };

        // 1. Victoire par capture : condition inconditionnelle (section 3.3).
        if rules::has_won_by_capture(&self.position, player) {
            self.outcome = Outcome::WinByCapture(player);
            self.log(format!("{} gagne par capture (10 pierres) !", player_name(player)));
            report.outcome = self.outcome;
            return Ok(report);
        }

        // 2. Alignement de 5+ tout juste formé : arbitrage section 3.4.
        if self.position.forms_five(index) {
            match rules::resolve_alignment(&self.position, index, player) {
                AlignmentOutcome::Win => {
                    self.outcome = Outcome::Win(player);
                    self.log(format!("{} aligne cinq pierres et gagne !", player_name(player)));
                }
                AlignmentOutcome::OpponentWinsByCapture => {
                    let winner = opponent(player);
                    self.outcome = Outcome::WinByCapture(winner);
                    self.log(format!(
                        "{} a déjà perdu 4 paires : {} gagne par capture avant que l'alignement ne compte",
                        player_name(player),
                        player_name(winner)
                    ));
                }
                AlignmentOutcome::Pending => {
                    let stones = self.position.winning_stones_through(index);
                    self.pending = Some(PendingAlignment { player, stones });
                    report.pending_alignment_started = true;
                    self.log(
                        "alignement de cinq cassable par capture : la partie continue, l'adversaire a un tour pour la casser".into(),
                    );
                }
            }
            report.outcome = self.outcome;
            if self.outcome.is_over() {
                return Ok(report);
            }
        } else if let Some(pending) = self.pending.clone() {
            // 3. Tour de grâce de l'adversaire sur un alignement en attente.
            if rules::alignment_still_intact(&self.position, &pending.stones, pending.player) {
                self.outcome = Outcome::Win(pending.player);
                self.log(format!(
                    "{} n'a pas cassé la ligne : {} gagne par alignement",
                    player_name(player),
                    player_name(pending.player)
                ));
                self.pending = None;
                report.outcome = self.outcome;
                return Ok(report);
            } else {
                self.pending = None;
                report.pending_alignment_resolved = true;
                self.log("la ligne a été cassée par capture, la partie continue".into());
            }
        }

        // 4. Match nul.
        if rules::is_draw(&self.position) {
            self.outcome = Outcome::Draw;
            self.log("plus aucun coup légal : match nul".into());
            report.outcome = self.outcome;
        }

        Ok(report)
    }

    /// Vrai si c'est actuellement au tour de l'humain de jouer (utile à
    /// l'interface pour savoir si elle doit attendre un clic ou lancer l'IA).
    pub fn is_human_turn(&self) -> bool {
        match self.mode {
            GameMode::HumanVsAi { .. } => self.position.to_move == self.human_color,
            GameMode::HumanVsHuman => true,
        }
    }
}

fn player_name(player: u8) -> &'static str {
    if player == BLACK {
        "Noir"
    } else {
        "Blanc"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::xy_to_index;

    #[test]
    fn partie_standard_simple_se_joue_normalement() {
        let mut game = Game::new(GameMode::HumanVsHuman, OpeningRule::Standard);
        let report = game.try_play(xy_to_index(9, 9)).unwrap();
        assert_eq!(report.player, BLACK);
        assert_eq!(game.outcome, Outcome::Ongoing);
    }

    #[test]
    fn regle_pro_bloque_un_premier_coup_hors_centre() {
        let mut game = Game::new(GameMode::HumanVsHuman, OpeningRule::Pro);
        let err = game.try_play(xy_to_index(0, 0));
        assert!(err.is_err());
    }

    #[test]
    fn victoire_par_alignement_termine_la_partie() {
        let mut game = Game::new(GameMode::HumanVsHuman, OpeningRule::Standard);
        // Noir : 5,5 6,5 7,5 8,5 puis 9,5 ; Blanc joue ailleurs entre-temps.
        let black_moves = [(5, 5), (6, 5), (7, 5), (8, 5), (9, 5)];
        let white_moves = [(0, 0), (0, 1), (0, 2), (0, 3)];
        for i in 0..4 {
            game.try_play(xy_to_index(black_moves[i].0, black_moves[i].1)).unwrap();
            game.try_play(xy_to_index(white_moves[i].0, white_moves[i].1)).unwrap();
        }
        let report = game
            .try_play(xy_to_index(black_moves[4].0, black_moves[4].1))
            .unwrap();
        assert_eq!(report.outcome, Outcome::Win(BLACK));
        assert!(game.outcome.is_over());
    }

    #[test]
    fn swap_declenche_bien_une_decision_apres_trois_coups() {
        let mut game = Game::new(GameMode::HumanVsHuman, OpeningRule::Swap);
        game.try_play(xy_to_index(9, 9)).unwrap();
        game.try_play(xy_to_index(3, 3)).unwrap();
        game.try_play(xy_to_index(15, 15)).unwrap();
        assert!(game.pending_choice().is_some());
        // Aucun coup ne doit être accepté tant que la décision n'est pas prise.
        assert!(game.try_play(xy_to_index(4, 4)).is_err());
        game.apply_color_decision(ColorDecision::TakeBlack, false);
        assert!(game.pending_choice().is_none());
        assert!(game.try_play(xy_to_index(4, 4)).is_ok());
    }
}
