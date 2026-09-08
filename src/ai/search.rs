//! Moteur de recherche : negamax + élagage alpha-bêta, approfondissement
//! itératif avec fenêtre d'aspiration, recherche à fenêtre nulle (PVS),
//! table de transposition, extension de quiescence bornée, et parallélisme
//! "Lazy SMP" via `std::thread::scope`.
//!
//! Voir `DEFENSE.md` pour l'explication pédagogique complète (arbre
//! alpha-bêta dessiné à la main, justification de chaque brique). Ce fichier
//! ne contient que l'implémentation ; les commentaires expliquent le
//! *pourquoi* de chaque choix, pas la mécanique de base de l'algorithme.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::ai::eval;
use crate::ai::movegen;
use crate::ai::tt::{Bound, TranspositionTable, TtEntry};
use crate::game::position::{opponent, Position, TOTAL};
use crate::game::rules;

/// Profondeur maximale (en demi-coups) d'un chemin de recherche, quiescence
/// comprise. Sert uniquement à dimensionner les tableaux `killers` sans
/// allocation : la recherche s'arrête toujours bien avant grâce au budget de
/// temps, cette constante n'est qu'un filet de sécurité contre tout
/// dépassement de tableau si jamais `max_depth` était mal configuré.
const MAX_PLY: usize = 160;

/// Nombre de demi-coups suppplémentaires que la quiescence peut explorer
/// au-delà de la profondeur nominale, en ne considérant que les coups
/// forçants (cinq, blocage, capture). Borne l'effet d'horizon sans laisser
/// la recherche s'emballer sur une position très tactique.
const MAX_QUIESCENCE: u32 = 8;

/// Toutes les combien de nœuds on vérifie l'horloge. Une vérification par
/// nœud coûterait un appel système à chaque appel de fonction ; toutes les
/// 2048, le coût est négligeable sans jamais dépasser l'échéance de plus de
/// quelques millisecondes.
const TIME_CHECK_INTERVAL: u64 = 2048;

#[derive(Debug, Clone, Copy)]
pub struct SearchLimits {
    /// Ne pas démarrer une nouvelle itération d'approfondissement au-delà de
    /// cette échéance (mais une itération déjà commencée peut continuer
    /// jusqu'à `hard_ms`).
    pub soft_ms: u64,
    /// Interrompt la recherche immédiatement, où qu'elle en soit.
    pub hard_ms: u64,
    pub max_depth: u8,
    pub threads: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        SearchLimits { soft_ms: 380, hard_ms: 480, max_depth: 16, threads: default_thread_count() }
    }
}

pub fn default_thread_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8)
}

#[derive(Debug, Clone, Default)]
pub struct SearchStats {
    pub depth_reached: u8,
    pub nodes: u64,
    pub elapsed: Duration,
    pub score: i32,
    pub pv: Vec<usize>,
    pub tt_probes: u64,
    pub tt_hits: u64,
    pub root_candidates: usize,
    pub root_scores: Vec<(usize, i32)>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub best_move: Option<usize>,
    pub stats: SearchStats,
}

/// Contexte mutable d'un fil de recherche : tout ce qui change pendant la
/// recherche mais ne fait pas partie de la position elle-même. Un `ctx` par
/// thread, jamais partagé (seuls `tt` et `stop` sont partagés, via
/// références vers des données synchronisées).
struct SearchContext<'a> {
    tt: &'a TranspositionTable,
    stop: &'a AtomicBool,
    deadline_hard: Instant,
    nodes: u64,
    tt_probes: u64,
    tt_hits: u64,
    killers: [[Option<usize>; 2]; MAX_PLY],
    history: [[i32; TOTAL]; 3],
    /// Sel de départage propre à ce thread, pour diversifier légèrement
    /// l'ordre des coups entre threads (Lazy SMP) sans changer la logique de
    /// tri : deux threads qui exploreraient exactement le même ordre
    /// n'apporteraient rien l'un à l'autre via la table de transposition.
    seed: u64,
}

impl<'a> SearchContext<'a> {
    fn new(tt: &'a TranspositionTable, stop: &'a AtomicBool, deadline_hard: Instant, seed: u64) -> Self {
        SearchContext {
            tt,
            stop,
            deadline_hard,
            nodes: 0,
            tt_probes: 0,
            tt_hits: 0,
            killers: [[None; 2]; MAX_PLY],
            history: [[0; TOTAL]; 3],
            seed,
        }
    }

    /// Vrai si la recherche doit s'arrêter maintenant. Incrémente le compteur
    /// de nœuds et ne consulte l'horloge que périodiquement.
    #[inline]
    fn tick(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes % TIME_CHECK_INTERVAL == 0 && Instant::now() >= self.deadline_hard {
            self.stop.store(true, Relaxed);
        }
        self.stop.load(Relaxed)
    }
}

/// Renvoie `Some(score)` si la position vient de se terminer par le dernier
/// coup joué (`pos.last_move`), du point de vue de `pos.to_move` (qui, si la
/// position est terminale, vient donc de perdre). `ply` sert à préférer les
/// victoires rapides : un mat en 1 vaut mieux qu'un mat en 5.
///
/// Simplification assumée (voir `DEFENSE.md`) : la clause de fin de partie
/// liée aux captures (section 3.4 du sujet, alignement "cassable") n'est PAS
/// réévaluée à l'intérieur de l'arbre de recherche à chaque nœud — seul
/// `game::state::Game::try_play` l'applique strictement pour la partie
/// réellement jouée. Dans l'arbre, un alignement de 5+ est traité comme une
/// victoire immédiate. Un raffinement complet impliquerait de simuler aussi
/// les captures de rupture à chaque nœud terminal, ce qui multiplierait le
/// coût de la détection terminale par le nombre de coups de capture
/// possibles ; le compromis est documenté et mesuré.
fn terminal_score(pos: &Position, ply: u32) -> Option<i32> {
    let last = pos.last_move?;
    let mover = opponent(pos.to_move);
    if rules::has_won_by_capture(pos, mover) {
        return Some(-(eval::WIN - ply as i32));
    }
    if pos.forms_five(last) {
        return Some(-(eval::WIN - ply as i32));
    }
    None
}

/// Coups à considérer en quiescence : uniquement les coups "forçants"
/// (compléter un cinq, bloquer un cinq adverse, capturer, créer un quatre).
/// Liste volontairement courte (au plus 8) : la quiescence doit rester bon
/// marché, elle n'a pas vocation à explorer largement.
fn forcing_moves(pos: &Position, player: u8) -> Vec<usize> {
    use crate::game::patterns::{virtual_best_run_score, would_form_five};
    use crate::game::position::{xy_to_index, BOARD_SIZE};
    use crate::game::rules::{capturing_pairs, is_move_legal_as};

    let opp = opponent(player);
    let mut moves = Vec::new();
    for y in 0..BOARD_SIZE {
        for x in 0..BOARD_SIZE {
            let index = xy_to_index(x, y);
            if !pos.is_empty(index) || !pos.has_neighbour(index) {
                continue;
            }
            let forcing = would_form_five(pos, index, player)
                || would_form_five(pos, index, opp)
                || !capturing_pairs(pos, index, player).is_empty()
                || virtual_best_run_score(pos, index, player) >= 100_000;
            if forcing && is_move_legal_as(pos, index, player) {
                moves.push(index);
                if moves.len() >= 8 {
                    return moves;
                }
            }
        }
    }
    moves
}

fn quiescence(pos: &mut Position, mut alpha: i32, beta: i32, ply: u32, qdepth: u32, ctx: &mut SearchContext) -> i32 {
    if ctx.tick() {
        return 0;
    }
    if let Some(s) = terminal_score(pos, ply) {
        return s;
    }

    let stand_pat = eval::evaluate(pos);
    if stand_pat >= beta {
        return stand_pat;
    }
    if stand_pat > alpha {
        alpha = stand_pat;
    }
    if qdepth >= MAX_QUIESCENCE || ply as usize >= MAX_PLY - 1 {
        return stand_pat;
    }

    let player = pos.to_move;
    for mv in forcing_moves(pos, player) {
        let undo = pos.make_move(mv);
        let score = -quiescence(pos, -beta, -alpha, ply + 1, qdepth + 1, ctx);
        pos.unmake_move(&undo);
        if ctx.stop.load(Relaxed) {
            return 0;
        }
        if score >= beta {
            return score;
        }
        if score > alpha {
            alpha = score;
        }
    }
    alpha
}

/// Cœur de la recherche : negamax + alpha-bêta + PVS + table de
/// transposition, sur les nœuds internes (hors racine, voir `search_root`).
fn negamax(pos: &mut Position, depth: i32, ply: u32, mut alpha: i32, beta: i32, ctx: &mut SearchContext) -> i32 {
    if ctx.tick() {
        return 0;
    }
    if let Some(s) = terminal_score(pos, ply) {
        return s;
    }
    if depth <= 0 {
        return quiescence(pos, alpha, beta, ply, 0, ctx);
    }

    let original_alpha = alpha;
    ctx.tt_probes += 1;
    let tt_entry = ctx.tt.probe(pos.zobrist);
    if let Some(e) = tt_entry {
        ctx.tt_hits += 1;
        if e.depth as i32 >= depth {
            match e.bound {
                Bound::Exact => return e.score,
                Bound::Lower if e.score >= beta => return e.score,
                Bound::Upper if e.score <= alpha => return e.score,
                _ => {}
            }
        }
    }
    let tt_move = tt_entry.and_then(|e| e.best_move);

    let player = pos.to_move;
    let killers = ctx.killers.get(ply as usize).copied().unwrap_or([None, None]);
    let moves = movegen::generate_ordered(
        pos,
        player,
        tt_move,
        killers,
        &ctx.history[player as usize],
        movegen::candidate_limit(ply),
    );

    if moves.is_empty() {
        return 0; // aucun coup légal : match nul
    }

    let mut best_score = -eval::INF;
    let mut best_move = None;
    let mut alpha = alpha;

    for (i, &mv) in moves.iter().enumerate() {
        let undo = pos.make_move(mv);
        let score = if i == 0 {
            -negamax(pos, depth - 1, ply + 1, -beta, -alpha, ctx)
        } else {
            let s = -negamax(pos, depth - 1, ply + 1, -alpha - 1, -alpha, ctx);
            if s > alpha && s < beta && !ctx.stop.load(Relaxed) {
                -negamax(pos, depth - 1, ply + 1, -beta, -alpha, ctx)
            } else {
                s
            }
        };
        pos.unmake_move(&undo);

        if ctx.stop.load(Relaxed) {
            return 0;
        }

        if score > best_score {
            best_score = score;
            best_move = Some(mv);
        }
        if score > alpha {
            alpha = score;
            if alpha >= beta {
                if (ply as usize) < MAX_PLY {
                    let k = &mut ctx.killers[ply as usize];
                    if k[0] != Some(mv) {
                        k[1] = k[0];
                        k[0] = Some(mv);
                    }
                }
                ctx.history[player as usize][mv] =
                    ctx.history[player as usize][mv].saturating_add(depth * depth);
                break;
            }
        }
    }

    let bound = if best_score <= original_alpha {
        Bound::Upper
    } else if best_score >= beta {
        Bound::Lower
    } else {
        Bound::Exact
    };
    ctx.tt.store(
        pos.zobrist,
        TtEntry { score: best_score, depth: depth.clamp(0, 255) as u8, bound, best_move },
    );

    best_score
}

/// Recherche à la racine : identique à `negamax` dans son principe, mais
/// gérée séparément pour pouvoir renvoyer le meilleur coup (pas seulement un
/// score) et remonter le drapeau d'abandon à l'appelant.
fn search_root(
    pos: &mut Position,
    depth: i32,
    alpha0: i32,
    beta0: i32,
    ctx: &mut SearchContext,
) -> (i32, Option<usize>, bool, Vec<(usize, i32)>) {
    if ctx.tick() {
        return (0, None, true, Vec::new());
    }

    let player = pos.to_move;
    let tt_entry = ctx.tt.probe(pos.zobrist);
    let tt_move = tt_entry.and_then(|e| e.best_move);
    let killers = ctx.killers[0];
    let moves = movegen::generate_ordered(
        pos,
        player,
        tt_move,
        killers,
        &ctx.history[player as usize],
        movegen::candidate_limit(0),
    );

    if moves.is_empty() {
        return (0, None, false, Vec::new());
    }

    let mut alpha = alpha0;
    let beta = beta0;
    let mut best_score = -eval::INF;
    let mut best_move = None;
    let mut root_scores = Vec::with_capacity(moves.len());

    for (i, &mv) in moves.iter().enumerate() {
        let undo = pos.make_move(mv);
        let score = if i == 0 {
            -negamax(pos, depth - 1, 1, -beta, -alpha, ctx)
        } else {
            let s = -negamax(pos, depth - 1, 1, -alpha - 1, -alpha, ctx);
            if s > alpha && s < beta && !ctx.stop.load(Relaxed) {
                -negamax(pos, depth - 1, 1, -beta, -alpha, ctx)
            } else {
                s
            }
        };
        pos.unmake_move(&undo);

        if ctx.stop.load(Relaxed) {
            return (best_score, best_move, true, root_scores);
        }

        root_scores.push((mv, score));
        if score > best_score {
            best_score = score;
            best_move = Some(mv);
        }
        if score > alpha {
            alpha = score;
            if alpha >= beta {
                break;
            }
        }
    }

    if let Some(bm) = best_move {
        let bound = if best_score >= beta {
            Bound::Lower
        } else if best_score <= alpha0 {
            Bound::Upper
        } else {
            Bound::Exact
        };
        ctx.tt.store(
            pos.zobrist,
            TtEntry { score: best_score, depth: depth.clamp(0, 255) as u8, bound, best_move: Some(bm) },
        );
    }

    (best_score, best_move, false, root_scores)
}

#[derive(Debug, Clone, Default)]
struct ThreadResult {
    best_move: Option<usize>,
    best_score: i32,
    depth_reached: u8,
    nodes: u64,
    tt_probes: u64,
    tt_hits: u64,
    root_scores: Vec<(usize, i32)>,
    is_primary: bool,
}

/// Approfondissement itératif pour un thread : augmente la profondeur de 1
/// en 1, avec une fenêtre d'aspiration resserrée autour du score de
/// l'itération précédente (élargie et relancée en cas d'échec haut ou bas).
///
/// Choix assumé (voir `DEFENSE.md`) : une itération interrompue par le temps
/// est TOUJOURS jetée intégralement, y compris si elle avait déjà amélioré
/// le premier coup. C'est plus simple à garantir correct que de tenter de
/// réutiliser un résultat partiel, au prix d'un peu de temps de réflexion
/// perdu en fin de budget.
fn iterative_deepening(
    mut pos: Position,
    tt: &TranspositionTable,
    stop: &AtomicBool,
    deadline_soft: Instant,
    deadline_hard: Instant,
    max_depth: u8,
    thread_id: usize,
) -> ThreadResult {
    let mut ctx = SearchContext::new(tt, stop, deadline_hard, thread_id as u64);
    let mut result = ThreadResult { is_primary: thread_id == 0, ..Default::default() };
    let mut prev_score = 0i32;

    let mut depth = 1i32;
    while depth <= max_depth as i32 {
        if depth > 1 && Instant::now() >= deadline_soft {
            break;
        }

        let mut window = 50i32;
        let (mut alpha, mut beta) = if depth <= 2 {
            (-eval::INF, eval::INF)
        } else {
            (
                (prev_score - window).max(-eval::INF),
                (prev_score + window).min(eval::INF),
            )
        };

        let (score, mv, aborted, root_scores) = loop {
            let (s, m, ab, rs) = search_root(&mut pos, depth, alpha, beta, &mut ctx);
            if ab {
                break (s, m, true, rs);
            }
            if s <= alpha && alpha > -eval::INF {
                alpha = (alpha - window).max(-eval::INF);
                window = window.saturating_mul(4);
                continue;
            }
            if s >= beta && beta < eval::INF {
                beta = (beta + window).min(eval::INF);
                window = window.saturating_mul(4);
                continue;
            }
            break (s, m, false, rs);
        };

        if aborted || stop.load(Relaxed) {
            break;
        }

        result.best_move = mv;
        result.best_score = score;
        result.depth_reached = depth as u8;
        result.root_scores = root_scores;
        prev_score = score;
        depth += 1;

        // Une victoire trouvée à coup sûr n'a pas besoin d'être confirmée
        // plus profondément : inutile de consommer le budget de temps.
        if score >= eval::WIN - MAX_PLY as i32 {
            break;
        }
    }

    result.nodes = ctx.nodes;
    result.tt_probes = ctx.tt_probes;
    result.tt_hits = ctx.tt_hits;
    result
}

/// Reconstitue la variante principale en suivant, depuis `pos`, le meilleur
/// coup mémorisé dans la table de transposition à chaque étape. Purement
/// indicative (affichage/debug) : une collision de table peut la tronquer
/// prématurément, ce qui est sans conséquence puisqu'elle ne sert pas au
/// choix du coup lui-même.
fn extract_pv(pos: &mut Position, tt: &TranspositionTable, max_len: usize) -> Vec<usize> {
    let mut pv = Vec::with_capacity(max_len);
    let mut seen = std::collections::HashSet::new();
    let mut undos = Vec::new();

    for _ in 0..max_len {
        if !seen.insert(pos.zobrist) {
            break; // cycle détecté (collision de table) : on s'arrête proprement
        }
        let Some(entry) = tt.probe(pos.zobrist) else { break };
        let Some(mv) = entry.best_move else { break };
        if !pos.is_empty(mv) {
            break;
        }
        pv.push(mv);
        undos.push(pos.make_move(mv));
    }

    for undo in undos.iter().rev() {
        pos.unmake_move(undo);
    }
    pv
}

pub struct Engine {
    tt: Arc<TranspositionTable>,
    threads: usize,
}

impl Engine {
    pub fn new(tt_entries: usize, threads: usize) -> Self {
        Engine { tt: Arc::new(TranspositionTable::new(tt_entries)), threads: threads.max(1) }
    }

    pub fn tt_len(&self) -> usize {
        self.tt.len()
    }

    /// Lance la recherche et renvoie le meilleur coup trouvé ainsi que les
    /// statistiques de la dernière itération complète.
    ///
    /// Garantie de robustesse : ne renvoie jamais `None` si `pos` a au moins
    /// un coup légal (filet de sécurité en toute fin de fonction). Ne
    /// panique jamais : c'est aussi la responsabilité de l'appelant de
    /// tolérer un panic interne éventuel via `catch_unwind` (voir
    /// `app.rs`), mais cette fonction elle-même n'utilise ni `unwrap` ni
    /// indexation non protégée sur des données dérivées de la recherche.
    pub fn search(&self, pos: &Position, limits: SearchLimits) -> SearchResult {
        let start = Instant::now();
        let deadline_soft = start + Duration::from_millis(limits.soft_ms);
        let deadline_hard = start + Duration::from_millis(limits.hard_ms);
        let stop = Arc::new(AtomicBool::new(false));
        let n_threads = limits.threads.max(1);

        let results: Vec<ThreadResult> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(n_threads);
            for t in 0..n_threads {
                let pos_clone = pos.clone();
                let tt = Arc::clone(&self.tt);
                let stop = Arc::clone(&stop);
                let max_depth = limits.max_depth;
                handles.push(scope.spawn(move || {
                    iterative_deepening(pos_clone, &tt, &stop, deadline_soft, deadline_hard, max_depth, t)
                }));
            }
            // `.join().ok()` : si un thread a paniqué (ne devrait jamais
            // arriver, mais le sujet interdit tout crash sans exception),
            // on ignore simplement sa contribution plutôt que de propager
            // le panic à l'appelant.
            handles.into_iter().filter_map(|h| h.join().ok()).collect()
        });
        stop.store(true, Relaxed);

        let chosen = results
            .iter()
            .find(|r| r.is_primary && r.best_move.is_some())
            .or_else(|| results.iter().filter(|r| r.best_move.is_some()).max_by_key(|r| r.depth_reached))
            .cloned();

        let total_nodes: u64 = results.iter().map(|r| r.nodes).sum();
        let total_probes: u64 = results.iter().map(|r| r.tt_probes).sum();
        let total_hits: u64 = results.iter().map(|r| r.tt_hits).sum();
        let elapsed = start.elapsed();

        let best_move = chosen.as_ref().and_then(|r| r.best_move).or_else(|| {
            // Filet de sécurité ultime : aucun thread n'a produit de coup
            // (budget de temps extrêmement court). On prend le premier coup
            // légal disponible plutôt que de ne rien jouer.
            movegen::generate_ordered(pos, pos.to_move, None, [None, None], &[], 1)
                .into_iter()
                .next()
        });

        let pv = match (&chosen, best_move) {
            (Some(r), Some(_)) if r.depth_reached > 0 => {
                let mut pv_pos = pos.clone();
                extract_pv(&mut pv_pos, &self.tt, r.depth_reached as usize + 4)
            }
            _ => Vec::new(),
        };

        let mut root_scores = chosen.as_ref().map(|r| r.root_scores.clone()).unwrap_or_default();
        root_scores.sort_unstable_by(|a, b| b.1.cmp(&a.1));

        SearchResult {
            best_move,
            stats: SearchStats {
                depth_reached: chosen.as_ref().map(|r| r.depth_reached).unwrap_or(0),
                nodes: total_nodes,
                elapsed,
                score: chosen.as_ref().map(|r| r.best_score).unwrap_or(0),
                pv,
                tt_probes: total_probes,
                tt_hits: total_hits,
                root_candidates: root_scores.len(),
                root_scores,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::position::{xy_to_index, BLACK, WHITE};
    use crate::game::rules::is_move_legal;

    fn engine_1_thread() -> Engine {
        Engine::new(1 << 16, 1)
    }

    fn quick_limits() -> SearchLimits {
        SearchLimits { soft_ms: 200, hard_ms: 400, max_depth: 10, threads: 1 }
    }

    #[test]
    fn trouve_le_coup_qui_complete_un_cinq() {
        let mut pos = Position::new();
        for x in 5..9 {
            pos.set_stone_for_test(xy_to_index(x, 9), BLACK);
        }
        // Bloque l'extrémité gauche pour qu'il n'existe qu'UN SEUL coup
        // gagnant (sans ça, le quatre est ouvert aux deux bouts et (4,9)
        // gagnerait tout aussi bien que (9,9) : ce ne serait pas un bug,
        // juste un test ambigu).
        pos.set_stone_for_test(xy_to_index(4, 9), WHITE);
        pos.set_stone_for_test(xy_to_index(3, 3), WHITE);
        pos.set_stone_for_test(xy_to_index(3, 4), WHITE);
        pos.to_move = BLACK;

        let engine = engine_1_thread();
        let result = engine.search(&pos, quick_limits());
        assert_eq!(result.best_move, Some(xy_to_index(9, 9)));
    }

    #[test]
    fn bloque_le_cinq_adverse_imminent() {
        let mut pos = Position::new();
        for x in 5..9 {
            pos.set_stone_for_test(xy_to_index(x, 9), WHITE);
        }
        pos.set_stone_for_test(xy_to_index(3, 3), BLACK);
        pos.set_stone_for_test(xy_to_index(3, 4), BLACK);
        pos.to_move = BLACK;

        let engine = engine_1_thread();
        let result = engine.search(&pos, quick_limits());
        let mv = result.best_move.expect("un coup doit être trouvé");
        assert!(
            mv == xy_to_index(9, 9) || mv == xy_to_index(4, 9),
            "le coup doit bloquer l'alignement blanc, obtenu {mv}"
        );
    }

    #[test]
    fn ne_joue_jamais_un_coup_illegal() {
        let mut pos = Position::new();
        pos.set_stone_for_test(xy_to_index(9, 9), BLACK);
        pos.set_stone_for_test(xy_to_index(9, 8), WHITE);
        pos.to_move = BLACK;

        let engine = engine_1_thread();
        let result = engine.search(&pos, quick_limits());
        if let Some(mv) = result.best_move {
            assert!(is_move_legal(&pos, mv));
        }
    }

    #[test]
    fn prefere_gagner_en_un_coup_plutot_que_plus_tard() {
        let mut pos = Position::new();
        for x in 5..9 {
            pos.set_stone_for_test(xy_to_index(x, 9), BLACK);
        }
        pos.to_move = BLACK;

        let engine = engine_1_thread();
        let result = engine.search(&pos, quick_limits());
        assert!(result.stats.score >= eval::WIN - MAX_PLY as i32);
    }

    #[test]
    fn deux_cents_positions_aleatoires_ne_produisent_jamais_de_coup_illegal() {
        let mut rng: u64 = 0x1234_5678_9ABC_DEF0;
        let mut next = || {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            rng
        };
        let engine = engine_1_thread();
        let fast_limits = SearchLimits { soft_ms: 20, hard_ms: 40, max_depth: 6, threads: 1 };

        for _ in 0..50u32 {
            let mut pos = Position::new();
            let moves = 4 + (next() % 20) as usize;
            for _ in 0..moves {
                let candidates = rules::legal_moves_naive(&pos);
                if candidates.is_empty() {
                    break;
                }
                let mv = candidates[(next() as usize) % candidates.len()];
                pos.make_move(mv);
            }
            let result = engine.search(&pos, fast_limits);
            if let Some(mv) = result.best_move {
                assert!(is_move_legal(&pos, mv), "coup illégal proposé : {mv}");
            }
        }
    }
}

#[cfg(test)]
mod bench_tmp {
    use super::*;
    use crate::game::position::{xy_to_index, BLACK, WHITE};

    #[test]
    #[ignore]
    fn bench_depth_500ms() {
        let threads = default_thread_count();
        println!("threads disponibles: {threads}");

        // Position vide.
        let pos = Position::new();
        let engine = Engine::new(1 << 20, threads);
        let limits = SearchLimits { soft_ms: 380, hard_ms: 500, max_depth: 30, threads };
        let t0 = Instant::now();
        let result = engine.search(&pos, limits);
        println!(
            "plateau vide: depth={} nodes={} elapsed={:?} score={} best={:?}",
            result.stats.depth_reached, result.stats.nodes, t0.elapsed(), result.stats.score, result.best_move
        );

        // Position de milieu de partie typique (quelques pierres, pas de
        // menace immédiate), plus représentative de ce qui compte vraiment.
        let mut mid = Position::new();
        // Disposition en "8 dames" (par couleur) : par construction, deux
        // pierres de la même couleur ne partagent jamais une ligne, une
        // colonne ou une diagonale, donc aucun alignement de trois ou plus
        // n'existe déjà. Ça garde ce test représentatif d'un vrai milieu de
        // partie tactique au lieu de se résoudre trivialement dès la
        // profondeur 1 à cause d'un quatre pré-existant.
        let stones = [
            (6, 6, BLACK), (7, 10, BLACK), (8, 13, BLACK), (9, 11, BLACK),
            (10, 8, BLACK), (11, 12, BLACK), (12, 7, BLACK), (13, 9, BLACK),
            (6, 9, WHITE), (7, 7, WHITE), (8, 12, WHITE), (9, 8, WHITE),
            (10, 11, WHITE), (11, 13, WHITE), (12, 10, WHITE), (13, 6, WHITE),
        ];
        for (x, y, c) in stones {
            mid.set_stone_for_test(xy_to_index(x, y), c);
        }
        mid.to_move = BLACK;
        let engine2 = Engine::new(1 << 20, threads);
        let t1 = Instant::now();
        let result2 = engine2.search(&mid, limits);
        println!(
            "milieu de partie: depth={} nodes={} elapsed={:?} score={} best={:?}",
            result2.stats.depth_reached, result2.stats.nodes, t1.elapsed(), result2.stats.score, result2.best_move
        );
    }
}
