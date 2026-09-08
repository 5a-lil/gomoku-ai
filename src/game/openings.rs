//! Règles d'ouverture (bonus, section IV du sujet) : Standard, Pro, Long Pro,
//! Swap et Swap2.
//!
//! Point clé de conception : les variantes Swap ne changent JAMAIS l'ordre
//! d'alternance des couleurs sur le plateau (`Position::to_move` continue de
//! basculer Noir/Blanc/Noir/... normalement pour les 3 premières pierres,
//! exactement comme une partie standard). Ce qu'elles changent, c'est
//! seulement **quelle identité (humain ou IA) contrôle quelle couleur** à
//! partir d'un point de décision : c'est une information de haut niveau,
//! stockée dans `Game`, pas dans `Position`. Cela simplifie énormément
//! l'implémentation par rapport à un système qui tenterait de faire jouer
//! deux coups de suite au même joueur.

use crate::game::position::{index_to_xy, BOARD_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpeningRule {
    Standard,
    Pro,
    LongPro,
    Swap,
    Swap2,
}

impl OpeningRule {
    pub const ALL: [OpeningRule; 5] = [
        OpeningRule::Standard,
        OpeningRule::Pro,
        OpeningRule::LongPro,
        OpeningRule::Swap,
        OpeningRule::Swap2,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            OpeningRule::Standard => "Standard",
            OpeningRule::Pro => "Pro",
            OpeningRule::LongPro => "Long Pro",
            OpeningRule::Swap => "Swap",
            OpeningRule::Swap2 => "Swap2",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            OpeningRule::Standard => "aucune contrainte de placement",
            OpeningRule::Pro => "Noir doit jouer le centre puis s'en écarter d'au moins 3",
            OpeningRule::LongPro => "Noir doit jouer le centre puis s'en écarter d'au moins 4",
            OpeningRule::Swap => "le joueur 1 place 3 pierres, le joueur 2 choisit sa couleur",
            OpeningRule::Swap2 => "comme Swap, avec une option intermédiaire de 2 pierres de plus",
        }
    }
}

/// Point de décision de couleur en attente, identifié par le nombre de
/// pierres déjà posées sur le plateau au moment où il se présente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingChoice {
    /// Swap : après les 3 pierres initiales, choisir Noir ou Blanc.
    PickColor,
    /// Swap2 : après les 3 pierres initiales, choisir Noir, Blanc, ou placer
    /// 2 pierres de plus avant de laisser le joueur 1 choisir.
    Swap2FirstDecision,
    /// Swap2 : après les 2 pierres supplémentaires, le joueur 1 choisit enfin
    /// sa couleur.
    Swap2FinalPick,
}

/// Décision prise à un point de choix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDecision {
    TakeBlack,
    TakeWhite,
    PlaceTwoMore,
}

#[derive(Debug, Clone)]
pub struct OpeningState {
    pub rule: OpeningRule,
    swap2_extra_placed: bool,
    /// Vrai une fois la 1re décision (Swap : couleur ; Swap2 : couleur ou
    /// "2 pierres de plus") prise. Nécessaire car cette décision ne fait
    /// avancer aucun coup sur le plateau : sans ce drapeau, `pending_choice`
    /// continuerait à la réclamer indéfiniment tant que `moves_played` reste
    /// à 3.
    initial_choice_resolved: bool,
    /// Même besoin pour la décision finale de Swap2 (à `moves_played == 5`).
    final_choice_resolved: bool,
}

impl OpeningState {
    pub fn new(rule: OpeningRule) -> Self {
        OpeningState {
            rule,
            swap2_extra_placed: false,
            initial_choice_resolved: false,
            final_choice_resolved: false,
        }
    }

    /// Vérifie si `index` est autorisé pour le coup n° `moves_played`
    /// (0-indexé : nombre de pierres déjà sur le plateau avant ce coup). Ne
    /// contraint que Pro/Long Pro ; toujours `Ok` pour les autres règles.
    pub fn check_placement(&self, moves_played: u32, index: usize) -> Result<(), String> {
        let center = (BOARD_SIZE as i32 - 1) / 2;
        let Some((x, y)) = index_to_xy(index) else {
            return Err("en dehors du plateau".to_string());
        };
        let (x, y) = (x as i32, y as i32);
        // Distance de Tchebychev au centre : c'est elle qui définit le carré
        // "interdit" autour du centre dans les règles Pro/Long Pro.
        let dist = (x - center).abs().max((y - center).abs());

        match (self.rule, moves_played) {
            (OpeningRule::Pro, 0) | (OpeningRule::LongPro, 0) => {
                if dist != 0 {
                    return Err("le premier coup de Noir doit être exactement le centre".into());
                }
            }
            (OpeningRule::Pro, 2) => {
                if dist < 3 {
                    return Err(
                        "le 2e coup de Noir doit être à au moins 3 intersections du centre".into(),
                    );
                }
            }
            (OpeningRule::LongPro, 2) => {
                if dist < 4 {
                    return Err(
                        "le 2e coup de Noir doit être à au moins 4 intersections du centre".into(),
                    );
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Décision en attente après `moves_played` pierres posées, s'il y en a
    /// une. `None` signifie que la partie peut continuer normalement.
    pub fn pending_choice(&self, moves_played: u32) -> Option<PendingChoice> {
        match (self.rule, moves_played) {
            (OpeningRule::Swap, 3) if !self.initial_choice_resolved => Some(PendingChoice::PickColor),
            (OpeningRule::Swap2, 3) if !self.initial_choice_resolved => {
                Some(PendingChoice::Swap2FirstDecision)
            }
            (OpeningRule::Swap2, 5) if self.swap2_extra_placed && !self.final_choice_resolved => {
                Some(PendingChoice::Swap2FinalPick)
            }
            _ => None,
        }
    }

    pub fn mark_swap2_extra_placed(&mut self) {
        self.swap2_extra_placed = true;
    }

    pub fn mark_initial_choice_resolved(&mut self) {
        self.initial_choice_resolved = true;
    }

    pub fn mark_final_choice_resolved(&mut self) {
        self.final_choice_resolved = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::xy_to_index;

    #[test]
    fn pro_refuse_premier_coup_hors_centre() {
        let state = OpeningState::new(OpeningRule::Pro);
        let center = xy_to_index(9, 9);
        let off_center = xy_to_index(9, 10);
        assert!(state.check_placement(0, center).is_ok());
        assert!(state.check_placement(0, off_center).is_err());
    }

    #[test]
    fn pro_refuse_deuxieme_coup_trop_proche() {
        let state = OpeningState::new(OpeningRule::Pro);
        let too_close = xy_to_index(10, 9); // distance 1 du centre (9,9)
        let far_enough = xy_to_index(12, 9); // distance 3
        assert!(state.check_placement(2, too_close).is_err());
        assert!(state.check_placement(2, far_enough).is_ok());
    }

    #[test]
    fn long_pro_exige_distance_quatre() {
        let state = OpeningState::new(OpeningRule::LongPro);
        let dist_3 = xy_to_index(12, 9); // distance 3, insuffisant pour Long Pro
        let dist_4 = xy_to_index(13, 9); // distance 4
        assert!(state.check_placement(2, dist_3).is_err());
        assert!(state.check_placement(2, dist_4).is_ok());
    }

    #[test]
    fn standard_naccepte_aucune_contrainte() {
        let state = OpeningState::new(OpeningRule::Standard);
        let anywhere = xy_to_index(0, 0);
        assert!(state.check_placement(0, anywhere).is_ok());
        assert!(state.check_placement(2, anywhere).is_ok());
    }

    #[test]
    fn swap_declenche_le_choix_apres_trois_pierres() {
        let state = OpeningState::new(OpeningRule::Swap);
        assert_eq!(state.pending_choice(2), None);
        assert_eq!(state.pending_choice(3), Some(PendingChoice::PickColor));
    }

    #[test]
    fn swap2_declenche_le_choix_final_apres_les_deux_pierres_extra() {
        let mut state = OpeningState::new(OpeningRule::Swap2);
        assert_eq!(state.pending_choice(3), Some(PendingChoice::Swap2FirstDecision));
        assert_eq!(state.pending_choice(5), None); // pas encore marqué
        state.mark_swap2_extra_placed();
        assert_eq!(state.pending_choice(5), Some(PendingChoice::Swap2FinalPick));
    }

    #[test]
    fn une_decision_une_fois_resolue_ne_revient_pas() {
        let mut state = OpeningState::new(OpeningRule::Swap);
        assert!(state.pending_choice(3).is_some());
        state.mark_initial_choice_resolved();
        assert_eq!(state.pending_choice(3), None);
    }
}
