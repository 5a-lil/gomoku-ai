//! État applicatif et boucle d'événements. `App` orchestre : l'écran actif
//! (menu de configuration ou partie en cours), la `Game` en cours, le
//! moteur IA (recherche lancée dans un thread séparé pour ne jamais geler
//! l'affichage), les statistiques de temps de réflexion et les bascules
//! d'affichage du panneau de débogage.
//!
//! Principe de robustesse central (voir section 6 du sujet) : la recherche
//! IA tourne toujours dans un thread à part, enveloppée dans
//! `std::panic::catch_unwind`. Si elle panique malgré tout (ne devrait
//! jamais arriver), un coup légal de repli est joué à la place de faire
//! planter l'application. Aucune fonction de ce fichier n'utilise `unwrap`,
//! `expect` ni `panic!` sur un chemin atteignable depuis une interaction
//! utilisateur.

use std::io;
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::DefaultTerminal;

use crate::ai::eval;
use crate::ai::search::{default_thread_count, Engine, SearchLimits, SearchResult, SearchStats};
use crate::event_thread::{Event, EventThread};
use crate::game::openings::{ColorDecision, OpeningRule, PendingChoice};
use crate::game::position::{index_to_xy, xy_to_index, BLACK, BOARD_SIZE, WHITE};
use crate::game::rules;
use crate::game::{Game, GameMode};
use crate::ui;

/// Taille minimale de terminal en-dessous de laquelle on refuse de dessiner
/// le plateau (section 6 du sujet : un terminal trop petit ne doit jamais
/// produire un affichage tronqué ou paniquer sur un calcul de layout).
pub const MIN_WIDTH: u16 = 96;
pub const MIN_HEIGHT: u16 = 30;

/// Nombre de tics d'horloge (250 ms chacun, voir `event_thread`) pendant
/// lesquels une pierre juste capturée reste mise en évidence en rouge avant
/// de redevenir un affichage normal de case vide.
const CAPTURE_HIGHLIGHT_TICKS: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Menu,
    Playing,
}

/// Ce que le thread de recherche IA était en train de calculer, pour savoir
/// quoi faire du résultat une fois reçu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AiTask {
    /// Coup réel à jouer pour le joueur au trait.
    Move,
    /// Simple suggestion (touche `s`) : on affiche le coup sans le jouer.
    Suggestion,
    /// Choix de couleur Swap/Swap2 : voir `App::finish_color_choice`.
    ColorChoice,
}

struct AiJob {
    rx: mpsc::Receiver<SearchResult>,
    started: Instant,
    task: AiTask,
    /// Couleur au trait au moment où le calcul a été lancé : nécessaire pour
    /// interpréter correctement le signe du score renvoyé (toujours relatif
    /// au joueur qui était au trait à cet instant, convention negamax).
    to_move_at_dispatch: u8,
}

#[derive(Debug, Clone, Copy)]
struct TimerStats {
    last_ms: f64,
    total_ms: f64,
    count: u64,
    max_ms: f64,
}

impl TimerStats {
    const fn new() -> Self {
        TimerStats { last_ms: 0.0, total_ms: 0.0, count: 0, max_ms: 0.0 }
    }

    fn record(&mut self, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        self.last_ms = ms;
        self.total_ms += ms;
        self.count += 1;
        if ms > self.max_ms {
            self.max_ms = ms;
        }
    }

    fn avg_ms(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.total_ms / self.count as f64
        }
    }
}

/// État de l'écran de configuration (avant qu'une partie ne commence).
/// Toutes les valeurs ont des bornes sûres : aucune combinaison choisissable
/// au clavier ne peut produire une configuration invalide pour `Engine`.
#[derive(Debug, Clone)]
pub struct MenuState {
    pub vs_ai: bool,
    pub human_black: bool,
    pub rule_idx: usize,
    pub soft_ms: u64,
    pub hard_ms: u64,
    pub threads: usize,
    pub row: usize,
}

pub const MENU_ROWS: usize = 7;

impl Default for MenuState {
    fn default() -> Self {
        MenuState {
            vs_ai: true,
            human_black: true,
            rule_idx: 0,
            soft_ms: 380,
            hard_ms: 480,
            threads: default_thread_count(),
            row: 0,
        }
    }
}

impl MenuState {
    fn move_row(&mut self, delta: i32) {
        let n = MENU_ROWS as i32;
        self.row = (((self.row as i32) + delta).rem_euclid(n)) as usize;
    }

    /// Ajuste la valeur de la ligne actuellement sélectionnée. `delta` vaut
    /// +1 ou -1 (touches gauche/droite). Toutes les bornes sont serrées ici
    /// : aucune valeur produite ne peut être invalide pour `SearchLimits`.
    fn adjust(&mut self, delta: i32) {
        match self.row {
            0 => self.vs_ai = !self.vs_ai,
            1 => self.human_black = !self.human_black,
            2 => {
                let n = OpeningRule::ALL.len() as i32;
                self.rule_idx = (((self.rule_idx as i32) + delta).rem_euclid(n)) as usize;
            }
            3 => {
                let step = 20i64 * delta as i64;
                let new_val = (self.soft_ms as i64 + step).clamp(100, self.hard_ms as i64 - 20);
                self.soft_ms = new_val.max(100) as u64;
            }
            4 => {
                let step = 20i64 * delta as i64;
                let new_val = (self.hard_ms as i64 + step).clamp(self.soft_ms as i64 + 20, 3000);
                self.hard_ms = new_val as u64;
            }
            5 => {
                let max_threads = default_thread_count().max(1);
                let new_val = (self.threads as i32 + delta).clamp(1, max_threads as i32);
                self.threads = new_val as usize;
            }
            _ => {}
        }
    }

    pub fn rule(&self) -> OpeningRule {
        // `rule_idx` est toujours maintenu dans `0..OpeningRule::ALL.len()`
        // par `adjust` (arithmétique modulo) ; l'indexation directe est donc
        // sûre. On garde tout de même un repli sur `Standard` par prudence
        // absolue (section 6 du sujet : aucune indexation non protégée).
        OpeningRule::ALL.get(self.rule_idx).copied().unwrap_or(OpeningRule::Standard)
    }

    fn limits(&self) -> SearchLimits {
        SearchLimits { soft_ms: self.soft_ms, hard_ms: self.hard_ms, max_depth: 24, threads: self.threads }
    }
}

fn player_label(player: u8) -> &'static str {
    if player == BLACK { "Noir" } else { "Blanc" }
}

/// Filet de sécurité ultime si le thread de recherche a paniqué (ce que
/// `catch_unwind` intercepte déjà) : joue le premier coup légal disponible
/// plutôt que de renvoyer une absence de coup. N'utilise que des prédicats
/// purs de `game::rules`, jamais de code potentiellement instable.
fn fallback_result(pos: &crate::game::position::Position) -> SearchResult {
    let best_move = rules::legal_moves_naive(pos).into_iter().next();
    SearchResult { best_move, stats: SearchStats::default() }
}

pub struct App {
    pub screen: Screen,
    pub menu: MenuState,
    pub game: Option<Game>,
    engine: Arc<Engine>,
    ai_job: Option<AiJob>,
    timer: TimerStats,
    pub last_search_stats: Option<SearchStats>,
    /// Zone exacte de la grille de jeu telle que dessinée au dernier appel
    /// de rendu, pour convertir un clic souris en coordonnées de plateau.
    pub goban_area: Rect,
    pub last_move_highlight: Option<usize>,
    pub just_captured: Vec<usize>,
    captured_ticks: u8,
    pub suggestion: Option<usize>,
    pub cursor: usize,
    pub show_debug: bool,
    pub show_heatmap: bool,
    pub show_breakdown: bool,
    pub status: Option<String>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            screen: Screen::Menu,
            menu: MenuState::default(),
            game: None,
            // Table de transposition de taille modeste par défaut (2^20
            // entrées, ~16 Mo) : voir `ai::tt` pour la dégradation propre en
            // cas d'échec d'allocation. Le moteur est créé une seule fois et
            // réutilisé pour toute la partie : la table survit d'un coup à
            // l'autre, ce qui est un gain net (beaucoup de sous-positions
            // reviennent d'un coup à l'autre) et sans risque de corruption
            // (schéma XOR sans verrou, voir `ai::tt`).
            engine: Arc::new(Engine::new(1 << 20, default_thread_count())),
            ai_job: None,
            timer: TimerStats::new(),
            last_search_stats: None,
            goban_area: Rect::default(),
            last_move_highlight: None,
            just_captured: Vec::new(),
            captured_ticks: 0,
            suggestion: None,
            cursor: xy_to_index(BOARD_SIZE / 2, BOARD_SIZE / 2),
            show_debug: true,
            show_heatmap: false,
            show_breakdown: false,
            status: None,
            should_quit: false,
        }
    }

    pub fn timer_last_ms(&self) -> f64 {
        self.timer.last_ms
    }
    pub fn timer_avg_ms(&self) -> f64 {
        self.timer.avg_ms()
    }
    pub fn timer_max_ms(&self) -> f64 {
        self.timer.max_ms
    }
    pub fn ai_thinking(&self) -> bool {
        self.ai_job.is_some()
    }
    pub fn tt_entries(&self) -> usize {
        self.engine.tt_len()
    }
    pub fn tt_is_empty(&self) -> bool {
        self.engine.tt_is_empty()
    }
    pub fn ai_thinking_elapsed(&self) -> Option<Duration> {
        self.ai_job.as_ref().map(|j| j.started.elapsed())
    }

    // ---------------------------------------------------------------
    // Démarrage / redémarrage de partie
    // ---------------------------------------------------------------

    fn start_game(&mut self) {
        let mode = if self.menu.vs_ai {
            GameMode::HumanVsAi { human: if self.menu.human_black { BLACK } else { WHITE } }
        } else {
            GameMode::HumanVsHuman
        };
        self.game = Some(Game::new(mode, self.menu.rule()));
        self.screen = Screen::Playing;
        self.ai_job = None;
        self.last_move_highlight = None;
        self.just_captured.clear();
        self.captured_ticks = 0;
        self.suggestion = None;
        self.last_search_stats = None;
        self.status = None;
        self.cursor = xy_to_index(BOARD_SIZE / 2, BOARD_SIZE / 2);
    }

    fn restart_quick(&mut self, vs_ai: bool) {
        self.menu.vs_ai = vs_ai;
        self.start_game();
    }

    // ---------------------------------------------------------------
    // Logique de tour : qui a le droit d'agir maintenant ?
    // ---------------------------------------------------------------

    /// Vrai pendant la phase de pose des 3 (ou 5) pierres d'ouverture des
    /// règles Swap/Swap2 en mode Humain vs IA. Simplification assumée et
    /// documentée dans `DEFENSE.md` : le sujet définit ces règles comme "le
    /// joueur 1 place N pierres, puis le joueur 2 choisit sa couleur" ; on
    /// fait donc poser TOUTES les pierres d'ouverture par l'humain (quelle
    /// que soit la couleur de la pierre à poser), pour rester fidèle à cette
    /// idée de "joueur 1 = celui qui prépare le plateau", plutôt que de
    /// suivre `Game::is_human_turn` (qui ne connaît que la couleur assignée
    /// et alternerait sinon entre humain et IA au fil des 3 premières
    /// pierres, ce qui n'aurait pas de sens tant que personne n'a encore
    /// choisi de couleur).
    fn in_human_opening_setup(game: &Game) -> bool {
        matches!(game.mode, GameMode::HumanVsAi { .. })
            && matches!(game.opening.rule, OpeningRule::Swap | OpeningRule::Swap2)
            && (game.position.stone_count as u32) < 3
    }

    /// Vrai si un humain peut agir immédiatement (poser une pierre ou
    /// répondre à une décision de couleur). Sert à la fois à accepter un
    /// clic et à décider si l'IA doit se déclencher (négation de ce
    /// prédicat, voir `drive_ai_if_needed`).
    fn human_may_act(&self) -> bool {
        if self.ai_job.is_some() {
            return false;
        }
        let Some(game) = &self.game else { return false };
        if game.outcome.is_over() {
            return false;
        }
        if game.pending_choice().is_some() {
            return !game.pending_choice_is_ai_turn();
        }
        if Self::in_human_opening_setup(game) {
            return true;
        }
        game.is_human_turn()
    }

    // ---------------------------------------------------------------
    // Boucle d'événements
    // ---------------------------------------------------------------

    pub fn on_tick(&mut self) {
        if self.captured_ticks > 0 {
            self.captured_ticks -= 1;
            if self.captured_ticks == 0 {
                self.just_captured.clear();
            }
        }
        self.poll_ai_job();
        self.drive_ai_if_needed();
    }

    fn poll_ai_job(&mut self) {
        let Some(job) = self.ai_job.take() else { return };
        match job.rx.try_recv() {
            Ok(result) => self.finish_ai_job(job.task, job.to_move_at_dispatch, result),
            Err(TryRecvError::Empty) => {
                self.ai_job = Some(job);
            }
            Err(TryRecvError::Disconnected) => {
                // Le thread s'est arrêté sans envoyer de résultat (ne devrait
                // jamais arriver : `spawn_search_job` envoie toujours quelque
                // chose, même en cas de panique interceptée). On ne bloque
                // pas la partie pour autant : on retente au prochain tour de
                // qui-doit-jouer.
                self.status = Some("l'IA n'a pas répondu, nouvelle tentative".into());
            }
        }
    }

    fn drive_ai_if_needed(&mut self) {
        if self.ai_job.is_some() {
            return;
        }
        let Some(game) = &self.game else { return };
        if self.screen != Screen::Playing || game.outcome.is_over() {
            return;
        }
        if game.pending_choice().is_some() {
            if game.pending_choice_is_ai_turn() {
                self.start_color_choice_job();
            }
            return;
        }
        if Self::in_human_opening_setup(game) {
            return;
        }
        if !game.is_human_turn() {
            self.start_move_job();
        }
    }

    fn spawn_search_job(&self, task: AiTask) -> Option<AiJob> {
        let game = self.game.as_ref()?;
        let pos = game.position.clone();
        let to_move_at_dispatch = pos.to_move;
        let engine = Arc::clone(&self.engine);
        let limits = self.menu.limits();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // Filet de sécurité imposé par le sujet (section 6) : si la
            // recherche panique malgré toutes les précautions internes, on
            // rattrape le panic et on joue un coup légal de repli plutôt que
            // de laisser mourir le thread sans réponse (ce qui bloquerait
            // la partie, `poll_ai_job` ne recevant jamais rien).
            let outcome = std::panic::catch_unwind(|| engine.search(&pos, limits));
            let result = outcome.unwrap_or_else(|_| fallback_result(&pos));
            let _ = tx.send(result);
        });
        Some(AiJob { rx, started: Instant::now(), task, to_move_at_dispatch })
    }

    fn start_move_job(&mut self) {
        self.ai_job = self.spawn_search_job(AiTask::Move);
    }

    fn start_color_choice_job(&mut self) {
        self.ai_job = self.spawn_search_job(AiTask::ColorChoice);
    }

    /// Lance une suggestion de coup (touche `s`) pour le joueur actuellement
    /// au trait, sans jouer le coup. Fonctionne pour les deux couleurs, y
    /// compris en hotseat (section 4.4 du sujet).
    fn request_suggestion(&mut self) {
        if self.ai_job.is_some() {
            self.status = Some("l'IA réfléchit déjà, patientez".into());
            return;
        }
        let Some(game) = &self.game else { return };
        if game.outcome.is_over() || game.pending_choice().is_some() {
            return;
        }
        self.status = Some("calcul de la suggestion...".into());
        self.ai_job = self.spawn_search_job(AiTask::Suggestion);
    }

    fn finish_ai_job(&mut self, task: AiTask, to_move_at_dispatch: u8, result: SearchResult) {
        match task {
            AiTask::Move => self.apply_ai_move(result),
            AiTask::Suggestion => {
                self.suggestion = result.best_move;
                self.last_search_stats = Some(result.stats);
                self.status = Some("suggestion calculée".into());
            }
            AiTask::ColorChoice => self.finish_color_choice(to_move_at_dispatch, result),
        }
    }

    fn apply_ai_move(&mut self, result: SearchResult) {
        self.timer.record(result.stats.elapsed);
        self.last_search_stats = Some(result.stats);
        let Some(mv) = result.best_move else {
            self.status = Some("l'IA n'a trouvé aucun coup légal (plateau plein ?)".into());
            return;
        };
        let Some(game) = &mut self.game else { return };
        match game.try_play(mv) {
            Ok(report) => {
                self.last_move_highlight = Some(report.index);
                self.just_captured = report.captured_indices;
                self.captured_ticks = if self.just_captured.is_empty() { 0 } else { CAPTURE_HIGHLIGHT_TICKS };
                self.suggestion = None;
                self.status = if report.captured_pairs > 0 {
                    Some(format!("{} capture {} paire(s) !", player_label(report.player), report.captured_pairs))
                } else {
                    None
                };
            }
            Err(e) => {
                // Ne devrait jamais arriver : `ai::movegen` ne propose que
                // des coups légaux. Filet de sécurité tout de même : on
                // journalise sans planter, la partie reste dans un état
                // cohérent (le coup n'a simplement pas été joué).
                self.status = Some(format!("coup IA rejeté ({e}) : nouvelle tentative au prochain tour"));
            }
        }
    }

    /// Résout la décision de couleur Swap/Swap2 pour l'IA : on vient de
    /// lancer une recherche complète sur la position réelle (même budget de
    /// temps que pour un coup normal), ce qui donne un score relatif au
    /// joueur qui était au trait à ce moment. On le convertit en "score du
    /// point de vue de Noir" puis on prend la couleur la plus favorable.
    /// Le jeu étant à somme nulle, le score de l'autre option est
    /// simplement l'opposé : les deux sont journalisés (voir `DEFENSE.md`,
    /// section "compromis assumés", pour la discussion de cette méthode).
    fn finish_color_choice(&mut self, to_move_at_dispatch: u8, result: SearchResult) {
        self.last_search_stats = Some(result.stats.clone());
        let score_from_mover = result.stats.score;
        let black_score = if to_move_at_dispatch == BLACK { score_from_mover } else { -score_from_mover };
        let white_score = -black_score;
        let decision = if black_score >= white_score { ColorDecision::TakeBlack } else { ColorDecision::TakeWhite };
        let Some(game) = &mut self.game else { return };
        game.log(format!(
            "l'IA évalue : Noir {black_score:+}, Blanc {white_score:+} — elle choisit {}",
            if matches!(decision, ColorDecision::TakeBlack) { "Noir" } else { "Blanc" }
        ));
        game.apply_color_decision(decision, true);
    }

    // ---------------------------------------------------------------
    // Entrées clavier / souris
    // ---------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        match self.screen {
            Screen::Menu => self.on_key_menu(key),
            Screen::Playing => self.on_key_playing(key),
        }
    }

    fn on_key_menu(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up => self.menu.move_row(-1),
            KeyCode::Down => self.menu.move_row(1),
            KeyCode::Left => self.menu.adjust(-1),
            KeyCode::Right => self.menu.adjust(1),
            KeyCode::Enter => self.start_game(),
            _ => {}
        }
    }

    fn on_key_playing(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('n') => {
                self.screen = Screen::Menu;
                self.ai_job = None;
            }
            KeyCode::Char('r') => self.restart_quick(false),
            KeyCode::Char('a') => self.restart_quick(true),
            KeyCode::Char('s') => self.request_suggestion(),
            KeyCode::Char('d') => self.show_debug = !self.show_debug,
            KeyCode::Char('h') => self.show_heatmap = !self.show_heatmap,
            KeyCode::Char('b') => self.show_breakdown = !self.show_breakdown,
            KeyCode::Char('1') => self.try_resolve_choice(ColorDecision::TakeBlack),
            KeyCode::Char('2') => self.try_resolve_choice(ColorDecision::TakeWhite),
            KeyCode::Char('3') => self.try_resolve_choice(ColorDecision::PlaceTwoMore),
            KeyCode::Esc => {
                self.suggestion = None;
                self.status = None;
            }
            KeyCode::Left => self.move_cursor(-1, 0),
            KeyCode::Right => self.move_cursor(1, 0),
            KeyCode::Up => self.move_cursor(0, -1),
            KeyCode::Down => self.move_cursor(0, 1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                let idx = self.cursor;
                self.try_human_click(idx);
            }
            _ => {}
        }
    }

    fn move_cursor(&mut self, dx: i32, dy: i32) {
        let Some((x, y)) = index_to_xy(self.cursor) else {
            self.cursor = xy_to_index(BOARD_SIZE / 2, BOARD_SIZE / 2);
            return;
        };
        let nx = (x as i32 + dx).clamp(0, BOARD_SIZE as i32 - 1) as usize;
        let ny = (y as i32 + dy).clamp(0, BOARD_SIZE as i32 - 1) as usize;
        self.cursor = xy_to_index(nx, ny);
    }

    fn try_resolve_choice(&mut self, decision: ColorDecision) {
        let Some(game) = &self.game else { return };
        let Some(pending) = game.pending_choice() else { return };
        if game.pending_choice_is_ai_turn() {
            return; // c'est à l'IA de décider, pas à l'humain.
        }
        if matches!(decision, ColorDecision::PlaceTwoMore) && pending != PendingChoice::Swap2FirstDecision {
            return; // cette option n'existe qu'au premier choix de Swap2.
        }
        let Some(game) = &mut self.game else { return };
        game.apply_color_decision(decision, false);
        self.status = None;
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        if self.screen != Screen::Playing {
            return;
        }
        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind
            && let Some((x, y)) = ui::goban::screen_to_cell(self.goban_area, mouse.column, mouse.row)
        {
            let index = xy_to_index(x, y);
            self.cursor = index;
            self.try_human_click(index);
        }
    }

    fn try_human_click(&mut self, index: usize) {
        if !self.human_may_act() {
            self.status = Some(self.explain_why_human_cannot_act());
            return;
        }
        let Some(game) = &mut self.game else { return };
        match game.try_play(index) {
            Ok(report) => {
                self.last_move_highlight = Some(report.index);
                self.just_captured = report.captured_indices;
                self.captured_ticks = if self.just_captured.is_empty() { 0 } else { CAPTURE_HIGHLIGHT_TICKS };
                self.suggestion = None;
                self.status = if report.captured_pairs > 0 {
                    Some(format!("{} capture {} paire(s) !", player_label(report.player), report.captured_pairs))
                } else {
                    None
                };
            }
            Err(e) => {
                self.status = Some(e);
            }
        }
    }

    /// Message explicatif quand un clic (ou la touche Entrée sur le
    /// curseur) est refusé, en distinguant les causes possibles : c'est
    /// `human_may_act` qui décide *si* on peut jouer, cette fonction ne fait
    /// que reformuler la raison la plus probable pour l'utilisateur.
    fn explain_why_human_cannot_act(&self) -> String {
        if self.ai_job.is_some() {
            return "l'IA réfléchit, patientez...".to_string();
        }
        let Some(game) = &self.game else { return "aucune partie en cours".to_string() };
        if game.outcome.is_over() {
            return "la partie est terminée (touche r ou a pour rejouer)".to_string();
        }
        if game.pending_choice().is_some() {
            return "une décision de couleur est en attente : touches 1/2/3".to_string();
        }
        "ce n'est pas votre tour".to_string()
    }

    // ---------------------------------------------------------------
    // Aides pour l'affichage (grisage des cases interdites, décomposition)
    // ---------------------------------------------------------------

    /// Vrai si `index` serait actuellement refusé par la règle d'ouverture
    /// active (Pro / Long Pro), pour griser la case côté interface (section
    /// 5 du sujet : "cases interdites grisées").
    pub fn opening_forbids(&self, index: usize) -> bool {
        let Some(game) = &self.game else { return false };
        if game.outcome.is_over() {
            return false;
        }
        let moves_played = game.position.stone_count as u32;
        game.opening.check_placement(moves_played, index).is_err()
    }

    pub fn score_breakdown(&self) -> Option<eval::Breakdown> {
        self.game.as_ref().map(|g| eval::breakdown(&g.position))
    }

    pub fn heatmap_color(&self, index: usize) -> Option<ratatui::style::Color> {
        let stats = self.last_search_stats.as_ref()?;
        if stats.root_scores.len() < 2 {
            return None;
        }
        let score = stats.root_scores.iter().find(|&&(i, _)| i == index)?.1;
        let min = stats.root_scores.iter().map(|&(_, s)| s).min()?;
        let max = stats.root_scores.iter().map(|&(_, s)| s).max()?;
        if max <= min {
            return None;
        }
        let t = ((score - min) as f64 / (max - min) as f64).clamp(0.0, 1.0);
        let r = (200.0 * (1.0 - t)) as u8;
        let g = (200.0 * t) as u8;
        Some(ratatui::style::Color::Rgb(r + 30, g + 30, 30))
    }
}

/// Boucle applicative principale : dessine, attend un événement, réagit,
/// recommence. `events` reste un thread indépendant (voir
/// `event_thread::EventThread`, inchangé par rapport à la base fournie) qui
/// garantit un tic toutes les 250 ms même en l'absence d'entrée clavier ou
/// souris, ce qui permet de faire progresser la recherche IA en tâche de
/// fond sans jamais bloquer le rendu.
pub fn run(terminal: &mut DefaultTerminal, events: &EventThread) -> io::Result<()> {
    let mut app = App::new();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;
        match events.read() {
            Ok(Event::Tick) => app.on_tick(),
            Ok(Event::Key(key)) => app.on_key(key),
            Ok(Event::Mouse(mouse)) => app.on_mouse(mouse),
            Err(_) => {
                // Le thread d'événements a fermé son canal (ne devrait
                // arriver qu'à la fermeture du terminal) : on quitte
                // proprement plutôt que de tourner en boucle sur une erreur.
                app.should_quit = true;
            }
        }
    }
    Ok(())
}
