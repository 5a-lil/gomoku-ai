//! Classification des motifs d'alignement.
//!
//! Deux besoins distincts sont couverts ici :
//!
//! 1. **Score positionnel** (`line_contribution`) : pour l'évaluation de
//!    l'IA, on balaie chaque ligne du plateau une seule fois et on note
//!    chaque alignement contigu rencontré selon sa longueur et l'ouverture de
//!    ses extrémités (`run_score`). Les bornes de chaque ligne sont calculées
//!    par arithmétique pure (pas de recherche de bord case par case), ce qui
//!    rend le recalcul d'une ligne O(longueur de la ligne) et non O(n²).
//!
//! 2. **Détection du trois libre** (`free_three_axes`) : nécessaire pour
//!    interdire le double-trois (section 3.5 du sujet). Un trois libre est
//!    un alignement de 3 pierres qui peut devenir un quatre aux deux
//!    extrémités libres. Le sujet en donne deux familles de motifs : le trois
//!    contigu (`_XXX__`, `__XXX_`) et le trois "à trou" (`_XX_X_`, `_X_XX_`).
//!    On teste les 4 gabarits directement sur des fenêtres de 6 cases, en
//!    simulant la pose de la pierre sans jamais muter le plateau réel.

use crate::game::position::{
    index_to_xy, xy_to_index, Position, AXES, BLACK, BOARD_SIZE, EMPTY, WALL,
};

/// Score d'un alignement contigu de `len` pierres (plafonné à 5 : un
/// alignement de 5+ est une victoire immédiate traitée directement par
/// `game::rules`, ce barème ne sert qu'à guider l'heuristique en cours de
/// partie). `left_open`/`right_open` indiquent si l'extrémité correspondante
/// est une case vide (donc si l'alignement peut encore s'étendre).
///
/// Valeurs choisies (voir `DEFENSE.md` pour la justification et les mesures) :
/// un quatre libre est indéfendable donc quasi décisif ; un trois libre vaut
/// dix fois un trois fermé, car lui seul menace réellement de devenir un
/// quatre libre.
pub fn run_score(len: i32, left_open: bool, right_open: bool) -> i32 {
    let len = len.min(5);
    let open_ends = left_open as i32 + right_open as i32;
    match (len, open_ends) {
        (5, _) => 100_000,
        (4, 2) => 100_000, // quatre libre : indéfendable, presque une victoire
        (4, 1) => 10_000,  // quatre simple : force une réponse immédiate
        (4, 0) => 0,       // quatre mort : ne peut plus jamais devenir cinq
        (3, 2) => 5_000,   // trois libre : menace de quatre libre
        (3, 1) => 500,     // trois fermé : beaucoup moins dangereux
        (3, 0) => 0,
        (2, 2) => 200,
        (2, 1) => 50,
        (2, 0) => 0,
        (1, 2) => 10,
        _ => 0,
    }
}

/// Point de départ de la ligne passant par `(x, y)` sur l'axe `axis` (index
/// dans `AXES`), calculé arithmétiquement (aucune boucle) : c'est le point de
/// cette ligne le plus proche du bord dans le sens négatif de l'axe.
fn line_start(x: usize, y: usize, axis: usize) -> (usize, usize) {
    match axis {
        0 => (0, y),        // horizontale : le bord gauche de la rangée
        1 => (x, 0),        // verticale : le haut de la colonne
        2 => {
            // diagonale \ : remonter vers le coin haut-gauche
            let k = x.min(y);
            (x - k, y - k)
        }
        _ => {
            // diagonale / : remonter vers le coin haut-droit
            let k = (BOARD_SIZE - 1 - x).min(y);
            (x + k, y - k)
        }
    }
}

/// Contribution au score global (positive si favorable à Noir) de la ligne
/// complète passant par `index` sur l'axe `axis`. Balaie la ligne une seule
/// fois de bout en bout, détecte chaque alignement contigu et cumule son
/// score signé via `run_score`.
pub fn line_contribution(pos: &Position, index: usize, axis: usize) -> i32 {
    let Some((x, y)) = index_to_xy(index) else {
        return 0;
    };
    let d = AXES[axis];
    let (sx, sy) = line_start(x, y, axis);
    let start = xy_to_index(sx, sy) as i32;

    let mut total = 0i32;
    let mut p = start;
    loop {
        let c = pos.cell_at(p);
        if c == WALL {
            break;
        }
        if c == EMPTY {
            p += d;
            continue;
        }
        let color = c;
        let left_open = pos.cell_at(p - d) == EMPTY;
        let mut len = 0;
        while pos.cell_at(p) == color {
            len += 1;
            p += d;
        }
        let right_open = pos.cell_at(p) == EMPTY;
        let score = run_score(len, left_open, right_open);
        total += if color == BLACK { score } else { -score };
    }
    total
}

/// Les 4 gabarits canoniques de trois libre, sur une fenêtre de 6 cases :
/// `1` = doit être une pierre du joueur, `0` = doit être une case vide. Toute
/// pierre adverse ou tout mur à l'une de ces positions invalide le motif,
/// exactement comme l'exige la définition du sujet (le trois doit pouvoir
/// devenir un quatre aux deux bouts libres).
const FREE_THREE_TEMPLATES: [[u8; 6]; 4] = [
    [0, 1, 1, 1, 0, 0], // _XXX__
    [0, 0, 1, 1, 1, 0], // __XXX_
    [0, 1, 1, 0, 1, 0], // _XX_X_
    [0, 1, 0, 1, 1, 0], // _X_XX_
];

/// Lit la case à l'index signé `p`, en substituant virtuellement `player` à
/// la position `virtual_index`. Permet de tester la légalité d'un coup avant
/// de le jouer réellement, sans muter la position.
#[inline(always)]
fn cell_with_virtual(pos: &Position, p: i32, virtual_index: usize, player: u8) -> u8 {
    if p == virtual_index as i32 {
        player
    } else {
        pos.cell_at(p)
    }
}

/// Vrai si jouer `player` en `index` (case vide sur le plateau réel) crée un
/// trois libre sur l'axe `axis`, la pierre jouée devant faire partie du
/// motif détecté (un trois libre préexistant ailleurs sur la ligne, qui ne
/// passe pas par `index`, ne compte pas : ce n'est pas ce coup qui l'a créé).
fn creates_free_three_on_axis(pos: &Position, index: usize, axis: usize, player: u8) -> bool {
    let d = AXES[axis];
    let idx = index as i32;
    for offset in -5..=0i32 {
        let base = idx + offset * d;
        let rel = -offset; // position de `index` dans cette fenêtre de 6 (doit être 0..6)
        if !(0..6).contains(&rel) {
            continue;
        }
        for template in FREE_THREE_TEMPLATES.iter() {
            if template[rel as usize] != 1 {
                continue; // le coup doit occuper une case 'X' du gabarit, pas une case vide
            }
            let mut matches = true;
            for (k, &want_stone) in template.iter().enumerate() {
                let p = base + k as i32 * d;
                let cell = cell_with_virtual(pos, p, index, player);
                let is_stone = want_stone == 1;
                if is_stone && cell != player {
                    matches = false;
                    break;
                }
                if !is_stone && cell != EMPTY {
                    matches = false;
                    break;
                }
            }
            if matches {
                return true;
            }
        }
    }
    false
}

/// Vrai si jouer `player` en `index` (case vide) formerait un alignement de
/// 5 pierres ou plus, sans muter la position. Sert à court-circuiter la règle
/// du double-trois : un coup qui gagne immédiatement est toujours légal (voir
/// `game::rules::check_move_legal_as`).
pub fn would_form_five(pos: &Position, index: usize, player: u8) -> bool {
    let idx = index as i32;
    for &d in AXES.iter() {
        let mut count = 1;
        let mut p = idx - d;
        while cell_with_virtual(pos, p, index, player) == player {
            count += 1;
            p -= d;
        }
        let mut p = idx + d;
        while cell_with_virtual(pos, p, index, player) == player {
            count += 1;
            p += d;
        }
        if count >= 5 {
            return true;
        }
    }
    false
}

/// Meilleur score de `run_score` obtenu en simulant la pose de `player` en
/// `index` (case vide), en prenant le maximum sur les 4 axes. Sert à la fois
/// à l'évaluation locale d'un coup et à l'ordonnancement des candidats dans
/// `ai::movegen` : un même barème (`run_score`) est ainsi réutilisé pour
/// noter aussi bien "quelle menace ce coup crée-t-il pour moi" que "quelle
/// menace ce coup bloque-t-il pour l'adversaire" (en appelant cette fonction
/// avec la couleur adverse).
pub fn virtual_best_run_score(pos: &Position, index: usize, player: u8) -> i32 {
    let idx = index as i32;
    let mut best = 0;
    for &d in AXES.iter() {
        let mut count = 1;
        let mut p = idx - d;
        while cell_with_virtual(pos, p, index, player) == player {
            count += 1;
            p -= d;
        }
        let left_open = cell_with_virtual(pos, p, index, player) == EMPTY;
        let mut p = idx + d;
        while cell_with_virtual(pos, p, index, player) == player {
            count += 1;
            p += d;
        }
        let right_open = cell_with_virtual(pos, p, index, player) == EMPTY;
        best = best.max(run_score(count, left_open, right_open));
    }
    best
}

/// Pour chacun des 4 axes, vrai si jouer `player` en `index` y crée un trois
/// libre. Le comptage se fait par axe (jamais par direction/8), comme
/// l'exige le sujet : un axe ne peut produire qu'un seul "trois libre" à la
/// fois, gauche et droite étant la même ligne.
pub fn free_three_axes(pos: &Position, index: usize, player: u8) -> [bool; 4] {
    let mut result = [false; 4];
    for axis in 0..AXES.len() {
        result[axis] = creates_free_three_on_axis(pos, index, axis, player);
    }
    result
}

pub fn count_free_threes(pos: &Position, index: usize, player: u8) -> u32 {
    free_three_axes(pos, index, player).iter().filter(|&&b| b).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::{xy_to_index, BLACK as B, WHITE as W};

    fn empty_board() -> Position {
        Position::new()
    }

    #[test]
    fn run_score_ordre_croissant_par_menace() {
        assert!(run_score(2, true, true) > run_score(2, true, false));
        assert!(run_score(3, true, true) > run_score(3, true, false));
        assert!(run_score(3, true, true) > run_score(2, true, true));
        assert!(run_score(4, true, true) > run_score(3, true, true));
        assert_eq!(run_score(3, false, false), 0);
        assert_eq!(run_score(4, false, false), 0);
    }

    #[test]
    fn contigu_ouvert_xxx_est_un_trois_libre() {
        let mut pos = empty_board();
        pos.cells[xy_to_index(5, 5)] = B;
        pos.cells[xy_to_index(6, 5)] = B;
        // On teste si jouer en (7,5) crée un trois libre horizontal.
        let idx = xy_to_index(7, 5);
        let axes = free_three_axes(&pos, idx, B);
        assert!(axes[0], "attendu : trois libre sur l'axe horizontal");
    }

    #[test]
    fn trois_ferme_par_un_bord_nest_pas_libre() {
        let mut pos = empty_board();
        // Mur simulé par la bordure du plateau : on colle le motif au bord.
        pos.cells[xy_to_index(0, 5)] = B;
        pos.cells[xy_to_index(1, 5)] = B;
        let idx = xy_to_index(2, 5);
        let axes = free_three_axes(&pos, idx, B);
        // Le côté gauche est le bord du plateau (mur), donc fermé : pas un
        // trois libre.
        assert!(!axes[0]);
    }

    #[test]
    fn trois_ferme_par_pierre_adverse_nest_pas_libre() {
        let mut pos = empty_board();
        pos.cells[xy_to_index(4, 5)] = W; // bloque le côté gauche
        pos.cells[xy_to_index(5, 5)] = B;
        pos.cells[xy_to_index(6, 5)] = B;
        let idx = xy_to_index(7, 5);
        let axes = free_three_axes(&pos, idx, B);
        assert!(!axes[0]);
    }

    #[test]
    fn trois_a_trou_xx_x_est_un_trois_libre() {
        let mut pos = empty_board();
        pos.cells[xy_to_index(5, 5)] = B;
        pos.cells[xy_to_index(6, 5)] = B;
        pos.cells[xy_to_index(8, 5)] = B;
        // (7,5) est vide ; jouer ailleurs ne teste rien ici : on vérifie
        // simplement que le motif _XX_X_ est bien reconnu comme trois libre,
        // en interrogeant l'axe pour la pierre en (8,5) déjà posée : on la
        // "rejoue" virtuellement au même endroit pour la classification.
        let idx = xy_to_index(8, 5);
        // Simule un plateau où (8,5) est encore vide pour tester la création.
        pos.cells[idx] = EMPTY;
        let axes = free_three_axes(&pos, idx, B);
        assert!(axes[0], "attendu : _XX_X_ est un trois libre");
    }

    #[test]
    fn double_trois_detecte_sur_deux_axes() {
        let mut pos = empty_board();
        // Croix centrée sur (9,9) : deux pierres horizontales et deux
        // verticales, la pierre jouée au centre complète un trois libre sur
        // chaque axe.
        pos.cells[xy_to_index(8, 9)] = B;
        pos.cells[xy_to_index(7, 9)] = B;
        pos.cells[xy_to_index(9, 8)] = B;
        pos.cells[xy_to_index(9, 7)] = B;
        let idx = xy_to_index(9, 9);
        assert_eq!(count_free_threes(&pos, idx, B), 2);
    }

    #[test]
    fn xx_trou_xx_ne_doit_pas_etre_confondu_avec_un_trois_libre_simple() {
        // Sert de garde-fou documentaire : ce motif particulier (X X _ X X)
        // est traité par game::rules (forms_five court-circuite le
        // double-trois), pas ici. On vérifie juste que la détection de trois
        // libre elle-même ne plante pas sur ce plateau.
        let mut pos = empty_board();
        pos.cells[xy_to_index(5, 5)] = B;
        pos.cells[xy_to_index(6, 5)] = B;
        pos.cells[xy_to_index(8, 5)] = B;
        pos.cells[xy_to_index(9, 5)] = B;
        let idx = xy_to_index(7, 5);
        let _ = free_three_axes(&pos, idx, B); // ne doit pas paniquer
    }
}
