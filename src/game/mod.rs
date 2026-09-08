//! Le module `game` contient tout ce qui définit les règles de Gomoku
//! elles-mêmes, indépendamment de l'IA et de l'interface :
//!
//! - [`position`] : la représentation du plateau et les primitives de bas
//!   niveau (placer/retirer une pierre, captures, score incrémental).
//! - [`patterns`] : la classification des alignements (score positionnel,
//!   détection du trois libre).
//! - [`rules`] : les règles de haut niveau (légalité, victoire, match nul).
//! - [`openings`] : les règles d'ouverture optionnelles (bonus).
//! - [`state`] : `Game`, qui orchestre tout cela pour une partie complète
//!   (historique, journal, règle d'ouverture active, alignement en attente).

pub mod openings;
pub mod patterns;
pub mod position;
pub mod rules;
pub mod state;

pub use state::{Game, GameMode, Outcome};
