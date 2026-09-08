//! Évaluation heuristique d'une position : la fonction dont dépend toute la
//! qualité de jeu de l'IA (voir `DEFENSE.md` pour la discussion complète).
//!
//! Toujours exprimée du point de vue **du joueur au trait** (convention
//! "negamax") : une valeur positive signifie que la position favorise le
//! joueur qui doit jouer. La quasi-totalité du score vient de
//! `Position::score`, maintenu incrémentalement par `game::position` (voir
//! `game::patterns::line_contribution`) : cette fonction se contente d'y
//! ajouter deux termes qui ne se prêtent pas à un maintien incrémental
//! simple (progression des captures, vulnérabilité du dernier coup).

use crate::game::position::{opponent, Position, BLACK, DIRS8, EMPTY, WHITE};

/// Score conventionnel pour "victoire certaine". Les scores de mat réels
/// sont `WIN - ply` (voir `ai::search`) afin de préférer les victoires
/// rapides ; `INF` sert de borne pour les fenêtres alpha-bêta initiales,
/// toujours strictement au-delà de tout score de mat possible.
pub const INF: i32 = 1_000_000_000;
pub const WIN: i32 = 900_000_000;

/// Score additionnel par nombre de paires déjà capturées (0 à 4 ; 5 est une
/// victoire immédiate traitée par `game::rules`, jamais atteinte ici).
/// Progression volontairement non linéaire : chaque paire supplémentaire
/// rapproche dangereusement de la victoire à 5 paires (section 3.3 du
/// sujet), donc son poids doit croître plus vite qu'un simple compteur
/// linéaire pour que l'IA priorise réellement la 4e paire.
const CAPTURE_SCORE: [i32; 5] = [0, 1_500, 4_000, 9_000, 25_000];

/// Pénalité par paire du joueur qui vient de jouer, immédiatement capturable
/// par l'adversaire (motif "vide-joueur-joueur-adversaire" sur un axe).
/// Limité au dernier coup joué (voir le commentaire de `vulnerability_term`)
/// : c'est une simplification assumée, documentée dans `DEFENSE.md`.
const VULNERABILITY_PENALTY: i32 = 800;

fn capture_term(pos: &Position, player: u8) -> i32 {
    let n = (pos.pairs_captured[player as usize] as usize).min(CAPTURE_SCORE.len() - 1);
    CAPTURE_SCORE[n]
}

/// Pénalise, pour `player`, le fait que son dernier coup ait créé une paire
/// immédiatement capturable par l'adversaire.
///
/// Limitation assumée : seule la paire impliquant la pierre du DERNIER coup
/// est vérifiée, pas l'intégralité du plateau (un balayage complet coûterait
/// à nouveau O(361) par feuille, ce qui annulerait le gain de l'évaluation
/// incrémentale). En pratique, c'est le cas le plus fréquent et le plus
/// exploitable tactiquement : une vulnérabilité plus ancienne, non corrigée,
/// réapparaît de toute façon dans l'évaluation dès qu'un coup la retouche.
fn vulnerability_term(pos: &Position, player: u8) -> i32 {
    let Some(last) = pos.last_move else {
        return 0;
    };
    if pos.cells[last] != player {
        return 0;
    }
    let opp = opponent(player);
    let idx = last as i32;
    let mut count = 0;
    for &d in DIRS8.iter() {
        if pos.cell_at(idx + d) == player
            && pos.cell_at(idx - d) == EMPTY
            && pos.cell_at(idx + 2 * d) == opp
        {
            count += 1;
        }
    }
    count * VULNERABILITY_PENALTY
}

/// Évalue `pos` du point de vue du joueur au trait. Ne présuppose pas que
/// `pos` est terminale : c'est `ai::search` qui vérifie l'alignement de 5+,
/// la victoire par capture et le match nul *avant* d'appeler `evaluate`,
/// pour attribuer les scores de victoire exacts (`WIN - ply`) plutôt qu'une
/// simple grande valeur heuristique.
pub fn evaluate(pos: &Position) -> i32 {
    let black_relative = pos.score + capture_term(pos, BLACK) - capture_term(pos, WHITE)
        - vulnerability_term(pos, BLACK)
        + vulnerability_term(pos, WHITE);

    if pos.to_move == BLACK {
        black_relative
    } else {
        -black_relative
    }
}

/// Décomposition de l'évaluation par catégorie, du point de vue de Noir
/// (positif favorise Noir), destinée exclusivement au panneau de débogage de
/// l'interface (touche « détail du score ») : ce n'est jamais consulté par
/// la recherche elle-même, qui n'utilise que [`evaluate`].
#[derive(Debug, Clone, Copy)]
pub struct Breakdown {
    /// `Position::score` : somme des fenêtres de motifs, point de vue Noir.
    pub positional_black: i32,
    pub capture_black: i32,
    pub capture_white: i32,
    pub vulnerability_black: i32,
    pub vulnerability_white: i32,
    /// Total du point de vue de Noir (positif favorise Noir).
    pub total_black: i32,
    /// Même total, mais du point de vue du joueur actuellement au trait
    /// (c'est cette valeur que la recherche utilise réellement).
    pub total_mover: i32,
}

pub fn breakdown(pos: &Position) -> Breakdown {
    let positional_black = pos.score;
    let capture_black = capture_term(pos, BLACK);
    let capture_white = capture_term(pos, WHITE);
    let vulnerability_black = vulnerability_term(pos, BLACK);
    let vulnerability_white = vulnerability_term(pos, WHITE);
    let total_black =
        positional_black + capture_black - capture_white - vulnerability_black + vulnerability_white;
    let total_mover = if pos.to_move == BLACK { total_black } else { -total_black };
    Breakdown {
        positional_black,
        capture_black,
        capture_white,
        vulnerability_black,
        vulnerability_white,
        total_black,
        total_mover,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::xy_to_index;

    #[test]
    fn position_vide_est_neutre() {
        let pos = Position::new();
        assert_eq!(evaluate(&pos), 0);
    }

    #[test]
    fn avoir_un_trois_libre_de_plus_favorise_le_joueur_au_trait() {
        let mut pos = Position::new();
        pos.set_stone_for_test(xy_to_index(9, 9), BLACK);
        pos.set_stone_for_test(xy_to_index(10, 9), BLACK);
        pos.set_stone_for_test(xy_to_index(11, 9), BLACK);
        pos.to_move = BLACK;
        assert!(evaluate(&pos) > 0, "Noir doit être favorisé et c'est à lui de jouer");
        pos.to_move = WHITE;
        assert!(evaluate(&pos) < 0, "du point de vue de Blanc, la même position est défavorable");
    }

    #[test]
    fn quatre_libre_vaut_beaucoup_plus_qu_un_trois_libre() {
        let mut trois = Position::new();
        trois.set_stone_for_test(xy_to_index(9, 9), BLACK);
        trois.set_stone_for_test(xy_to_index(10, 9), BLACK);
        trois.set_stone_for_test(xy_to_index(11, 9), BLACK);
        trois.to_move = BLACK;

        let mut quatre = Position::new();
        quatre.set_stone_for_test(xy_to_index(9, 9), BLACK);
        quatre.set_stone_for_test(xy_to_index(10, 9), BLACK);
        quatre.set_stone_for_test(xy_to_index(11, 9), BLACK);
        quatre.set_stone_for_test(xy_to_index(12, 9), BLACK);
        quatre.to_move = BLACK;

        assert!(evaluate(&quatre) > evaluate(&trois) * 5);
    }

    #[test]
    fn capture_term_est_croissant_et_convexe() {
        let mut pos = Position::new();
        let mut previous_gain = 0;
        let mut previous_score = evaluate(&pos);
        for n in 1..=4u8 {
            pos.pairs_captured[BLACK as usize] = n;
            let score = evaluate(&pos);
            let gain = score - previous_score;
            assert!(gain > previous_gain, "le gain marginal doit croître avec le nombre de paires");
            previous_gain = gain;
            previous_score = score;
        }
    }

    #[test]
    fn coup_qui_expose_une_paire_est_penalise() {
        // Noir vient de jouer en (5,5), formant une paire (4,5)-(5,5) avec
        // (3,5) déjà blanc d'un côté : si (6,5) est vide, Blanc peut capturer
        // en y jouant. La position doit être moins bonne pour Noir que la
        // même position sans cette vulnérabilité (Blanc absent en (3,5)).
        let mut vulnerable = Position::new();
        vulnerable.set_stone_for_test(xy_to_index(3, 5), WHITE);
        vulnerable.set_stone_for_test(xy_to_index(4, 5), BLACK);
        vulnerable.to_move = BLACK;
        vulnerable.make_move(xy_to_index(5, 5));

        let mut safe = Position::new();
        safe.set_stone_for_test(xy_to_index(4, 5), BLACK);
        safe.to_move = BLACK;
        safe.make_move(xy_to_index(5, 5));

        // Les deux positions sont évaluées du point de vue de Blanc (au
        // trait après le coup de Noir) : la version vulnérable doit être
        // relativement meilleure pour Blanc (donc un score plus grand, côté
        // Blanc) que la version sûre.
        assert!(evaluate(&vulnerable) > evaluate(&safe));
    }
}

#[cfg(test)]
mod bench {
    use super::*;
    use crate::game::position::xy_to_index;
    use std::time::Instant;

    /// Mesure le coût réel d'un appel à `evaluate` sur une position de
    /// milieu de partie typique, pour le comparer au chiffre mesuré sur
    /// l'ancienne implémentation (1,3 µs, voir `DEFENSE.md`, section
    /// "évaluation incrémentale"). `#[ignore]` par défaut : c'est une
    /// mesure de performance, pas une assertion de correction ; invocation
    /// : `cargo test --release ai::eval::bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn cout_de_evaluate() {
        let mut pos = Position::new();
        let stones: [(usize, usize, u8); 16] = [
            (6, 6, BLACK), (7, 10, BLACK), (8, 13, BLACK), (9, 11, BLACK),
            (10, 8, BLACK), (11, 12, BLACK), (12, 7, BLACK), (13, 9, BLACK),
            (6, 9, WHITE), (7, 7, WHITE), (8, 12, WHITE), (9, 8, WHITE),
            (10, 11, WHITE), (11, 13, WHITE), (12, 10, WHITE), (13, 6, WHITE),
        ];
        for (x, y, c) in stones {
            pos.set_stone_for_test(xy_to_index(x, y), c);
        }

        const N: u32 = 5_000_000;
        let t0 = Instant::now();
        let mut acc: i64 = 0;
        for _ in 0..N {
            acc += evaluate(&pos) as i64;
        }
        let elapsed = t0.elapsed();
        let per_call = elapsed / N;
        println!(
            "evaluate(): {N} appels en {elapsed:?}, soit {per_call:?}/appel (acc={acc}, pour éviter que le compilateur n'élimine la boucle)"
        );
        assert!(per_call.as_nanos() < 1_300, "evaluate() ne doit plus coûter les 1,3 µs de l'ancienne implémentation");
    }
}
