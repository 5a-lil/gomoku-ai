//! Règles de haut niveau construites sur `Position` : légalité d'un coup
//! (occupation, double-trois), victoire par alignement, clause de fin de
//! partie liée aux captures (section 3.4 du sujet), victoire par capture et
//! match nul.
//!
//! Toutes les fonctions de ce module sont des **prédicats purs** : elles ne
//! mutent jamais la `Position` qu'on leur passe. Quand une simulation est
//! nécessaire (« si l'adversaire jouait ici, casserait-il l'alignement ? »),
//! elle est faite via `make_move` sur un **clone jetable** (`Position` fait
//! moins de 2 Ko, le cloner ne coûte rien), jamais en laissant un prédicat
//! modifier subrepticement l'état de jeu comme le faisait l'ancienne
//! implémentation (`is_capturable` appelé à l'intérieur de la condition d'un
//! `while`, avec effets de bord).

use crate::game::patterns::{count_free_threes, would_form_five};
use crate::game::position::{
    opponent, xy_to_index, Position, BOARD_SIZE, DIRS8,
};

/// 10 pierres capturées, soit 5 paires : condition de victoire du sujet.
pub const CAPTURE_PAIRS_TO_WIN: u8 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IllegalReason {
    HorsPlateau,
    CaseOccupee,
    DoubleTrois,
    RegleOuverture(String),
}

impl std::fmt::Display for IllegalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IllegalReason::HorsPlateau => write!(f, "en dehors du plateau"),
            IllegalReason::CaseOccupee => write!(f, "case déjà occupée"),
            IllegalReason::DoubleTrois => write!(f, "double-trois interdit"),
            IllegalReason::RegleOuverture(msg) => write!(f, "règle d'ouverture : {msg}"),
        }
    }
}

/// Liste des paires adverses que capturerait `player` en jouant en `index`,
/// sans muter la position. Sert à la fois à la légalité (exception
/// double-trois par capture) et à la clause de fin de partie (section 3.4).
pub fn capturing_pairs(pos: &Position, index: usize, player: u8) -> Vec<(usize, usize)> {
    let opp = opponent(player);
    let idx = index as i32;
    let mut pairs = Vec::new();
    for &d in DIRS8.iter() {
        let p1 = idx + d;
        let p2 = idx + 2 * d;
        let p3 = idx + 3 * d;
        if pos.cell_at(p1) == opp && pos.cell_at(p2) == opp && pos.cell_at(p3) == player {
            pairs.push((p1 as usize, p2 as usize));
        }
    }
    pairs
}

/// Légalité d'un coup pour `player` (règles générales uniquement :
/// occupation, double-trois). Les contraintes d'ouverture sont vérifiées à
/// part par `game::openings`, car elles dépendent du numéro de coup, pas
/// seulement de l'état du plateau.
///
/// Ne dépend jamais de `pos.to_move` : on peut ainsi tester la légalité d'un
/// coup pour n'importe quel joueur, y compris pour évaluer un coup de
/// l'adversaire dans `resolve_alignment`.
pub fn check_move_legal_as(pos: &Position, index: usize, player: u8) -> Result<(), IllegalReason> {
    if !pos.is_on_board(index) {
        return Err(IllegalReason::HorsPlateau);
    }
    if !pos.is_empty(index) {
        return Err(IllegalReason::CaseOccupee);
    }

    // Un coup qui aligne 5 pierres ou plus est toujours légal : le sujet
    // n'interdit le double-trois que parce qu'il garantirait une victoire
    // par alignement ultérieure. Si la victoire est immédiate, la question
    // ne se pose plus. Sans ce court-circuit, un coup comme "X X _ X X"
    // serait à tort rejeté puisqu'il forme, en apparence, deux "trois".
    if would_form_five(pos, index, player) {
        return Ok(());
    }

    // Exception explicite du sujet : il n'est pas interdit d'introduire un
    // double-trois en capturant une paire au même coup.
    if !capturing_pairs(pos, index, player).is_empty() {
        return Ok(());
    }

    if count_free_threes(pos, index, player) >= 2 {
        return Err(IllegalReason::DoubleTrois);
    }

    Ok(())
}

#[inline]
pub fn check_move_legal(pos: &Position, index: usize) -> Result<(), IllegalReason> {
    check_move_legal_as(pos, index, pos.to_move)
}

#[inline]
pub fn is_move_legal_as(pos: &Position, index: usize, player: u8) -> bool {
    check_move_legal_as(pos, index, player).is_ok()
}

#[inline]
pub fn is_move_legal(pos: &Position, index: usize) -> bool {
    check_move_legal(pos, index).is_ok()
}

/// Liste brute de tous les coups légaux pour le joueur au trait (règles
/// générales seulement). Balaie les 361 cases : réservée aux tests et à
/// `is_draw`. La recherche IA utilise `ai::movegen`, bien plus rapide car
/// limitée aux cases proches d'une pierre déjà posée.
pub fn legal_moves_naive(pos: &Position) -> Vec<usize> {
    let mut moves = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            let index = xy_to_index(x, y);
            if pos.is_empty(index) && is_move_legal(pos, index) {
                moves.push(index);
            }
        }
    }
    moves
}

/// Résultat de l'arbitrage d'un alignement de 5+ pierres, en tenant compte de
/// la clause de fin de partie liée aux captures (section 3.4 du sujet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignmentOutcome {
    /// Victoire immédiate et définitive de l'auteur de l'alignement.
    Win,
    /// L'auteur de l'alignement a déjà perdu 4 paires ; l'adversaire dispose
    /// d'un coup de capture : il gagne immédiatement par capture.
    OpponentWinsByCapture,
    /// L'alignement peut encore être cassé par une capture adverse qui
    /// retire au moins une pierre de la ligne : la partie continue, avec un
    /// tour de grâce pour l'adversaire (voir `game::Game`).
    Pending,
}

/// Cases candidates pour une capture : vides et à distance de Tchebychev <= 2
/// d'au moins une pierre du plateau. Sur-ensemble large mais borné des coups
/// à essayer : la capture flanque une paire n'importe où sur le plateau, pas
/// nécessairement près de la ligne gagnante elle-même.
fn candidate_capture_squares(pos: &Position) -> Vec<usize> {
    let mut squares = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            let index = xy_to_index(x, y);
            if pos.is_empty(index) && pos.has_neighbour(index) {
                squares.push(index);
            }
        }
    }
    squares
}

/// Applique la règle de fin de partie de la section 3.4 : `player` vient de
/// former un alignement de 5+ passant par `winning_index`. `pos` doit être la
/// position **juste après** ce coup (donc `pos.to_move == opponent(player)`
/// en temps normal, mais cette fonction ne s'appuie jamais sur `pos.to_move`,
/// uniquement sur le paramètre `player` explicite).
pub fn resolve_alignment(pos: &Position, winning_index: usize, player: u8) -> AlignmentOutcome {
    let opp = opponent(player);

    // Clause explicite du sujet : si l'auteur de l'alignement a déjà perdu 4
    // paires (donc si l'ADVERSAIRE a déjà capturé 4 paires à `player` :
    // `pairs_captured` compte les paires prises PAR chaque joueur, pas les
    // paires perdues) et que l'adversaire a au moins un coup de capture
    // légal disponible, l'adversaire gagne immédiatement par capture.
    if pos.pairs_captured[opp as usize] >= 4 {
        for candidate in candidate_capture_squares(pos) {
            if is_move_legal_as(pos, candidate, opp)
                && !capturing_pairs(pos, candidate, opp).is_empty()
            {
                return AlignmentOutcome::OpponentWinsByCapture;
            }
        }
    }

    let winning_stones = pos.winning_stones_through(winning_index);

    for candidate in candidate_capture_squares(pos) {
        if !is_move_legal_as(pos, candidate, opp) {
            continue;
        }
        let pairs = capturing_pairs(pos, candidate, opp);
        if pairs.is_empty() {
            continue;
        }
        let touches_line = pairs
            .iter()
            .any(|&(a, b)| winning_stones.contains(&a) || winning_stones.contains(&b));
        if !touches_line {
            continue;
        }
        // Simule le coup pour vérifier qu'il casse effectivement TOUS les
        // alignements de 5+ que `player` possède actuellement (un coup peut
        // en théorie créer plusieurs lignes de 5 à la fois).
        let mut sim = pos.clone();
        sim.to_move = opp;
        sim.make_move(candidate);
        if !alignment_still_intact(&sim, &winning_stones, player) {
            return AlignmentOutcome::Pending;
        }
    }

    AlignmentOutcome::Win
}

/// Vrai si au moins une des pierres listées appartient encore à `player` et
/// forme toujours un alignement de 5+. Utilisé pour re-vérifier un
/// alignement "en attente" après le tour de grâce de l'adversaire.
pub fn alignment_still_intact(pos: &Position, stones: &[usize], player: u8) -> bool {
    stones
        .iter()
        .any(|&s| s < crate::game::position::TOTAL && pos.cells[s] == player && pos.forms_five(s))
}

/// Victoire par capture : 10 pierres adverses capturées, soit 5 paires.
pub fn has_won_by_capture(pos: &Position, player: u8) -> bool {
    pos.pairs_captured[player as usize] >= CAPTURE_PAIRS_TO_WIN
}

/// Match nul : le joueur au trait n'a plus aucun coup légal.
pub fn is_draw(pos: &Position) -> bool {
    legal_moves_naive(pos).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::{BLACK, WHITE};

    #[test]
    fn xx_trou_xx_est_legal_car_fait_cinq() {
        let mut pos = Position::new();
        pos.cells[xy_to_index(5, 5)] = BLACK;
        pos.cells[xy_to_index(6, 5)] = BLACK;
        pos.cells[xy_to_index(8, 5)] = BLACK;
        pos.cells[xy_to_index(9, 5)] = BLACK;
        pos.to_move = BLACK;
        let idx = xy_to_index(7, 5);
        assert!(
            is_move_legal(&pos, idx),
            "X X _ X X doit être légal : il forme un alignement de cinq"
        );
    }

    #[test]
    fn double_trois_simple_est_illegal() {
        let mut pos = Position::new();
        pos.cells[xy_to_index(8, 9)] = BLACK;
        pos.cells[xy_to_index(7, 9)] = BLACK;
        pos.cells[xy_to_index(9, 8)] = BLACK;
        pos.cells[xy_to_index(9, 7)] = BLACK;
        pos.to_move = BLACK;
        let idx = xy_to_index(9, 9);
        assert_eq!(check_move_legal(&pos, idx), Err(IllegalReason::DoubleTrois));
    }

    #[test]
    fn double_trois_autorise_si_le_coup_capture() {
        let mut pos = Position::new();
        // Croix de double-trois...
        pos.cells[xy_to_index(8, 9)] = BLACK;
        pos.cells[xy_to_index(7, 9)] = BLACK;
        pos.cells[xy_to_index(9, 8)] = BLACK;
        pos.cells[xy_to_index(9, 7)] = BLACK;
        // ... plus une capture disponible pour ce même coup (motif O O X).
        pos.cells[xy_to_index(10, 9)] = WHITE;
        pos.cells[xy_to_index(11, 9)] = WHITE;
        pos.cells[xy_to_index(12, 9)] = BLACK;
        pos.to_move = BLACK;
        let idx = xy_to_index(9, 9);
        assert!(!capturing_pairs(&pos, idx, BLACK).is_empty());
        assert!(is_move_legal(&pos, idx));
    }

    #[test]
    fn triple_trois_est_illegal() {
        let mut pos = Position::new();
        pos.cells[xy_to_index(8, 9)] = BLACK;
        pos.cells[xy_to_index(7, 9)] = BLACK;
        pos.cells[xy_to_index(9, 8)] = BLACK;
        pos.cells[xy_to_index(9, 7)] = BLACK;
        pos.cells[xy_to_index(10, 10)] = BLACK;
        pos.cells[xy_to_index(11, 11)] = BLACK;
        pos.to_move = BLACK;
        let idx = xy_to_index(9, 9);
        assert!(count_free_threes(&pos, idx, BLACK) >= 2);
        assert_eq!(check_move_legal(&pos, idx), Err(IllegalReason::DoubleTrois));
    }

    #[test]
    fn victoire_par_capture_a_cinq_paires() {
        let mut pos = Position::new();
        pos.pairs_captured[BLACK as usize] = 5;
        assert!(has_won_by_capture(&pos, BLACK));
        pos.pairs_captured[BLACK as usize] = 4;
        assert!(!has_won_by_capture(&pos, BLACK));
    }

    #[test]
    fn alignement_non_cassable_est_une_victoire() {
        let mut pos = Position::new();
        for x in 5..10 {
            pos.set_stone_for_test(xy_to_index(x, 5), BLACK);
        }
        let idx = xy_to_index(9, 5);
        assert!(pos.forms_five(idx));
        let outcome = resolve_alignment(&pos, idx, BLACK);
        assert_eq!(outcome, AlignmentOutcome::Win);
    }

    #[test]
    fn alignement_cassable_par_capture_reste_en_attente() {
        // Ligne noire horizontale de 5 en y=5, x=5..9. La pierre (5,5) (une
        // extrémité de la ligne) forme aussi, avec une pierre noire (5,4)
        // placée juste au-dessus, une paire VERTICALE capturable par Blanc :
        // Blanc joue en (5,3), avec (5,6) déjà blanc, motif Blanc-Noir-Noir-
        // Blanc sur l'axe vertical. Cette capture retire (5,4) et (5,5) : la
        // ligne horizontale de 5 perd une pierre et n'est plus qu'un
        // alignement de 4, donc cassée.
        let mut pos = Position::new();
        for x in 5..10 {
            pos.set_stone_for_test(xy_to_index(x, 5), BLACK);
        }
        pos.set_stone_for_test(xy_to_index(5, 4), BLACK);
        pos.set_stone_for_test(xy_to_index(5, 6), WHITE);
        pos.to_move = WHITE;

        let idx = xy_to_index(5, 3);
        assert!(is_move_legal(&pos, idx));
        let pairs = capturing_pairs(&pos, idx, WHITE);
        assert_eq!(pairs, vec![(xy_to_index(5, 4), xy_to_index(5, 5))]);

        let winning_index = xy_to_index(7, 5);
        assert!(pos.forms_five(winning_index));
        let outcome = resolve_alignment(&pos, winning_index, BLACK);
        assert_eq!(outcome, AlignmentOutcome::Pending);
    }

    #[test]
    fn quatre_paires_perdues_et_capture_disponible_fait_gagner_l_adversaire() {
        // Noir aligne 5, mais Blanc a déjà capturé 4 paires à Noir et dispose
        // d'un coup de capture supplémentaire : Blanc gagne immédiatement,
        // l'alignement de Noir ne compte pas.
        let mut pos = Position::new();
        for x in 5..10 {
            pos.set_stone_for_test(xy_to_index(x, 5), BLACK);
        }
        pos.pairs_captured[WHITE as usize] = 4;
        // Un coup de capture pour Blanc, sans rapport avec la ligne.
        pos.set_stone_for_test(xy_to_index(2, 2), BLACK);
        pos.set_stone_for_test(xy_to_index(3, 2), BLACK);
        pos.set_stone_for_test(xy_to_index(4, 2), WHITE);
        pos.to_move = WHITE;

        let winning_index = xy_to_index(7, 5);
        assert!(pos.forms_five(winning_index));
        let outcome = resolve_alignment(&pos, winning_index, BLACK);
        assert_eq!(outcome, AlignmentOutcome::OpponentWinsByCapture);
    }

    #[test]
    fn plateau_vide_nest_pas_nul() {
        let pos = Position::new();
        assert!(!is_draw(&pos));
    }
}
