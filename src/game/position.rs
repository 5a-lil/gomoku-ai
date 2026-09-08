//! Représentation compacte du plateau de jeu et primitives de bas niveau.
//!
//! Le plateau est "matelassé" (padded) : la grille réelle de 19x19 intersections
//! est plongée au centre d'une grille de 27x27 dont la bordure de 4 cases est
//! remplie de sentinelles `WALL`. Une pierre ne peut jamais être posée à moins
//! de 4 cases du bord logique, donc tout accès `index ± k*direction` avec
//! `k <= 4` reste dans le tableau. Cela élimine les tests de bornes/modulo que
//! nécessitait l'ancienne implémentation (`check_up_right` et consorts) et
//! fait tenir le plateau entier (729 octets) dans le cache L1, contre 141 Ko
//! pour l'ancienne structure (`Cell` faisait 400 octets à cause des tables de
//! capture par case). C'est ce changement de représentation qui rend une
//! profondeur de recherche de 10 atteignable sous 500 ms.
//!
//! Par prudence (le sujet interdit tout crash, sans aucune exception), tous
//! les accès dérivés d'un calcul d'index utilisent [`Position::cell_at`], qui
//! renvoie `WALL` pour tout index hors tableau au lieu de paniquer. La preuve
//! géométrique que ce cas ne devrait jamais se produire est documentée
//! ci-dessous, mais on ne parie pas la note du projet dessus.

use std::sync::OnceLock;

pub const BOARD_SIZE: usize = 19;
pub const PAD: usize = 4;
pub const STRIDE: usize = BOARD_SIZE + 2 * PAD; // 27
pub const TOTAL: usize = STRIDE * STRIDE; // 729

pub const EMPTY: u8 = 0;
pub const BLACK: u8 = 1;
pub const WHITE: u8 = 2;
pub const WALL: u8 = 3;

/// Les 4 axes de jeu. Chaque axe est représenté par un unique décalage
/// "canonique" ; on parcourt l'axe complet en allant de -k*d à +k*d autour
/// d'une case.
pub const AXES: [i32; 4] = [
    1,                     // horizontale
    STRIDE as i32,         // verticale
    STRIDE as i32 + 1,     // diagonale \ (descendante vers la droite)
    STRIDE as i32 - 1,     // diagonale / (descendante vers la gauche)
];

/// Les 8 directions individuelles, utilisées pour la détection de capture.
pub const DIRS8: [i32; 8] = [
    1, -1,
    STRIDE as i32, -(STRIDE as i32),
    STRIDE as i32 + 1, -(STRIDE as i32 + 1),
    STRIDE as i32 - 1, -(STRIDE as i32 - 1),
];

#[inline(always)]
pub const fn xy_to_index(x: usize, y: usize) -> usize {
    (y + PAD) * STRIDE + (x + PAD)
}

/// Convertit un index de tableau en coordonnées de jeu, si l'index tombe bien
/// dans la zone jouable (pas dans la bordure).
#[inline(always)]
pub fn index_to_xy(index: usize) -> Option<(usize, usize)> {
    if index >= TOTAL {
        return None;
    }
    let row = index / STRIDE;
    let col = index % STRIDE;
    let x = col.checked_sub(PAD).filter(|&v| v < BOARD_SIZE);
    let y = row.checked_sub(PAD).filter(|&v| v < BOARD_SIZE);
    match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    }
}

#[inline(always)]
pub const fn opponent(player: u8) -> u8 {
    // BLACK=1, WHITE=2 => 3-player bascule l'un vers l'autre.
    3 - player
}

/// Nom lisible d'une intersection, par ex. "K10", pour l'affichage (variante
/// principale, suggestions, logs). Convention proche du Go : colonnes A-T en
/// sautant le 'I', lignes numérotées à partir de 1.
pub fn coord_name(index: usize) -> String {
    match index_to_xy(index) {
        Some((x, y)) => {
            const LETTERS: &[u8] = b"ABCDEFGHJKLMNOPQRST";
            let letter = LETTERS.get(x).copied().unwrap_or(b'?') as char;
            format!("{}{}", letter, y + 1)
        }
        None => String::from("??"),
    }
}

/// Clés de Zobrist, générées une seule fois avec un générateur pseudo-aléatoire
/// déterministe (splitmix64, graine fixe) afin que les tests soient
/// reproductibles d'une exécution à l'autre et d'une machine à l'autre.
pub struct ZobristKeys {
    pub stones: [[u64; TOTAL]; 3], // index 0 inutilisé ; 1=noir, 2=blanc
    pub side: u64,
    pub captures: [[u64; 6]; 3],   // clé par nombre de paires capturées 0..=5 ; index 0 inutilisé
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn build_zobrist() -> ZobristKeys {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut stones = [[0u64; TOTAL]; 3];
    for player in [BLACK as usize, WHITE as usize] {
        for slot in stones[player].iter_mut() {
            *slot = splitmix64(&mut seed);
        }
    }
    let side = splitmix64(&mut seed);
    let mut captures = [[0u64; 6]; 3];
    for player in [BLACK as usize, WHITE as usize] {
        for slot in captures[player].iter_mut() {
            *slot = splitmix64(&mut seed);
        }
    }
    ZobristKeys { stones, side, captures }
}

pub fn zobrist() -> &'static ZobristKeys {
    static KEYS: OnceLock<ZobristKeys> = OnceLock::new();
    KEYS.get_or_init(build_zobrist)
}

/// Annule un coup joué avec [`Position::make_move`]. Conserve tout ce qu'il
/// faut pour revenir exactement à l'état précédent : jusqu'à une paire
/// capturée par direction (8 directions), le score et le hachage d'avant
/// coup (moins cher à sauvegarder qu'à recalculer par delta inverse), et le
/// nombre de paires déjà capturées par l'auteur du coup.
#[derive(Debug, Clone, Copy)]
pub struct MoveUndo {
    pub index: usize,
    player: u8,
    captured: [Option<(usize, usize)>; 8],
    prev_score: i32,
    prev_zobrist: u64,
    prev_pairs_captured: u8,
    prev_last_move: Option<usize>,
}

impl MoveUndo {
    /// Nombre de paires capturées par ce coup (0 à 8, une par direction).
    pub fn pairs_captured_count(&self) -> usize {
        self.captured.iter().filter(|c| c.is_some()).count()
    }

    /// Indices de toutes les pierres capturées par ce coup, à plat. Utilisé
    /// par l'interface pour les mettre en surbrillance brièvement.
    pub fn captured_indices(&self) -> Vec<usize> {
        self.captured
            .iter()
            .flatten()
            .flat_map(|&(a, b)| [a, b])
            .collect()
    }
}

/// Position de jeu complète : plateau, joueur au trait, score incrémental,
/// clé de hachage, compteurs de captures et de voisinage.
///
/// Volontairement petite (moins de 2 Ko) pour pouvoir être clonée à bas coût
/// par thread de recherche (voir `ai::search`, parallélisme "Lazy SMP").
#[derive(Debug, Clone)]
pub struct Position {
    pub cells: [u8; TOTAL],
    /// Nombre de pierres à distance de Tchebychev <= 2 de chaque case.
    /// Maintenu en O(1) amorti par coup ; une case est candidate au jeu si et
    /// seulement si son compteur est > 0. Évite de balayer les 361 cases à
    /// chaque génération de coups.
    pub neighbour_count: [u8; TOTAL],
    pub to_move: u8,
    /// Score incrémental de la position, du point de vue de Noir (positif
    /// favorise Noir). Voir `game::patterns` pour le détail du calcul.
    pub score: i32,
    pub zobrist: u64,
    pub pairs_captured: [u8; 3], // index 0 inutilisé ; 1=noir, 2=blanc
    pub stone_count: u16,
    pub ply: u16,
    pub last_move: Option<usize>,
}

impl Position {
    pub fn new() -> Self {
        let mut cells = [WALL; TOTAL];
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                cells[xy_to_index(x, y)] = EMPTY;
            }
        }
        Position {
            cells,
            neighbour_count: [0; TOTAL],
            to_move: BLACK,
            score: 0,
            zobrist: 0,
            pairs_captured: [0; 3],
            stone_count: 0,
            ply: 0,
            last_move: None,
        }
    }

    /// Lecture "sûre" d'une case à un index signé calculé par arithmétique de
    /// pointeur. Renvoie `WALL` pour tout index hors tableau : par
    /// construction (bordure de 4 cases) cela ne devrait jamais arriver pour
    /// les décalages utilisés dans ce module (magnitude <= 4), mais le sujet
    /// interdit absolument tout crash, donc on ne fait jamais confiance à
    /// l'arithmétique seule.
    #[inline(always)]
    pub(crate) fn cell_at(&self, p: i32) -> u8 {
        if p < 0 {
            return WALL;
        }
        self.cells.get(p as usize).copied().unwrap_or(WALL)
    }

    #[inline(always)]
    pub fn is_on_board(&self, index: usize) -> bool {
        index < TOTAL && self.cells[index] != WALL
    }

    #[inline(always)]
    pub fn is_empty(&self, index: usize) -> bool {
        index < TOTAL && self.cells[index] == EMPTY
    }

    #[inline(always)]
    pub fn has_neighbour(&self, index: usize) -> bool {
        index < TOTAL && self.neighbour_count[index] > 0
    }

    fn update_neighbours(&mut self, index: usize, increment: bool) {
        let Some((x, y)) = index_to_xy(index) else { return };
        let (x, y) = (x as i32, y as i32);
        for dy in -2..=2i32 {
            for dx in -2..=2i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx as usize >= BOARD_SIZE || ny as usize >= BOARD_SIZE {
                    continue;
                }
                let ni = xy_to_index(nx as usize, ny as usize);
                if increment {
                    self.neighbour_count[ni] = self.neighbour_count[ni].saturating_add(1);
                } else {
                    self.neighbour_count[ni] = self.neighbour_count[ni].saturating_sub(1);
                }
            }
        }
    }

    /// Somme la contribution des 4 lignes (une par axe) passant par `index`,
    /// dans l'état courant du plateau. Utilisé avant et après toute mutation
    /// de case : la différence des deux appels est le delta incrémental du
    /// coup. Voir `game::patterns::line_contribution` pour le détail du
    /// balayage (bornes de ligne calculées en arithmétique pure, sans
    /// recherche de bord, donc rapide malgré l'absence de table précalculée).
    fn lines_score(&self, index: usize) -> i32 {
        (0..AXES.len())
            .map(|axis| crate::game::patterns::line_contribution(self, index, axis))
            .sum()
    }

    /// Change la valeur d'une case et maintient en même temps le score
    /// incrémental, le hachage de Zobrist et les compteurs de voisinage.
    /// C'est le seul point d'écriture sur `cells` : toute mutation du
    /// plateau doit passer par ici pour rester cohérente.
    fn set_cell(&mut self, index: usize, new_state: u8) {
        let old_state = self.cells[index];
        if old_state == new_state {
            return;
        }
        let before = self.lines_score(index);
        if old_state == BLACK || old_state == WHITE {
            self.zobrist ^= zobrist().stones[old_state as usize][index];
            self.update_neighbours(index, false);
        }
        self.cells[index] = new_state;
        if new_state == BLACK || new_state == WHITE {
            self.zobrist ^= zobrist().stones[new_state as usize][index];
            self.update_neighbours(index, true);
        }
        let after = self.lines_score(index);
        self.score += after - before;
    }

    /// Joue une pierre du joueur au trait en `index`, applique les captures
    /// éventuelles dans les 8 directions, met à jour score/hachage/voisinage,
    /// et bascule le joueur au trait.
    ///
    /// Ne vérifie PAS la légalité du coup (case libre, double-trois...) :
    /// c'est la responsabilité de l'appelant (`game::rules`). Cette fonction
    /// est le point le plus chaud de la recherche ; elle reste volontairement
    /// directe, sans allocation.
    pub fn make_move(&mut self, index: usize) -> MoveUndo {
        let player = self.to_move;
        let opp = opponent(player);
        let prev_score = self.score;
        let prev_zobrist = self.zobrist;
        let prev_pairs_captured = self.pairs_captured[player as usize];
        let prev_last_move = self.last_move;

        self.set_cell(index, player);

        // Capture : dans chacune des 8 directions, `O O X` (deux pierres
        // adverses immédiatement suivies d'une pierre à soi) fait disparaître
        // la paire. On ne capture jamais une pierre seule ni trois pierres ou
        // plus alignées : les indices testés sont exactement +1d et +2d, ni
        // plus ni moins.
        let mut captured = [None; 8];
        for (i, &d) in DIRS8.iter().enumerate() {
            let idx = index as i32;
            let p1 = idx + d;
            let p2 = idx + 2 * d;
            let p3 = idx + 3 * d;
            if self.cell_at(p1) == opp && self.cell_at(p2) == opp && self.cell_at(p3) == player {
                let (p1, p2) = (p1 as usize, p2 as usize);
                self.set_cell(p1, EMPTY);
                self.set_cell(p2, EMPTY);
                captured[i] = Some((p1, p2));
            }
        }

        let pairs_this_move = captured.iter().filter(|c| c.is_some()).count() as u8;
        if pairs_this_move > 0 {
            self.zobrist ^= zobrist().captures[player as usize][prev_pairs_captured as usize];
            self.pairs_captured[player as usize] = self.pairs_captured[player as usize]
                .saturating_add(pairs_this_move)
                .min(5);
            self.zobrist ^=
                zobrist().captures[player as usize][self.pairs_captured[player as usize] as usize];
        }

        self.to_move = opp;
        self.zobrist ^= zobrist().side;
        self.stone_count += 1;
        self.ply += 1;
        self.last_move = Some(index);

        MoveUndo {
            index,
            player,
            captured,
            prev_score,
            prev_zobrist,
            prev_pairs_captured,
            prev_last_move,
        }
    }

    /// Défait exactement le coup décrit par `undo`. Les appels doivent
    /// s'empiler en LIFO, comme dans tout minimax à base de make/unmake : on
    /// ne défait jamais un coup qui n'est pas le dernier joué.
    pub fn unmake_move(&mut self, undo: &MoveUndo) {
        for pair in undo.captured.iter().flatten() {
            self.set_cell(pair.0, opponent(undo.player));
            self.set_cell(pair.1, opponent(undo.player));
        }
        self.set_cell(undo.index, EMPTY);

        self.to_move = undo.player;
        self.pairs_captured[undo.player as usize] = undo.prev_pairs_captured;
        self.score = undo.prev_score;
        self.zobrist = undo.prev_zobrist;
        self.stone_count -= 1;
        self.ply -= 1;
        self.last_move = undo.prev_last_move;
    }

    /// Compte, à partir de `index`, la longueur de l'alignement de la couleur
    /// `player` sur l'axe `d`, et si chaque extrémité est ouverte (case vide,
    /// ni pierre adverse ni bordure).
    pub fn count_axis(&self, index: usize, d: i32, player: u8) -> (i32, bool, bool) {
        let mut count = 1;
        let idx = index as i32;
        let mut p = idx - d;
        while self.cell_at(p) == player {
            count += 1;
            p -= d;
        }
        let left_open = self.cell_at(p) == EMPTY;
        let mut p = idx + d;
        while self.cell_at(p) == player {
            count += 1;
            p += d;
        }
        let right_open = self.cell_at(p) == EMPTY;
        (count, left_open, right_open)
    }

    /// Vrai si la pierre en `index` (déjà posée) forme un alignement de 5
    /// pierres ou plus sur au moins un axe.
    pub fn forms_five(&self, index: usize) -> bool {
        let player = self.cells[index];
        if player != BLACK && player != WHITE {
            return false;
        }
        AXES.iter().any(|&d| self.count_axis(index, d, player).0 >= 5)
    }

    /// Renvoie tous les indices appartenant à un alignement de 5+ passant par
    /// `index`, tous axes confondus. Utilisé par la règle de fin de partie
    /// liée aux captures (section 3.4 du sujet) : il faut savoir précisément
    /// quelles pierres composent la ligne gagnante pour vérifier si une
    /// capture adverse peut la casser.
    pub fn winning_stones_through(&self, index: usize) -> Vec<usize> {
        let player = self.cells[index];
        let mut stones = Vec::new();
        if player != BLACK && player != WHITE {
            return stones;
        }
        for &d in AXES.iter() {
            let (count, _, _) = self.count_axis(index, d, player);
            if count >= 5 {
                let idx = index as i32;
                let mut p = idx;
                while self.cell_at(p) == player {
                    p -= d;
                }
                p += d;
                while self.cell_at(p) == player {
                    stones.push(p as usize);
                    p += d;
                }
            }
        }
        stones
    }

    /// Recalcule le score depuis zéro, en balayant chaque ligne du plateau
    /// (une fois par ligne, pas par case) sur les 4 axes. Coûteux : réservé
    /// aux tests de cohérence. La recherche utilise exclusivement le score
    /// incrémental maintenu par `set_cell`.
    ///
    /// Chaque ligne n'est comptée qu'une fois : on ne l'évalue que depuis sa
    /// première case (celle dont le voisin dans le sens négatif de l'axe est
    /// un mur), ce qui fonctionne uniformément pour les lignes horizontales,
    /// verticales et diagonales quelle que soit leur longueur.
    #[cfg(test)]
    pub fn recompute_score_from_scratch(&self) -> i32 {
        let mut total = 0i32;
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                let index = xy_to_index(x, y);
                for (axis, &d) in AXES.iter().enumerate() {
                    if self.cell_at(index as i32 - d) == WALL {
                        total += crate::game::patterns::line_contribution(self, index, axis);
                    }
                }
            }
        }
        total
    }
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Position {
    /// Place directement une pierre pour construire un scénario de test, en
    /// maintenant score/zobrist/voisinage/`stone_count` cohérents
    /// (contrairement à une écriture directe `pos.cells[i] = couleur`, qui
    /// laisse le plateau dans un état incohérent où les compteurs ne
    /// reflètent pas les pierres présentes). Ne vérifie aucune règle, ne
    /// bascule pas `to_move` : usage tests uniquement.
    pub fn set_stone_for_test(&mut self, index: usize, color: u8) {
        let was_stone = self.cells[index] == BLACK || self.cells[index] == WHITE;
        self.set_cell(index, color);
        let is_stone = color == BLACK || color == WHITE;
        if is_stone && !was_stone {
            self.stone_count += 1;
        } else if !is_stone && was_stone {
            self.stone_count = self.stone_count.saturating_sub(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::rules::legal_moves_naive;

    #[test]
    fn plateau_neuf_est_vide_et_score_nul() {
        let pos = Position::new();
        for y in 0..BOARD_SIZE {
            for x in 0..BOARD_SIZE {
                assert_eq!(pos.cells[xy_to_index(x, y)], EMPTY);
            }
        }
        assert_eq!(pos.score, 0);
        assert_eq!(pos.to_move, BLACK);
    }

    #[test]
    fn bordure_est_bien_un_mur() {
        let pos = Position::new();
        assert_eq!(pos.cells[0], WALL);
        assert_eq!(pos.cells[TOTAL - 1], WALL);
        assert_eq!(pos.cell_at(-1), WALL);
        assert_eq!(pos.cell_at(TOTAL as i32 + 100), WALL);
    }

    #[test]
    fn coord_name_coins() {
        assert_eq!(coord_name(xy_to_index(0, 0)), "A1");
        assert_eq!(coord_name(xy_to_index(18, 18)), "T19");
        // Le 'I' est sauté, la 9e colonne (index 8) doit être 'J'.
        assert_eq!(coord_name(xy_to_index(8, 0)), "J1");
    }

    #[test]
    fn make_puis_unmake_simple_restaure_tout() {
        let mut pos = Position::new();
        let before = pos.clone();
        let center = xy_to_index(9, 9);
        let undo = pos.make_move(center);
        assert_eq!(pos.cells[center], BLACK);
        assert_eq!(pos.to_move, WHITE);
        pos.unmake_move(&undo);
        assert_eq!(pos.cells, before.cells);
        assert_eq!(pos.score, before.score);
        assert_eq!(pos.zobrist, before.zobrist);
        assert_eq!(pos.to_move, before.to_move);
        assert_eq!(pos.stone_count, before.stone_count);
        assert_eq!(pos.neighbour_count, before.neighbour_count);
    }

    #[test]
    fn capture_horizontale_simple() {
        let mut pos = Position::new();
        // Noir en (5,5) et (8,5), Blanc en (6,5) et (7,5). Noir joue... déjà
        // joué : on place directement puis on vérifie qu'un coup blanc ne
        // capture pas ses propres pierres, puis on rejoue la capture noire
        // depuis une position fraîche avec le bon joueur au trait.
        let mut pos2 = Position::new();
        pos2.cells[xy_to_index(8, 5)] = BLACK;
        pos2.cells[xy_to_index(6, 5)] = WHITE;
        pos2.cells[xy_to_index(7, 5)] = WHITE;
        pos2.to_move = BLACK;
        // On force to_move = BLACK et on joue en (5,5) : la paire blanche en
        // (6,5)-(7,5) doit être capturée.
        let idx = xy_to_index(5, 5);
        pos2.make_move(idx);
        assert_eq!(pos2.cells[xy_to_index(6, 5)], EMPTY);
        assert_eq!(pos2.cells[xy_to_index(7, 5)], EMPTY);
        assert_eq!(pos2.pairs_captured[BLACK as usize], 1);
        let _ = &mut pos; // évite un warning si la première variable n'est plus utilisée
    }

    #[test]
    fn capture_puis_unmake_restaure_les_pierres_capturees() {
        let mut pos = Position::new();
        pos.set_stone_for_test(xy_to_index(8, 5), BLACK);
        pos.set_stone_for_test(xy_to_index(6, 5), WHITE);
        pos.set_stone_for_test(xy_to_index(7, 5), WHITE);
        pos.to_move = BLACK;
        let before = pos.clone();
        let idx = xy_to_index(5, 5);
        let undo = pos.make_move(idx);
        pos.unmake_move(&undo);
        assert_eq!(pos.cells, before.cells);
        assert_eq!(pos.pairs_captured, before.pairs_captured);
        assert_eq!(pos.score, before.score);
        assert_eq!(pos.zobrist, before.zobrist);
        assert_eq!(pos.neighbour_count, before.neighbour_count);
    }

    #[test]
    fn pas_de_capture_sur_une_seule_pierre() {
        let mut pos = Position::new();
        pos.cells[xy_to_index(6, 5)] = WHITE;
        pos.cells[xy_to_index(7, 5)] = BLACK;
        pos.to_move = BLACK;
        let idx = xy_to_index(4, 5); // trop loin, pas de motif O O X
        pos.make_move(idx);
        assert_eq!(pos.cells[xy_to_index(6, 5)], WHITE);
    }

    #[test]
    fn pas_de_capture_sur_trois_pierres_alignees() {
        let mut pos = Position::new();
        pos.cells[xy_to_index(6, 5)] = WHITE;
        pos.cells[xy_to_index(7, 5)] = WHITE;
        pos.cells[xy_to_index(8, 5)] = WHITE;
        pos.to_move = BLACK;
        let idx = xy_to_index(5, 5);
        pos.make_move(idx);
        // Le motif est O O O, pas O O X : aucune capture (la 3e case n'est
        // pas la pierre du joueur).
        assert_eq!(pos.cells[xy_to_index(6, 5)], WHITE);
        assert_eq!(pos.cells[xy_to_index(7, 5)], WHITE);
        assert_eq!(pos.pairs_captured[BLACK as usize], 0);
    }

    #[test]
    fn pas_d_autocapture_en_entrant_dans_un_sandwich() {
        // Le sujet : "on ne peut pas se déplacer dans une capture". Concrètement,
        // poser volontairement sa pierre entre deux pierres adverses (O _ O)
        // ne doit jamais capturer ni être traité comme un coup spécial : ce
        // n'est tout simplement pas un des 8 motifs "O O X" vérifiés par
        // make_move. On le prouve en jouant Blanc dans le trou d'un O _ O noir
        // et en vérifiant qu'aucune pierre n'a disparu et qu'aucune capture
        // n'a été comptée pour personne.
        let mut pos = Position::new();
        pos.cells[xy_to_index(5, 5)] = BLACK;
        pos.cells[xy_to_index(7, 5)] = BLACK;
        pos.to_move = WHITE;
        let idx = xy_to_index(6, 5);
        pos.make_move(idx);
        assert_eq!(pos.cells[xy_to_index(5, 5)], BLACK);
        assert_eq!(pos.cells[xy_to_index(7, 5)], BLACK);
        assert_eq!(pos.cells[xy_to_index(6, 5)], WHITE);
        assert_eq!(pos.pairs_captured[BLACK as usize], 0);
        assert_eq!(pos.pairs_captured[WHITE as usize], 0);
    }

    #[test]
    fn reversibilite_sur_sequences_aleatoires() {
        // Garantie la plus importante du moteur : après avoir joué puis
        // entièrement défait une longue séquence de coups légaux, la position
        // doit être bit-à-bit identique à l'état de départ.
        let mut rng: u64 = 0xC0FFEE_1234_5678;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        for run in 0..200u32 {
            let mut pos = Position::new();
            let initial = pos.clone();
            let mut undos = Vec::new();
            let moves = 10 + (next() % 30) as usize;

            for _ in 0..moves {
                let candidates = legal_moves_naive(&pos);
                if candidates.is_empty() {
                    break;
                }
                let choice = candidates[(next() as usize) % candidates.len()];
                undos.push(pos.make_move(choice));
            }

            while let Some(undo) = undos.pop() {
                pos.unmake_move(&undo);
            }

            assert_eq!(pos.cells, initial.cells, "run {run}: plateau différent après unmake complet");
            assert_eq!(pos.score, initial.score, "run {run}: score différent après unmake complet");
            assert_eq!(pos.zobrist, initial.zobrist, "run {run}: zobrist différent après unmake complet");
            assert_eq!(pos.pairs_captured, initial.pairs_captured, "run {run}: captures différentes");
            assert_eq!(pos.neighbour_count, initial.neighbour_count, "run {run}: voisinage différent");
            assert_eq!(pos.to_move, initial.to_move, "run {run}: joueur au trait différent");
            assert_eq!(pos.stone_count, initial.stone_count, "run {run}: compteur de pierres différent");
        }
    }

    #[test]
    fn score_incremental_egale_recalcul_complet() {
        let mut rng: u64 = 0xDEAD_BEEF_1234_5678;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };

        for run in 0..200u32 {
            let mut pos = Position::new();
            let moves = 5 + (next() % 60) as usize;
            for _ in 0..moves {
                let candidates = legal_moves_naive(&pos);
                if candidates.is_empty() {
                    break;
                }
                let choice = candidates[(next() as usize) % candidates.len()];
                pos.make_move(choice);
                assert_eq!(
                    pos.score,
                    pos.recompute_score_from_scratch(),
                    "run {run}: score incrémental désynchronisé après {moves} coups"
                );
            }
        }
    }
}
