# Gomoku

Un Gomoku (règles Ninuki-Renju : alignement de 5+, captures de paires,
victoire à 10 pierres capturées) avec une IA negamax + alpha-bêta + PVS,
jouable contre l'IA ou en hotseat, avec les 5 règles d'ouverture du bonus
(Standard, Pro, Long Pro, Swap, Swap2).

Voir [`DEFENSE.md`](DEFENSE.md) pour l'explication complète et détaillée du
moteur (représentation du plateau, minimax pas à pas, heuristique, mesures
de performance réelles, compromis assumés, questions probables de
soutenance).

## Compilation et lancement

```sh
make            # compile en release, produit ./Gomoku
./Gomoku
```

```sh
make re         # fclean + all
make fclean     # supprime le binaire et target/
make test       # cargo test --release (61 tests, 58 exécutés par défaut)
make perf       # test de performance dédié (profondeur >= 10 en < 500 ms
                # sur 5 positions de référence), volontairement séparé de
                # `make test` : le mesurer en parallèle des autres tests
                # fausserait le temps observé.
make clippy     # cargo clippy --release --all-targets
```

Un second `make` consécutif sans modification ne recompile rien
(`make: Nothing to be done for 'all'`).

Prérequis : Rust stable (édition 2024), `cargo`. Aucune dépendance externe
autre que `ratatui` (interface) et `crossterm` (terminal), déjà déclarées
dans `Cargo.toml`.

## Écran de configuration

Au lancement, un écran de configuration permet de choisir :

- **Mode** : Humain vs IA (avec la couleur de l'humain) ou Humain vs Humain
  (hotseat).
- **Règle d'ouverture** : Standard, Pro, Long Pro, Swap, Swap2.
- **Budget de temps de l'IA** (soft/hard, en ms) et **nombre de threads**.

Navigation : flèches haut/bas pour choisir une ligne, gauche/droite pour
changer la valeur, `Entrée` pour démarrer la partie.

## Commandes en partie

| Touche / action | Effet |
|---|---|
| Clic gauche sur une case | Jouer à cette case |
| Flèches + `Entrée`/`Espace` | Déplacer le curseur puis jouer à sa position (jeu 100% clavier, sans souris) |
| `s` | Suggestion de coup pour le joueur au trait (surbrillance verte, sans jouer) — fonctionne pour les deux couleurs en hotseat |
| `d` | Afficher/masquer le panneau de débogage IA (profondeur, nœuds, score, PV, taux de TT, meilleurs coups à la racine) |
| `h` | Afficher/masquer la carte de chaleur des scores de la racine sur le goban |
| `b` | Afficher/masquer le détail du score par catégorie (positionnel, captures, vulnérabilité) |
| `1` / `2` / `3` | Répondre à une décision de couleur Swap/Swap2 (Noir / Blanc / placer 2 pierres de plus) |
| `r` | Relancer une partie Humain vs Humain avec les réglages actuels |
| `a` | Relancer une partie Humain vs IA avec les réglages actuels |
| `n` | Retourner à l'écran de configuration |
| `Échap` | Effacer la suggestion / le message de statut affiché |
| `q` | Quitter |

Le panneau de gauche journalise chaque coup, capture et décision. Le
panneau de droite affiche en permanence le temps de réflexion du dernier
coup de l'IA, sa moyenne et son maximum sur la partie (l'indicateur passe en
rouge au-delà de 500 ms), ainsi que l'état de la partie et, si activé, le
détail de la dernière recherche.

## Règles implémentées

- Plateau 19x19, alignement de **5 pierres ou plus** sur une des 4
  directions.
- **Capture de paires** : poser une pierre qui encadre exactement deux
  pierres adverses (`X O O X`) les retire du plateau, dans les 8 directions
  à la fois si besoin. Aucune autocapture en jouant dans un sandwich
  adverse (`O _ O` → `O X O`).
- **Victoire par capture** à 10 pierres adverses capturées (5 paires).
- **Fin de partie liée aux captures** (section 3.4 du sujet) : un alignement
  de 5+ qui peut encore être cassé par une capture adverse touchant la ligne
  ne gagne pas immédiatement ; si l'auteur de l'alignement a déjà perdu 4
  paires et que l'adversaire dispose d'un coup de capture, ce dernier gagne
  immédiatement.
- **Interdiction du double-trois** (deux "trois libres" ou plus créés par le
  même coup), sauf si ce coup capture une paire ou forme un alignement de 5+
  (ces deux cas restent toujours légaux).
- **Match nul** si le joueur au trait n'a plus aucun coup légal.
- **5 règles d'ouverture** : Standard, Pro, Long Pro, Swap, Swap2 — avec
  contraintes de placement affichées (cases grisées) et l'IA capable de
  choisir sa couleur quand c'est à elle de le faire (Swap/Swap2).

Chacune de ces règles est couverte par au moins un test unitaire nommé
explicitement — voir `DEFENSE.md`, section 6, pour la correspondance
règle → fonction → test.

## Moteur IA

Negamax + élagage alpha-bêta + recherche à fenêtre nulle (PVS) +
approfondissement itératif avec fenêtre d'aspiration + table de
transposition sans verrou + extension de quiescence bornée + parallélisme
Lazy SMP (`std::thread::scope`, aucun `unsafe`). Atteint de façon robuste
une profondeur **>= 10** en moins de **500 ms** sur les 5 positions de
référence (voir mesures réelles dans `DEFENSE.md`).

## Robustesse

Aucun `unwrap`/`expect`/`panic!`/indexation non protégée en dehors des
tests et de `event_thread.rs` (thread d'événements bas niveau, dont l'échec
éventuel ne fait que fermer proprement le canal d'événements plutôt que de
crasher — voir `DEFENSE.md`, section 6). Hook de panique qui restaure le
terminal avant tout affichage d'erreur. Recherche IA toujours enveloppée
dans `catch_unwind`, avec un coup légal de repli en cas d'imprévu. Table de
transposition qui dégrade proprement sa taille en cas d'échec d'allocation
au lieu de paniquer. Terminal trop petit détecté et signalé au lieu d'être
dessiné n'importe comment.

## Organisation du code

```
src/
  main.rs            point d'entrée, hook de panique, mouse capture
  app.rs              état applicatif, boucle d'événements, jobs IA en thread séparé
  event_thread.rs      thread d'événements clavier/souris/tick (fourni, inchangé)
  ui/                  rendu ratatui (goban, panneaux, menu)
  game/
    position.rs        plateau matelassé 27x27, make/unmake, Zobrist, score incrémental
    patterns.rs         classification des motifs, détection du trois libre
    rules.rs            captures, double-trois, alignement, fin de partie, nul
    openings.rs         règles d'ouverture (bonus)
    state.rs            Game : orchestration d'une partie complète
  ai/
    eval.rs             évaluation heuristique
    movegen.rs          génération et ordonnancement des coups candidats
    search.rs           negamax, alpha-bêta, PVS, itératif, quiescence, Lazy SMP
    tt.rs                table de transposition sans verrou
```
