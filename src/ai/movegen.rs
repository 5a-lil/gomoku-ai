//! Génération et ordonnancement des coups candidats.
//!
//! Deux responsabilités : ne proposer que des coups **légaux** (filtre le
//! double-trois dès la génération, la recherche n'a donc jamais besoin d'y
//! repenser), et les trier du plus prometteur au moins prometteur pour que
//! l'élagage alpha-bêta coupe le plus tôt possible. Voir aussi la limitation
//! au `k` premiers candidats plus bas : c'est ce qui ramène le facteur de
//! branchement d'environ 100 à 10-14, condition sine qua non pour atteindre
//! la profondeur 10 en moins de 500 ms (voir `DEFENSE.md`, section
//! "compromis assumés").

use crate::game::patterns::{virtual_best_run_score, would_form_five};
use crate::game::position::{opponent, xy_to_index, Position, BOARD_SIZE};
use crate::game::rules::{capturing_pairs, is_move_legal_as};

pub const K_SHALLOW: usize = 12;
pub const K_MID: usize = 8;
pub const K_DEEP: usize = 5;

/// Nombre de candidats conservés selon la distance à la racine (voir
/// `ai::search`, où `depth_from_root` est le nombre de demi-coups déjà
/// joués depuis le début de la recherche, pas la profondeur restante).
pub fn candidate_limit(depth_from_root: u32) -> usize {
    match depth_from_root {
        0 | 1 => K_SHALLOW,
        2 | 3 => K_MID,
        _ => K_DEEP,
    }
}

struct Scored {
    index: usize,
    score: i64,
    critical: bool,
}

/// Génère les coups légaux pour `player`, triés par score d'ordonnancement
/// décroissant, limités aux `limit` meilleurs. Les coups "critiques" (coup
/// gagnant, blocage d'un coup gagnant adverse, capture décisive) ne sont
/// **jamais** élagués, quel que soit `limit` : les en écarter ferait rater
/// des victoires ou des blocages forcés, ce qui n'est plus un compromis de
/// performance mais un bug tactique.
pub fn generate_ordered(
    pos: &Position,
    player: u8,
    tt_move: Option<usize>,
    killers: [Option<usize>; 2],
    history: &[i32],
    limit: usize,
) -> Vec<usize> {
    if pos.stone_count == 0 {
        // Plateau vide : le centre est le seul choix qui a un sens, inutile
        // de balayer les 361 cases pour le découvrir.
        return vec![xy_to_index(BOARD_SIZE / 2, BOARD_SIZE / 2)];
    }

    let opp = opponent(player);

    // Étape 1 (bon marché) : score purement positionnel sur toutes les cases
    // candidates, SANS vérifier la légalité complète (qui implique la
    // détection de double-trois, nettement plus coûteuse). Un coup gagnant,
    // bloquant ou capturant obtient déjà un score élevé via
    // `virtual_best_run_score` (100 000 pour un cinq/quatre libre) : il
    // remonte donc naturellement en tête de ce tri préliminaire.
    let mut prelim: Vec<(usize, i64)> = Vec::with_capacity(48);
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            let index = xy_to_index(x, y);
            if !pos.is_empty(index) || !pos.has_neighbour(index) {
                continue;
            }
            let mut score: i64 = 0;
            if tt_move == Some(index) {
                score += 10_000_000;
            }
            score += virtual_best_run_score(pos, index, player) as i64 * 10;
            score += virtual_best_run_score(pos, index, opp) as i64 * 8;
            if killers[0] == Some(index) || killers[1] == Some(index) {
                score += 100_000;
            }
            score += *history.get(index).unwrap_or(&0) as i64;
            prelim.push((index, score));
        }
    }
    prelim.sort_unstable_by(|a, b| b.1.cmp(&a.1));

    // Étape 2 (coûteuse) : ne vérifier la légalité complète (et calculer le
    // score final, avec captures) que sur un sur-ensemble large des
    // meilleurs candidats préliminaires. Filet de sécurité : un coup qui
    // capturerait la 5e paire (victoire immédiate par capture) est ajouté
    // explicitement même s'il n'apparaît pas dans ce sur-ensemble, car sa
    // valeur positionnelle seule ne le garantit pas toujours en tête.
    let buffer_size = (limit * 3 + 12).min(prelim.len());
    let mut buffer_indices: Vec<usize> = prelim.iter().take(buffer_size).map(|&(i, _)| i).collect();
    for &(index, _) in prelim.iter().skip(buffer_size) {
        let pairs = capturing_pairs(pos, index, player);
        if !pairs.is_empty()
            && (pos.pairs_captured[player as usize] as usize + pairs.len()) >= 5
            && !buffer_indices.contains(&index)
        {
            buffer_indices.push(index);
        }
    }

    let mut scored: Vec<Scored> = Vec::with_capacity(buffer_indices.len());
    for index in buffer_indices {
        if !is_move_legal_as(pos, index, player) {
            continue;
        }

        let mut score: i64 = 0;
        let mut critical = false;

        if tt_move == Some(index) {
            score += 10_000_000;
            critical = true;
        }
        if would_form_five(pos, index, player) {
            score += 9_000_000;
            critical = true;
        }
        if would_form_five(pos, index, opp) {
            score += 8_000_000;
            critical = true;
        }
        let pairs = capturing_pairs(pos, index, player);
        if !pairs.is_empty() && (pos.pairs_captured[player as usize] as usize + pairs.len()) >= 5 {
            score += 7_000_000;
            critical = true;
        }
        score += virtual_best_run_score(pos, index, player) as i64 * 10;
        score += virtual_best_run_score(pos, index, opp) as i64 * 8;
        score += pairs.len() as i64 * 50_000;
        if killers[0] == Some(index) || killers[1] == Some(index) {
            score += 100_000;
        }
        score += *history.get(index).unwrap_or(&0) as i64;

        scored.push(Scored { index, score, critical });
    }

    scored.sort_unstable_by(|a, b| b.score.cmp(&a.score));

    if scored.len() <= limit {
        return scored.into_iter().map(|s| s.index).collect();
    }

    scored
        .iter()
        .enumerate()
        .filter(|(i, s)| *i < limit || s.critical)
        .map(|(_, s)| s.index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::{BLACK, WHITE};

    fn no_killers() -> [Option<usize>; 2] {
        [None, None]
    }

    #[test]
    fn plateau_vide_ne_propose_que_le_centre() {
        let pos = Position::new();
        let moves = generate_ordered(&pos, BLACK, None, no_killers(), &[], 20);
        assert_eq!(moves, vec![xy_to_index(BOARD_SIZE / 2, BOARD_SIZE / 2)]);
    }

    #[test]
    fn coup_gagnant_est_toujours_propose_meme_hors_limite() {
        let mut pos = Position::new();
        for x in 5..9 {
            pos.set_stone_for_test(xy_to_index(x, 5), BLACK);
        }
        // Beaucoup de bruit autour pour forcer un élagage à `limit`.
        for i in 0..30 {
            let x = i % BOARD_SIZE;
            let y = (i / BOARD_SIZE) + 10;
            if y < BOARD_SIZE {
                pos.set_stone_for_test(xy_to_index(x, y), WHITE);
            }
        }
        pos.to_move = BLACK;
        let moves = generate_ordered(&pos, BLACK, None, no_killers(), &[], 1);
        assert!(moves.contains(&xy_to_index(9, 5)), "le coup gagnant doit être proposé");
    }

    #[test]
    fn double_trois_est_exclu_des_candidats() {
        let mut pos = Position::new();
        pos.set_stone_for_test(xy_to_index(8, 9), BLACK);
        pos.set_stone_for_test(xy_to_index(7, 9), BLACK);
        pos.set_stone_for_test(xy_to_index(9, 8), BLACK);
        pos.set_stone_for_test(xy_to_index(9, 7), BLACK);
        pos.to_move = BLACK;
        let moves = generate_ordered(&pos, BLACK, None, no_killers(), &[], 100);
        assert!(!moves.contains(&xy_to_index(9, 9)));
    }

    #[test]
    fn coup_de_blocage_est_bien_priorise() {
        let mut pos = Position::new();
        for x in 5..9 {
            pos.set_stone_for_test(xy_to_index(x, 5), WHITE);
        }
        pos.to_move = BLACK;
        let moves = generate_ordered(&pos, BLACK, None, no_killers(), &[], 5);
        assert!(moves.contains(&xy_to_index(9, 5)) || moves.contains(&xy_to_index(4, 5)));
    }
}
