//! L'IA : évaluation, génération/ordonnancement de coups, recherche
//! negamax + alpha-bêta, table de transposition, parallélisme.
//!
//! Ce module ne connaît que `game::position::Position` (et les prédicats
//! purs de `game::rules`) : il ignore tout du mode de jeu, de l'interface ou
//! des règles d'ouverture, qui restent la responsabilité de `game::state`.

pub mod eval;
pub mod movegen;
pub mod search;
pub mod tt;
