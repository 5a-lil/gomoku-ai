//! Table de transposition sans verrou, partagée entre les threads de
//! recherche (voir `ai::search`, parallélisme "Lazy SMP").
//!
//! Schéma classique "XOR" pour une table lock-free en Rust sûr (pas
//! d'`unsafe`) : chaque emplacement stocke deux `AtomicU64`, `key` et
//! `data`. À l'écriture, on stocke `data` puis `key = zobrist ^ data`. À la
//! lecture, on recharge les deux et on vérifie que `key ^ data == zobrist`
//! attendu : si un autre thread a écrit entre les deux chargements, la
//! vérification échoue et l'entrée est simplement ignorée (jamais une valeur
//! corrompue n'est utilisée). C'est une dégradation sans risque : au pire on
//! perd un coup de table, jamais la correction du résultat.

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bound {
    Exact,
    Lower,
    Upper,
}

#[derive(Debug, Clone, Copy)]
pub struct TtEntry {
    pub score: i32,
    pub depth: u8,
    pub bound: Bound,
    pub best_move: Option<usize>,
}

fn pack(entry: &TtEntry) -> u64 {
    let score = entry.score as i64 as u64 & 0xFFFF_FFFF;
    let depth = entry.depth as u64;
    let bound = match entry.bound {
        Bound::Exact => 0u64,
        Bound::Lower => 1u64,
        Bound::Upper => 2u64,
    };
    let mv = entry.best_move.map(|m| m as u64 + 1).unwrap_or(0); // 0 = aucun coup, sinon m+1
    score | (depth << 32) | (bound << 40) | (mv << 42)
}

fn unpack(data: u64) -> TtEntry {
    let score = (data & 0xFFFF_FFFF) as u32 as i32;
    let depth = ((data >> 32) & 0xFF) as u8;
    let bound = match (data >> 40) & 0b11 {
        0 => Bound::Exact,
        1 => Bound::Lower,
        _ => Bound::Upper,
    };
    let mv_raw = (data >> 42) & 0x3FF; // 10 bits suffisent (max index 728)
    let best_move = if mv_raw == 0 { None } else { Some((mv_raw - 1) as usize) };
    TtEntry { score, depth, bound, best_move }
}

struct Slot {
    key: AtomicU64,
    data: AtomicU64,
}

/// Table de transposition. `mask` est la taille (en emplacements) moins un :
/// la taille est toujours une puissance de 2 pour que l'indexation par
/// masque de bits soit une simple opération `&`, sans division.
pub struct TranspositionTable {
    slots: Vec<Slot>,
    mask: usize,
}

impl TranspositionTable {
    /// Plafond dur sur le nombre d'emplacements, quoi que demande
    /// l'appelant : 2^24 emplacements de 16 octets = 256 Mo, largement
    /// suffisant pour une table de transposition de Gomoku. Sans ce plafond,
    /// une requête absurde (mauvaise config, ou le test qui la simule)
    /// pourrait passer `try_reserve_exact` avec succès -- la réservation
    /// d'espace d'adressage virtuel réussit souvent même pour une taille
    /// délirante, sur-engagement (`overcommit`) du système -- puis se faire
    /// tuer par l'OOM killer de l'OS (un vrai SIGKILL, non rattrapable en
    /// Rust) au moment où l'on écrit réellement dans chaque case pour
    /// l'initialiser. Ce plafond garantit qu'on ne tente jamais une telle
    /// allocation en premier lieu.
    const MAX_ENTRIES: usize = 1 << 24;

    /// Tente de créer une table d'environ `target_entries` emplacements
    /// (arrondi à la puissance de 2 inférieure, plafonné à `MAX_ENTRIES`).
    /// Dégrade proprement en cas d'échec d'allocation : divise la taille par
    /// 4 et réessaie, jusqu'à un plancher de 2^10 ; si même cela échoue,
    /// renvoie une table minimale de taille 1 (la recherche reste correcte
    /// sans table utile, juste plus lente). Le sujet interdit tout crash, y
    /// compris par manque de mémoire : on ne fait jamais `Vec::with_capacity`
    /// suivi d'un `unwrap`.
    pub fn new(target_entries: usize) -> Self {
        let target_entries = target_entries.min(Self::MAX_ENTRIES);
        let mut size = target_entries.next_power_of_two().clamp(1, Self::MAX_ENTRIES);
        loop {
            match Self::try_allocate(size) {
                Some(slots) => {
                    return TranspositionTable { slots, mask: size - 1 };
                }
                None => {
                    if size <= 1024 {
                        // Dernier recours : une table riquiqui mais qui ne
                        // peut raisonnablement pas échouer à s'allouer.
                        let slots = Self::try_allocate(1).unwrap_or_else(|| {
                            vec![Slot { key: AtomicU64::new(0), data: AtomicU64::new(0) }]
                        });
                        let mask = slots.len().saturating_sub(1).max(0);
                        return TranspositionTable { slots, mask };
                    }
                    size /= 4;
                }
            }
        }
    }

    fn try_allocate(size: usize) -> Option<Vec<Slot>> {
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(size)
            .ok()
            .map(|_| {
                for _ in 0..size {
                    slots.push(Slot { key: AtomicU64::new(0), data: AtomicU64::new(0) });
                }
                slots
            })
    }

    #[inline]
    fn index(&self, zobrist: u64) -> usize {
        (zobrist as usize) & self.mask
    }

    pub fn probe(&self, zobrist: u64) -> Option<TtEntry> {
        let slot = &self.slots[self.index(zobrist)];
        let key = slot.key.load(Ordering::Relaxed);
        let data = slot.data.load(Ordering::Relaxed);
        if key ^ data == zobrist {
            Some(unpack(data))
        } else {
            None
        }
    }

    /// Enregistre une entrée. Stratégie de remplacement : toujours écraser
    /// si l'ancienne entrée est moins profonde (une recherche plus profonde
    /// est plus fiable), sinon conserver l'existante. L'emplacement étant
    /// partagé entre threads, deux écritures concurrentes peuvent
    /// s'entrelacer sans risque de corruption exploitée : la prochaine
    /// lecture qui échoue à la vérification XOR ignorera simplement
    /// l'entrée.
    pub fn store(&self, zobrist: u64, entry: TtEntry) {
        let idx = self.index(zobrist);
        let slot = &self.slots[idx];
        let old_data = slot.data.load(Ordering::Relaxed);
        let old_key = slot.key.load(Ordering::Relaxed);
        if old_key ^ old_data == zobrist {
            let old = unpack(old_data);
            if old.depth > entry.depth {
                return;
            }
        }
        let data = pack(&entry);
        slot.data.store(data, Ordering::Relaxed);
        slot.key.store(zobrist ^ data, Ordering::Relaxed);
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrip() {
        let entry = TtEntry { score: -12345, depth: 7, bound: Bound::Lower, best_move: Some(360) };
        let packed = pack(&entry);
        let back = unpack(packed);
        assert_eq!(back.score, entry.score);
        assert_eq!(back.depth, entry.depth);
        assert_eq!(back.bound, entry.bound);
        assert_eq!(back.best_move, entry.best_move);
    }

    #[test]
    fn pack_unpack_sans_coup() {
        let entry = TtEntry { score: 42, depth: 0, bound: Bound::Exact, best_move: None };
        let back = unpack(pack(&entry));
        assert_eq!(back.best_move, None);
        assert_eq!(back.score, 42);
    }

    #[test]
    fn store_puis_probe_retrouve_l_entree() {
        let tt = TranspositionTable::new(1024);
        let entry = TtEntry { score: 555, depth: 4, bound: Bound::Exact, best_move: Some(42) };
        tt.store(0xABCDEF, entry);
        let found = tt.probe(0xABCDEF).expect("entrée attendue");
        assert_eq!(found.score, 555);
        assert_eq!(found.best_move, Some(42));
    }

    #[test]
    fn probe_sur_cle_absente_renvoie_rien() {
        let tt = TranspositionTable::new(1024);
        assert!(tt.probe(0x1234).is_none());
    }

    #[test]
    fn remplacement_prefere_la_plus_grande_profondeur() {
        let tt = TranspositionTable::new(16); // petite table, collisions possibles mais même clé ici
        let key = 0x42;
        tt.store(key, TtEntry { score: 1, depth: 2, bound: Bound::Exact, best_move: None });
        tt.store(key, TtEntry { score: 2, depth: 1, bound: Bound::Exact, best_move: None });
        // La profondeur 1 est inférieure à la profondeur 2 déjà en place :
        // l'entrée ne doit pas être remplacée.
        assert_eq!(tt.probe(key).unwrap().score, 1);
        tt.store(key, TtEntry { score: 3, depth: 5, bound: Bound::Exact, best_move: None });
        assert_eq!(tt.probe(key).unwrap().score, 3);
    }

    #[test]
    fn degrade_proprement_sur_taille_absurde() {
        // Ne doit jamais paniquer, même pour une taille visée déraisonnable.
        let tt = TranspositionTable::new(usize::MAX / 2);
        assert!(!tt.is_empty());
    }
}
