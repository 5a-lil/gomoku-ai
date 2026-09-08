# Mission : finaliser le projet Gomoku (42) — moteur, règles, interface, perfs

Tu travailles dans le dépôt Rust `/Users/wofty/gomoku-ai`, branche `pasprime`. C'est un projet scolaire 42
que le propriétaire devra **défendre oralement en détail** devant un correcteur. Ta mission est de le mener
à un état **complet, correct, rapide et défendable**, en une seule passe.

Lis intégralement ce document avant d'écrire une ligne de code. Il contient l'audit de l'existant, les
spécifications exactes des règles, l'architecture cible, les algorithmes attendus et les critères
d'acceptation. Ne t'écarte pas des spécifications sans le signaler explicitement dans ton rapport final.

---

## 0. Contraintes non négociables (elles viennent du sujet, leur non-respect vaut 0)

1. **L'exécutable doit s'appeler `Gomoku`** et être produit à la racine du dépôt.
2. **Un `Makefile`** doit exister avec au minimum les règles `$(NAME)`, `all`, `clean`, `fclean`, `re`,
   et **il ne doit pas relink** : un second `make` consécutif ne doit rien reconstruire.
3. **L'IA doit chercher au minimum 10 niveaux de profondeur** dans son arbre de jeu. La profondeur
   affichée doit être la profondeur réellement atteinte par la dernière itération **terminée** — jamais
   un chiffre décoratif.
4. **L'IA doit trouver son coup en moins de 500 ms en moyenne.** Vise 250 ms de moyenne pour avoir de
   la marge sur une machine de correction plus lente.
5. **Un timer doit être affiché en permanence dans l'interface**, indiquant le temps de réflexion de
   l'IA pour son dernier coup. Sans timer, le projet n'est pas validé.
6. **Le programme ne doit jamais crasher, en aucune circonstance**, y compris en cas de manque de
   mémoire. Aucun `panic!`, `unwrap()`, `expect()`, indexation hors-bornes ni débordement arithmétique
   ne doit pouvoir être atteint depuis une interaction utilisateur.
7. **L'algorithme doit être un Min-Max** (une variante est acceptée : negamax + alpha-bêta + PVS en est
   une, et c'est ce qui est demandé ici). Il faut pouvoir l'expliquer ligne par ligne.
8. Le jeu doit être jouable **contre l'IA** et **en hotseat à deux humains, avec une fonction de
   suggestion de coup**.
9. Les règles imposées par le sujet doivent être implémentées correctement : alignement de 5 **ou plus**,
   captures de paires, victoire par 10 pierres capturées, interdiction du double-trois, et les règles
   de fin de partie liées aux captures.
10. `unsafe` est **interdit**. Aucune nouvelle dépendance externe : uniquement `std` et les crates déjà
    présentes (`ratatui`, `crossterm`, `chrono`). Le parallélisme se fait avec `std::thread`.

---

## 1. État actuel du dépôt — audit à connaître avant de commencer

### Ce qui existe et qu'il faut préserver dans l'esprit

- Une interface TUI `ratatui` fonctionnelle : goban 19×19 cliquable à la souris, panneau de logs
  horodatés à gauche, panneau de métriques et logo ASCII `Gomoku` à droite, raccourcis `[q]` quitter,
  `[r]` relancer un 1v1, `[a]` relancer un 1vsIA. **Garde cette interface, son logo, son logger et ses
  raccourcis**, et étends-la. Ne la remplace pas par autre chose.
- `src/event_thread.rs` : thread d'événements avec tick de 250 ms. Correct, garde-le tel quel.
- Une logique de captures, de comptage de captures, de détection d'alignement et de match nul, écrite
  à la main dans `src/board.rs`. **La logique métier est à réécrire** (voir plus bas), mais lis-la
  d'abord : elle documente l'intention de l'auteur.

### Les défauts mesurés et identifiés — ils sont la raison de cette mission

**Performance (le blocage principal) :**

- `size_of::<Cell>()` vaut **400 octets**, à cause de `captures: [Capture; 8]` (192 o) et
  `virtual_capturer: [Option<(usize, usize)>; 8]` (192 o). Le plateau complet fait donc **141 Ko** et
  ne tient pas dans le cache L1.
- `Ai::evaluation()` (`src/board/ai/evaluation.rs:47`) retraverse les 361 cases dans les 4 directions
  à chaque feuille. Mesuré : **634 ns** pour horizontal + vertical seuls, soit **~1,3 µs** pour
  l'évaluation complète. Cela plafonne à ~380 000 feuilles en 500 ms, en supposant un coût nul pour
  tout le reste. Pour une profondeur 10, il faut un budget de l'ordre de **100 ns par nœud tout
  compris**. Il manque un facteur 30 : aucune micro-optimisation de la boucle actuelle ne suffira,
  il faut changer de représentation et passer à une évaluation incrémentale.
- `Ai::new()` clone les 141 Ko du plateau à chaque coup.
- Aucune table de transposition, aucun approfondissement itératif, aucun ordonnancement de coups,
  aucun budget de temps.

**Bugs de correction :**

- `static_evaluation_vertical` (`src/board/ai/evaluation.rs:71`) : le `continue` sur case vide saute
  aussi le `i += BOARD_SIZE`. Dès qu'une case vide est rencontrée, l'index se **fige** et les
  itérations restantes relisent la même cellule. L'évaluation verticale est donc quasiment inopérante.
  La variante `test_static_evaluation_vertical` incrémente `i` avant le `continue` : les tests passent
  alors que le code de production est cassé.
- L'heuristique ne distingue pas les extrémités ouvertes des extrémités fermées. `Patterns::from(count)`
  ne mesure que la longueur d'une série : un trois libre `. X X X .` et un trois mort `O X X X O`
  reçoivent le même score de 20 000. C'est le défaut le plus grave de l'heuristique.
- Les captures, qui sont pourtant une condition de victoire, n'apparaissent nulle part dans le score.
- Les scores sont absolus (blanc positif, noir négatif) au lieu d'être relatifs au joueur au trait, ce
  qui est incompatible avec un negamax.
- `Five = 9_250_000` avec `ALPHA_START = i64::MIN` : `ret *= -1` sur `i64::MIN` déborde.
- `src/board/ai.rs:114` : profondeur câblée à 4.
- La génération de coups n'explore que **les 8 voisins immédiats du dernier coup et les 8 voisins du
  coup précédent**. L'IA est structurellement aveugle au reste du plateau : elle ne peut pas aller
  bloquer une menace ailleurs ni ouvrir un second front.
- La recherche ne simule pas les captures : elle fait `board[i].state = White` et rien d'autre. L'IA ne
  peut donc ni gagner par capture, ni éviter de perdre par capture, ni casser un cinq adverse.
- `src/board/ai.rs:220` : `self.best_index` est écrit à chaque nœud. Cela fonctionne par accident
  (la racine écrit en dernier) mais c'est fragile.
- `double_free_three` (`src/board.rs:994`) n'est pas un détecteur de trois libre : il compte, dans
  chacune des 8 directions, le nombre de pierres amies parmi les 3 cases suivantes, et renvoie
  `found == 2`. Conséquences : `X X _ X X` est déclaré double-trois donc **interdit, alors que le coup
  fait cinq et gagne** ; aucune vérification des extrémités libres ni de la contiguïté ; les 8
  directions sont comptées séparément au lieu des 4 axes, donc un même axe compte double ; `== 2` au
  lieu de `>= 2` laisse passer un triple-trois ; l'exception « capturer une paire autorise le
  double-trois » est absente ; et la règle n'est jamais appliquée aux coups de l'IA.
- `check_lines` appelle `is_capturable` **dans la condition d'un `while`**, alors que `is_capturable`
  mute le plateau via `virtual_capturing_add`. Prédicat à effets de bord dans une boucle de comptage.
  De plus il interrompt le comptage au lieu de marquer la victoire comme « en attente ».
- `Captured::decr` fait `panic!` sur `State::Empty` et un `u32 -= 1` qui peut passer sous zéro.
  `delete_capture` et `capture` font `.unwrap()` sur un `find` qui peut échouer.

**Non-conformités au sujet :**

- Makefile non conforme (`all: cargo test && cargo run`, `clean:` vide, pas de `$(NAME)`, exécutable
  nommé `gomoku`).
- Timer affiché en « ms » mais mesuré en microsecondes pour l'humain et en millisecondes pour l'IA,
  et basé sur `chrono::Utc::now()` (horloge murale) au lieu de `std::time::Instant`.
- Pas de suggestion de coup en hotseat.
- Pas de vue de débogage du raisonnement de l'IA.
- `num-bigint` est déclaré dans `Cargo.toml` mais inutilisé.
- 11 warnings à la compilation.
- Aucun bonus.

---

## 2. Architecture cible

Réécris le cœur du projet avec cette organisation. Garde `event_thread.rs`, adapte l'interface,
remplace la logique métier et le moteur.

```
src/
  main.rs                 point d'entrée, hook de panique, init/restore du terminal
  app.rs                  état applicatif, boucle d'événements, dispatch (ex-contenu de main.rs)
  ui/
    mod.rs
    goban.rs              widget du plateau (reprends le rendu existant, ajoute les surbrillances)
    panels.rs             logs, métriques, panneau debug IA, aide
    menu.rs               écran de configuration de partie (mode, règle d'ouverture, couleur, budget)
  game/
    mod.rs                Game : partie en cours, tour, historique, mode, arbitrage
    position.rs           Position : plateau, make/unmake, Zobrist, score incrémental
    rules.rs              captures, double-trois, victoire par alignement, fin de partie
    patterns.rs           tables précalculées et classification O(1) des motifs
    openings.rs           Standard / Pro / Long Pro / Swap / Swap2
  ai/
    mod.rs                Engine : API publique (recherche, suggestion)
    search.rs             negamax + alpha-bêta + PVS + itératif + fenêtre d'aspiration + quiescence
    movegen.rs            génération et ordonnancement des candidats
    eval.rs               évaluation
    tt.rs                 table de transposition sans verrou
    zobrist.rs            clés de hachage
  event_thread.rs         inchangé
```

### 2.1 `Position` — la structure centrale

C'est le point le plus important de la mission. Conception imposée :

- **Plateau matelassé (padded).** Un tableau `cells: [u8; 27 * 27]` (729 octets, tient entièrement en
  cache L1). Le goban 19×19 est logé au centre, entouré d'une bordure de 4 cases de sentinelles.
  Valeurs : `EMPTY = 0`, `BLACK = 1`, `WHITE = 2`, `WALL = 3`.
  - Index d'une coordonnée : `idx(x, y) = (y + 4) * 27 + (x + 4)`.
  - Décalages directionnels : `1` (horizontal), `27` (vertical), `28` (diagonale ↘), `26` (diagonale ↗).
  - La bordure de 4 cases garantit qu'une fenêtre de 9 cases centrée sur n'importe quelle case du
    goban est toujours lisible. **Cela supprime tout test de bornes dans les boucles chaudes** et
    élimine des dizaines de lignes de calculs de modulo comme ceux de `check_up_right` et compagnie.
- **État de partie** : `to_move: Player`, `pairs_captured: [u8; 2]`, `zobrist: u64`,
  `score: i32` (score incrémental), `stone_count: u16`.
- **Proximité incrémentale** : `neighbour_count: [u8; 27 * 27]`. Lors du placement d'une pierre en `i`,
  incrémente le compteur des cases situées à distance de Tchebychev ≤ 2 de `i` ; décrémente lors du
  retrait. Une case est candidate si et seulement si `neighbour_count[i] > 0`. Maintenance O(1)
  amortie, génération de candidats immédiate.
- **`make_move` / `unmake_move`** : jouer et défaire un coup **sans copier le plateau**. `make_move`
  renvoie (ou empile) un `MoveUndo { index, captures: [u8; 8] /* directions ayant capturé */,
  captured_count, zobrist_before, score_before, neighbour_delta_ref }` suffisant pour restaurer
  exactement l'état antérieur. Écris un test qui joue une longue séquence aléatoire de coups puis les
  défait tous et vérifie que le plateau, le score, le Zobrist, les compteurs de captures et les
  compteurs de voisinage sont **strictement identiques** à l'état initial. Ce test est ta garantie
  la plus importante.
- `Position` doit être `Clone` et faire moins de 2 Ko, pour pouvoir être copiée par thread sans coût.

### 2.2 `patterns.rs` — tables précalculées

Deux tables, construites une seule fois au démarrage (`std::sync::OnceLock`, pas de `lazy_static`).

**Table A — classification de motif, fenêtre de 9 cases.**
Pour une case donnée et un axe donné, extrais la fenêtre des 9 cases `[i-4d, …, i, …, i+4d]` et encode-la
en **base 3** relativement au joueur considéré : `0` = vide, `1` = pierre du joueur, `2` = pierre
adverse **ou** sentinelle (un bord bloque exactement comme une pierre adverse). L'index obtenu tient
dans `0..3^9 = 19683`. La table `PATTERN_KIND: [PatternKind; 19683]` (19 Ko, tient en L1) donne, en O(1),
la nature de la menace que la case centrale constitue sur cet axe :

```rust
enum PatternKind {
    None,
    Two,          // deux pierres, potentiel faible
    OpenTwo,      // _XX_ avec de la place des deux côtés
    ClosedThree,  // trois dont une extrémité est bloquée
    BrokenThree,  // _XX_X_ ou _X_XX_ : trois libre à trou
    OpenThree,    // _XXX_ extensible en quatre libre
    Four,         // quatre avec une seule extrémité libre, ou quatre à trou
    OpenFour,     // _XXXX_ : indéfendable
    Five,         // cinq ou plus alignés
}
```

Le générateur de cette table doit être écrit comme une fonction de classification lisible et
**couverte par des tests unitaires exhaustifs** sur les motifs du sujet. C'est cette table qui sert à :
la détection de victoire en O(1), la détection du trois libre pour la règle du double-trois,
l'ordonnancement des coups, et la recherche de menaces en quiescence.

**Table B — score de fenêtre, fenêtre de 6 cases.**
`WINDOW_SCORE: [i32; 729]` indexée par une fenêtre de 6 cases encodée en base 3 avec
`0` = vide, `1` = noir, `2` = blanc **ou** sentinelle. La valeur est le score de la fenêtre **du point
de vue de Noir** (positif si favorable à Noir, négatif si favorable à Blanc, 0 si la fenêtre est mixte
ou vide). Une fenêtre contenant les deux couleurs vaut 0 : elle est morte pour tout le monde.

**Évaluation incrémentale.** `Position::score` est la somme de `WINDOW_SCORE` sur toutes les fenêtres
de 6 cases de toutes les lignes des 4 axes. Quand une case change de valeur :

```
pour chaque axe d in [1, 27, 28, 26]:
    pour offset in -5..=0:
        score -= WINDOW_SCORE[encode6(i + offset*d, d)]   // avant mutation
mute la case
pour chaque axe d in [1, 27, 28, 26]:
    pour offset in -5..=0:
        score += WINDOW_SCORE[encode6(i + offset*d, d)]   // après mutation
```

Soit 24 lectures de table avant et 24 après, par case modifiée. Un coup modifie 1 case, plus 2 cases
par paire capturée. Coût typique : **moins de 100 ns**, contre 1,3 µs aujourd'hui. Applique la même
routine `apply_cell_delta` pour le placement et pour chaque pierre capturée : comme les fenêtres sont
relues depuis l'état courant du plateau à chaque appel, le traitement case par case reste correct même
quand plusieurs cases d'une même fenêtre changent.

Attention : la bordure doit faire 4 cases de large pour la fenêtre de 9 ; les fenêtres de 6 en profitent
aussi. La lecture de la fenêtre pour l'encodage doit être écrite pour être déroulée par le compilateur
(boucle de longueur constante, pas de `Vec`, pas d'itérateur alloué).

### 2.3 Barème de l'heuristique

Ce barème est un point de départ **à mesurer et à ajuster** ; documente les valeurs finales et la
raison de chaque ajustement dans `DEFENSE.md`.

| Motif | Score |
|---|---|
| Cinq ou plus | traité comme terminal, pas via le barème |
| Quatre libre `_XXXX_` | 100 000 |
| Quatre simple / quatre à trou | 10 000 |
| Trois libre `_XXX_` | 5 000 |
| Trois libre à trou `_XX_X_` | 4 000 |
| Trois fermé | 500 |
| Deux libre | 200 |
| Deux fermé | 50 |

Termes additionnels, à ajouter en dehors de la somme des fenêtres :

- **Captures.** C'est une condition de victoire, donc la progression doit être fortement non linéaire.
  Barème progressif par nombre de paires prises : `[0, 1_500, 4_000, 9_000, 25_000]` pour 0 à 4 paires.
  5 paires est terminal.
- **Vulnérabilité aux captures.** Pénalise chaque paire du camp évalué qui est immédiatement capturable
  par l'adversaire (motif `X O O _` avec la case vide jouable). Sans ce terme, l'IA construit des
  alignements qui se font casser au coup suivant.
- **Tempo léger vers le centre**, quelques points, pour départager les coups équivalents en ouverture.

**Convention de signe : l'évaluation renvoyée à la recherche est toujours relative au joueur au trait**
(`eval(pos) = pos.score * signe(to_move)` + termes relatifs). C'est indispensable pour le negamax.
Toutes les valeurs tiennent dans un `i32`. Constantes :
`INF = 1_000_000_000`, `WIN = 900_000_000`. Un score de victoire est renvoyé comme `WIN - ply` afin de
préférer les victoires rapides, et une défaite comme `-WIN + ply`. Vérifie qu'aucune négation ne peut
déborder (n'utilise **jamais** `i32::MIN` comme alpha).

### 2.4 `search.rs` — le moteur

Structure imposée :

```
fn search(&mut self, pos: &Position, budget: TimeBudget) -> SearchResult
```

- **Approfondissement itératif** de la profondeur 1 jusqu'à `MAX_DEPTH` (≥ 10, prévois 16 comme
  plafond). Le meilleur coup conservé est celui de la dernière itération **terminée** ; une itération
  interrompue par le temps est jetée (sauf si elle a déjà amélioré le premier coup, cas classique que
  tu peux gérer ou ignorer — dis lequel dans `DEFENSE.md`).
- **Fenêtre d'aspiration** autour du score de l'itération précédente (±50), avec réouverture
  progressive en cas d'échec haut ou bas.
- **Negamax avec alpha-bêta** et **PVS** (recherche à fenêtre nulle sur les coups suivant le premier,
  ré-ouverture complète en cas d'échec haut).
- **Table de transposition** (voir 2.5), sondée avant génération, alimentée après recherche avec le
  drapeau `Exact` / `LowerBound` / `UpperBound`.
- **Détection de terminal en O(1)** sur le dernier coup joué, via `PATTERN_KIND` : cinq aligné,
  5 paires capturées, absence de coup légal.
- **Extension de quiescence bornée** à la profondeur 0 : si le camp au trait a un quatre, ou si
  l'adversaire a un quatre ou un quatre libre, prolonge la recherche en ne considérant que les coups
  forcés (faire cinq, bloquer un cinq, faire un quatre libre, casser un cinq par capture, prendre la
  5ᵉ paire). Plafonne l'extension à 8 demi-coups. Sans cela, l'IA subit l'effet d'horizon et rend un
  score mensonger.
- **Contrôle du temps** : vérifie l'horloge toutes les 2048 nœuds via un `AtomicBool` d'arrêt partagé.
  Deux échéances : **soft 380 ms** (ne pas démarrer une nouvelle itération au-delà) et
  **hard 480 ms** (interrompre immédiatement). Utilise `std::time::Instant`, jamais `chrono`.
- **Statistiques** renvoyées : profondeur atteinte, nœuds visités, nœuds/s, score, **variante
  principale complète** (extraite via la TT ou une table de PV triangulaire), taux de succès de la TT,
  temps écoulé, nombre de coups candidats à la racine.

### 2.5 Ordonnancement et limitation des candidats

C'est ce qui rend la profondeur 10 atteignable. Le facteur de branchement brut est de l'ordre de 100 ;
il faut le ramener à 10-14.

Génération : toutes les cases avec `neighbour_count > 0` et **légales** (le double-trois est filtré
dès la génération). Si le plateau est vide, joue le centre.

Score d'ordonnancement, du plus fort au plus faible :

1. Coup de la table de transposition : priorité absolue.
2. Faire cinq : `10_000_000`.
3. Bloquer un cinq adverse : `9_000_000`.
4. Capture qui atteint la 5ᵉ paire : `8_000_000`.
5. Faire un quatre libre : `1_000_000`.
6. Bloquer un quatre libre adverse : `900_000`.
7. Coups de tueur (*killer moves*) du même niveau, 2 par niveau : `100_000`.
8. Captures : `50_000` par paire prise.
9. Somme des gains de motif sur les 4 axes, via `PATTERN_KIND`, pour soi et pour l'adversaire
   (un coup qui crée une menace **et** en bloque une est doublement bon).
10. Heuristique d'historique : `history[player][index]`, incrémentée de `depth * depth` à chaque coup
    qui provoque une coupure bêta.

Trie par score décroissant, puis **ne garde que les K premiers** : `K = 20` aux niveaux 0-1,
`K = 14` aux niveaux 2-3, `K = 10` au-delà. Les coups des catégories 2 à 6 ne sont **jamais** élagués,
quel que soit K, sinon l'IA rate des victoires et des blocages forcés.

Cette limitation est un élagage heuristique, donc non exact : **documente-la explicitement comme un
compromis assumé dans `DEFENSE.md`**, avec les mesures qui le justifient. Un correcteur qui découvre
cela sans explication le prendra pour une triche ; expliqué et mesuré, c'est une décision d'ingénierie.

### 2.6 `tt.rs` — table de transposition sans verrou

- Clés de Zobrist : `[[u64; 729]; 2]` pour les pierres, plus une clé de trait, plus des clés pour les
  compteurs de paires capturées (`[[u64; 6]; 2]`). Générateur pseudo-aléatoire déterministe écrit à la
  main (xorshift64 ou splitmix64 avec une graine fixe) pour que les tests soient reproductibles.
- Entrée : score `i32`, profondeur `u8`, drapeau (`Exact` / `Lower` / `Upper`), meilleur coup `u16`,
  âge `u8` — le tout empaqueté dans un `u64` de données.
- Stockage partagé entre threads en **Rust sûr** : `Vec<(AtomicU64, AtomicU64)>`, schéma XOR sans
  verrou. À l'écriture : `data.store(d)` puis `key.store(zobrist ^ d)`. À la lecture :
  `k = key.load(); d = data.load(); if k ^ d == zobrist { entrée valide }`. Une course produit une
  entrée rejetée, jamais une corruption d'état.
- Taille par défaut : 2^21 entrées (~32 Mo). **L'allocation doit dégrader proprement** : essaie
  2^21, puis 2^19, puis 2^17, puis renonce à la TT et continue sans elle. Aucune panique sur OOM.
  N'utilise pas `vec![]` avec un `unwrap` : construis progressivement avec `Vec::try_reserve`.
- Remplacement : préfère la profondeur supérieure, à âge égal ; écrase toujours une entrée d'un
  âge antérieur.

### 2.7 Parallélisme

`std::thread::scope`, **sans `unsafe`, sans nouvelle crate**.

- Schéma **Lazy SMP** : `N = std::thread::available_parallelism()` bornée à 8. Chaque thread possède
  sa propre `Position` clonée (moins de 2 Ko) et effectue le même approfondissement itératif, avec un
  léger décalage de profondeur pour les threads auxiliaires (`+0, +1, +0, +1, …`) et une perturbation
  déterministe de l'ordre des coups à la racine. Tous partagent la table de transposition et le
  drapeau d'arrêt. Le thread 0 est celui dont on retient le résultat.
- **Un mode monothread doit exister et être exact** : `Engine::set_threads(1)`. Tous les tests
  de recherche tournent en monothread pour être déterministes.
- Aucun `JoinHandle` ne doit pouvoir laisser l'application bloquée : le drapeau d'arrêt est armé sur
  l'échéance dure, et `scope` garantit la jointure.

---

## 3. Spécification exacte des règles — implémente-les à la lettre

C'est là que les points se perdent en soutenance. Chaque point ci-dessous doit avoir **au moins un
test unitaire nommé explicitement**.

### 3.1 Alignement

Un alignement de **5 pierres ou plus** de la même couleur, sur un des 4 axes, est une condition de
victoire (sous réserve de 3.4).

### 3.2 Captures

Le joueur `X` vient de poser une pierre en `P`. Pour **chacune des 8 directions** `d` :

> si `cells[P + d] == O` et `cells[P + 2d] == O` et `cells[P + 3d] == X`,
> alors les deux pierres adverses en `P + d` et `P + 2d` sont retirées du plateau.

Points de vigilance :

- On ne capture **que des paires** : ni une pierre seule, ni trois pierres ou plus alignées.
- Un seul coup peut capturer dans plusieurs directions simultanément. Traite les 8 directions.
- Les intersections libérées redeviennent jouables normalement, **sans aucune mémoire de la capture**.
  Supprime toute la machinerie `Capture` / `virtual_capturer` / `Captured` de l'existant : elle est la
  cause des 400 octets par cellule et elle n'est pas nécessaire.
- **« On ne peut pas se déplacer dans une capture. »** Cela signifie qu'une capture n'est évaluée que
  pour le joueur qui vient de jouer, et uniquement dans le sens « je prends en sandwich ». Poser
  volontairement une pierre entre deux pierres adverses (`O _ O` → `O X O`) n'est **jamais** une
  autocapture. Il n'existe donc aucun cas à traiter au-delà de la règle ci-dessus : c'est précisément
  l'absence de règle supplémentaire qui implémente ce point du sujet. Écris un test nommé
  `pas_d_autocapture_en_entrant_dans_un_sandwich` qui le prouve.

### 3.3 Victoire par capture

**10 pierres adverses capturées, soit 5 paires**, est une condition de victoire immédiate.

### 3.4 Fin de partie et captures

Quand le joueur `X` joue et forme un alignement de 5 ou plus :

1. Soit `L` l'ensemble des pierres appartenant aux alignements de 5+ de `X`.
2. Énumère tous les coups légaux de `O` (l'adversaire) qui capturent au moins une paire.
3. **Si `X` a déjà perdu 4 paires et que `O` dispose d'au moins un coup de capture** (quelle que soit
   la paire visée), alors `O` gagne immédiatement par capture. C'est la clause explicite du sujet.
4. **Sinon, s'il existe un coup de capture de `O` qui retire au moins une pierre de `L`** et qu'après
   ce retrait `X` n'a plus aucun alignement de 5+, alors la victoire de `X` est **en attente** : la
   partie continue, `O` doit encore jouer effectivement ce coup. Journalise clairement l'événement
   (« alignement cassable par capture, la partie continue »).
5. **Sinon, `X` gagne.**

Cette fonction ne doit **jamais** muter le plateau. Elle doit être un prédicat pur (ou travailler sur
une copie / faire make-unmake proprement). Le comportement actuel — appeler un prédicat à effets de
bord dans la condition d'un `while` — est à supprimer.

### 3.5 Interdiction du double-trois

Un coup est **illégal** s'il crée **deux trois libres ou plus** simultanément.

Définition du sujet : « un trois libre est un alignement de trois pierres qui, s'il n'est pas
immédiatement bloqué, permet d'obtenir un alignement de quatre indéfendable, c'est-à-dire un
alignement de quatre pierres dont les deux extrémités sont libres ».

Implémentation exigée :

- Les motifs canoniques de trois libre, dans une fenêtre de 6 cases (`_` vide, `X` joueur) :
  `_XXX__`, `__XXX_`, `_XX_X_`, `_X_XX_`. Un trois n'est libre que s'il peut effectivement devenir un
  **quatre libre** (`_XXXX_`), ce qui se lit directement dans `PATTERN_KIND` via
  `OpenThree` et `BrokenThree`. Une sentinelle de bord bloque comme une pierre adverse.
- Le comptage se fait **par axe** (4 axes maximum), jamais par direction (8). Un coup crée au plus
  4 trois libres.
- La condition d'interdiction est **`>= 2`**, pas `== 2`.
- Contre-exemple obligatoire à ne pas régresser : `X X _ X X` — jouer dans le trou fait cinq et gagne,
  ce coup doit être **légal**. L'implémentation actuelle l'interdit. Écris le test
  `xx_trou_xx_est_legal_car_fait_cinq`.
- **Exception du sujet** : « il n'est pas interdit d'introduire un double-trois en capturant une
  paire ». Donc : si le coup capture au moins une paire, il est légal sans autre vérification.
- Un coup qui fait **cinq ou plus** est toujours légal, quel que soit le nombre de trois libres créés.
- Cette règle s'applique aux **deux camps**, humain **et IA**. La génération de coups de l'IA doit
  filtrer les coups illégaux ; l'IA ne doit jamais proposer un coup interdit.
- Détermine les trois libres **sur la position après application du coup et de ses captures**.

### 3.6 Match nul

Il y a match nul quand le joueur au trait n'a plus aucun coup légal.

---

## 4. Interface, timer et débogage

Garde `ratatui`, le logo ASCII, le logger horodaté et la disposition en trois colonnes. Étends :

### 4.1 Écran de configuration

Accessible au lancement et via une touche (`[n]` par exemple). Permet de choisir :

- Mode : **Humain vs IA** (avec choix de la couleur de l'humain), **Humain vs Humain** (hotseat),
  ou une partie de démonstration si tu l'ajoutes.
- **Règle d'ouverture** : Standard / Pro / Long Pro / Swap / Swap2 (voir section 5).
- Budget de temps de l'IA (par défaut 380 ms soft / 480 ms hard).
- Nombre de threads.

Navigation clavier. Les raccourcis existants `[q]`, `[r]`, `[a]` doivent continuer de fonctionner.

### 4.2 Timer — exigence de validation

Affiche en permanence, dans le panneau de droite :

- **Temps du dernier coup de l'IA**, en millisecondes avec une décimale, mesuré avec
  `std::time::Instant` autour du seul appel de recherche.
- **Moyenne** sur tous les coups de l'IA de la partie — c'est la moyenne qui est notée par le sujet.
- **Maximum** observé.
- Un indicateur visuel qui passe au rouge si le dernier temps ou la moyenne dépasse 500 ms.

### 4.3 Panneau de débogage du raisonnement de l'IA

Le sujet le recommande fortement et il sera indispensable en soutenance. Affiche :

- Profondeur atteinte par la dernière itération terminée (elle doit être **≥ 10**).
- Nœuds visités, nœuds par seconde.
- Score de la position, avec son signe expliqué (positif = favorable à l'IA).
- **Variante principale** complète, en coordonnées lisibles (par exemple `K10 J11 L9 …`).
- Taux de succès de la table de transposition.
- Nombre de coups candidats retenus à la racine, et le score des 5 meilleurs.
- Un mode « détail du score » activable par une touche, qui décompose l'évaluation de la position
  courante par catégorie de motif et par joueur, ainsi que l'apport des captures.
- Une touche qui affiche une **carte de chaleur** des scores d'ordonnancement des coups candidats
  directement sur le goban. C'est extrêmement parlant en soutenance.

### 4.4 Suggestion de coup en hotseat

Une touche (`[s]`) lance le moteur pour le joueur au trait et **met en surbrillance** la case
suggérée sur le goban, sans la jouer. Le temps de calcul et la variante principale s'affichent dans
le panneau de débogage. La suggestion doit fonctionner pour les deux couleurs.

### 4.5 Retour visuel de jeu

- Surbrillance de la dernière pierre posée.
- Signalement visuel des pierres qui viennent d'être capturées, sur un tick ou deux.
- Message clair et journalisé quand un coup est refusé pour double-trois, avec les axes fautifs.
- Compteur de paires capturées par joueur, bien visible.
- Coordonnées lisibles autour du goban (lettres et chiffres), sinon la lecture de la PV est inutilisable.

---

## 5. Bonus retenu : règles d'ouverture

C'est le seul bonus demandé, mais il doit être **impeccable**, y compris côté IA.

- **Standard** : aucune restriction.
- **Pro** : le 1ᵉʳ coup de Noir doit être le centre exact. Le 2ᵉ coup de Noir doit être à au moins
  3 intersections du centre (donc hors du carré 5×5 centré). Blanc est libre.
- **Long Pro** : identique, mais le 2ᵉ coup de Noir doit être à au moins 4 intersections du centre
  (hors du carré 7×7 centré).
- **Swap** : le joueur 1 place 3 pierres (2 noires, 1 blanche) ; le joueur 2 choisit ensuite la
  couleur qu'il prend.
- **Swap2** : le joueur 1 place 3 pierres (2 noires, 1 blanche) ; le joueur 2 choisit alors entre
  (a) prendre Noir, (b) prendre Blanc, ou (c) placer 2 pierres de plus (1 noire, 1 blanche) et laisser
  le joueur 1 choisir sa couleur.

Exigences :

- Les contraintes de placement doivent être **appliquées et expliquées à l'écran** pendant la phase
  d'ouverture (cases interdites grisées, message indiquant ce qui est attendu).
- **L'IA doit savoir jouer ces ouvertures**, y compris **faire le choix de couleur** : quand c'est à
  elle de choisir, elle évalue la position avec son moteur et prend la couleur dont le score est le
  plus favorable. Journalise le score des deux options : c'est un excellent point de soutenance.
- Un test par règle, vérifiant qu'un placement interdit est refusé et qu'un placement valide est
  accepté.

---

## 6. Robustesse — « ne doit jamais crasher »

- **Aucun `unwrap()`, `expect()`, `panic!`, `unreachable!`, `assert!` ni indexation potentiellement
  hors-bornes** dans le code de jeu, de moteur ou d'interface. Les seules exceptions tolérées sont
  dans le code de test. Traite tous les cas d'erreur par `Result` ou par une valeur de repli
  documentée.
- Compile en release avec `overflow-checks = true` **et** assure-toi qu'aucun débordement n'est
  atteignable : utilise `saturating_*` / `checked_*` sur tous les compteurs (`pairs_captured`,
  historique, statistiques).
- **Hook de panique** installé dans `main` : il doit restaurer le terminal (`ratatui::restore()`,
  désactivation de la capture souris, sortie du mode brut) **avant** d'afficher le message, sinon une
  panique laisse le terminal du correcteur inutilisable. Même chose sur toute sortie d'erreur.
- Enveloppe l'appel à la recherche dans un filet de sécurité de dernier recours
  (`std::panic::catch_unwind`) qui, en cas d'imprévu, joue un coup légal de repli au lieu de tuer
  l'application.
- Toutes les allocations importantes (table de transposition) doivent utiliser `try_reserve` et
  dégrader proprement, comme spécifié en 2.6.
- Gère un redimensionnement de terminal, y compris un terminal plus petit que le goban : affiche un
  message « agrandissez le terminal » au lieu de paniquer ou de dessiner n'importe quoi.
- Clics hors du goban, clics sur une case occupée, clic pendant que l'IA réfléchit, touche pressée
  pendant une animation : rien ne doit casser l'état de la partie.

---

## 7. Livrables

### 7.1 `Makefile`

Conforme au sujet, **sans relink** :

```make
NAME    := Gomoku
SRCS    := $(shell find src -type f -name '*.rs') Cargo.toml Cargo.lock

all: $(NAME)

$(NAME): $(SRCS)
	cargo build --release
	cp target/release/gomoku $(NAME)

clean:
	cargo clean

fclean: clean
	rm -f $(NAME)

re: fclean all

test:
	cargo test --release

.PHONY: all clean fclean re test
```

Adapte si nécessaire, mais **vérifie effectivement** qu'un second `make` affiche `up to date` et ne
relance pas la compilation. Ajoute `/Gomoku` au `.gitignore`.

### 7.2 `Cargo.toml`

- Retire `num-bigint` (inutilisé).
- Ajoute un profil release optimisé :

```toml
[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "unwind"
overflow-checks = true
```

`overflow-checks = true` en release est un choix délibéré de sûreté : mesure son coût et, si l'impact
sur les nœuds/s est significatif, garde-le mais documente la mesure dans `DEFENSE.md`.

### 7.3 `DEFENSE.md` — document de soutenance, en français

C'est un livrable de **première importance** : le sujet est explicite, si le candidat ne sait pas
expliquer son minimax et son heuristique en détail, il n'a aucun point dessus. Écris ce document pour
qu'une personne qui ne connaît **rien** à minimax puisse suivre, et pour que le propriétaire du projet
puisse s'en servir pour réviser. Contenu attendu :

1. **Vue d'ensemble** : les modules, leurs responsabilités, le flux d'un coup de bout en bout.
2. **Représentation de la position** : pourquoi un plateau matelassé de 27×27, pourquoi les sentinelles
   suppriment les tests de bornes, pourquoi 729 octets change tout par rapport à 141 Ko, avec les
   chiffres de cache à l'appui.
3. **Minimax pas à pas** : d'abord le minimax naïf sur un mini-exemple d'arbre dessiné en ASCII, puis
   l'introduction d'alpha-bêta sur le même arbre en montrant exactement quelles branches sont coupées
   et pourquoi c'est sans perte, puis le passage à negamax (et pourquoi le signe s'inverse), puis PVS.
   Explique l'approfondissement itératif et pourquoi il **accélère** la recherche au lieu de la
   ralentir (l'ordonnancement issu de l'itération précédente). Explique la fenêtre d'aspiration, les
   drapeaux de la table de transposition, les coups de tueur et l'heuristique d'historique.
4. **Heuristique en détail** : la table de motifs, l'encodage en base 3, pourquoi une extrémité fermée
   annule la valeur d'un alignement, le barème final et le raisonnement derrière chaque poids, le
   traitement des captures et de la vulnérabilité aux captures, la convention de signe. Donne des
   exemples chiffrés sur 3 ou 4 positions concrètes.
5. **Évaluation incrémentale** : le calcul du delta, pourquoi c'est correct, le gain mesuré.
6. **Les règles** : chaque règle du sujet, la fonction qui l'implémente, le test qui la couvre. Traite
   explicitement les pièges : `XX_XX`, l'absence d'autocapture, le double-trois par capture,
   l'alignement cassable par capture, la victoire par capture à 4 paires perdues.
7. **Compromis assumés** : la limitation à K candidats, le plafond de quiescence, le remplacement dans
   la TT, l'élagage non exact — avec les mesures qui les justifient et l'aveu franc de ce qu'ils
   coûtent en exactitude théorique.
8. **Mesures de performance** : un tableau réel, obtenu en exécutant le benchmark, avec pour plusieurs
   positions représentatives (plateau vide, ouverture, milieu de partie simple, milieu de partie avec
   menaces multiples, position tactique complexe) : profondeur atteinte, nœuds, nœuds/s, temps, coup
   choisi. Plus une comparaison avant/après sur le coût de l'évaluation.
9. **Questions probables du correcteur, avec les réponses.** Au minimum : « pourquoi negamax et pas
   minimax ? », « alpha-bêta change-t-il le résultat ? », « que se passe-t-il si ton heuristique est
   fausse ? », « pourquoi ta table de transposition ne fausse-t-elle pas le score ? », « comment
   garantis-tu la profondeur 10 ? », « pourquoi limiter les candidats n'est-il pas de la triche ? »,
   « comment ton IA gère-t-elle les captures dans sa recherche ? », « comment détectes-tu un trois
   libre ? ».

### 7.4 `README.md`

Réécris-le : compilation, lancement, commandes clavier et souris, modes de jeu, règles implémentées,
options. Le contenu actuel est un copier-coller de code, à remplacer.

### 7.5 `TODO.md`

Mets-le à jour ou supprime-le, mais ne laisse pas un fichier de notes obsolète dans le dépôt.

---

## 8. Tests obligatoires

Tous en `cargo test`, moteur forcé en monothread pour le déterminisme. Nomme les tests de façon
parlante, en français si tu veux, mais explicitement.

**Règles :**
- Capture simple dans les 8 directions.
- Capture multiple par un seul coup.
- Pas de capture sur 1 pierre, ni sur 3 pierres alignées ou plus.
- `pas_d_autocapture_en_entrant_dans_un_sandwich`.
- Victoire à 5 paires capturées.
- Alignement de 5 puis de 6, sur les 4 axes, y compris au bord et dans les coins.
- `xx_trou_xx_est_legal_car_fait_cinq`.
- Double-trois interdit sur les 4 motifs canoniques ; trois fermé **non** comptabilisé ; triple-trois
  interdit ; double-trois **autorisé** quand le coup capture ; comptage par axe et non par direction.
- Fin de partie : alignement cassable par capture → partie qui continue ; alignement non cassable →
  victoire ; 4 paires perdues + capture disponible → victoire de l'adversaire par capture.
- Match nul.
- Les 5 règles d'ouverture : placement interdit refusé, placement valide accepté.

**Position et cohérence :**
- **Test de réversibilité** : 5 000 séquences aléatoires de coups légaux jouées puis intégralement
  défaites ; le plateau, le score incrémental, le Zobrist, les compteurs de paires et les compteurs de
  voisinage doivent revenir **exactement** à l'état initial.
- **Test de cohérence du score incrémental** : sur 5 000 positions aléatoires, le score incrémental
  doit être **strictement égal** à un recalcul complet depuis zéro.
- Cohérence du Zobrist : deux chemins de coups menant à la même position produisent la même clé.

**Évaluation :**
- Un trois libre vaut significativement plus qu'un trois fermé.
- Un quatre libre vaut plus qu'un trois libre ; un quatre libre est presque décisif.
- Une position symétrique vaut exactement 0.
- Inverser les couleurs inverse exactement le signe du score.
- La table `PATTERN_KIND` classe correctement une liste exhaustive de motifs de référence écrite à la
  main dans le test.

**Recherche :**
- Trouve le mat en 1 (compléter un cinq) sur plusieurs positions.
- Bloque le mat en 1 adverse.
- Trouve une victoire par capture quand elle est disponible.
- Évite de jouer un coup illégal (double-trois).
- Préfère une victoire en 1 à une victoire en 3 (test de la distance au mat).
- Ne renvoie jamais un coup illégal ni un index hors du plateau, sur 200 positions aléatoires.

**Performance (peut être `#[ignore]` par défaut si trop lourd en CI, mais doit être exécutable et
exécuté par toi) :**
- Sur au moins 5 positions représentatives, la profondeur de la dernière itération terminée est
  **≥ 10** et le temps est **< 500 ms**. Le test échoue sinon.
- Un benchmark qui produit le tableau chiffré destiné à `DEFENSE.md`.

Supprime les fonctions dupliquées `test_static_evaluation_*` : les tests doivent porter sur le code de
production, jamais sur une copie. C'est précisément ce qui a masqué le bug de l'évaluation verticale.
Aucun `println!` dans une fonction du chemin chaud.

---

## 9. Protocole de vérification — à exécuter réellement, pas à supposer

Avant de rendre ton rapport, exécute ces commandes et **rapporte leur sortie réelle** :

1. `cargo build --release` → **zéro warning**. Corrige les 11 warnings existants.
2. `cargo clippy --release --all-targets` s'il est disponible → traite les avertissements pertinents.
3. `cargo test --release` → tout passe.
4. Le test de performance, avec les chiffres : profondeur atteinte, temps moyen, temps max, nœuds/s,
   pour chaque position de référence.
5. `make` → produit `./Gomoku`.
6. `make` une seconde fois → **doit ne rien reconstruire**. Colle la sortie qui le prouve.
7. `make re` → reconstruit proprement.
8. `make fclean` → `./Gomoku` a disparu.
9. `./Gomoku --help` ou l'équivalent, si tu ajoutes des options en ligne de commande.

Si un critère n'est pas atteint — en particulier la profondeur 10 sous 500 ms — **ne le maquille pas**.
Itère sur l'ordonnancement, la valeur de K, le plafond de quiescence, la taille de la TT et la
quiescence, puis reporte les chiffres réels. Si après itération un objectif reste hors d'atteinte, dis-le
franchement, avec les mesures et les pistes restantes.

---

## 10. Style et méthode de travail

- **Commentaires et documentation en français.** Le propriétaire du projet doit pouvoir relire et
  s'approprier chaque ligne : ce code sera défendu comme le sien, il doit donc être **compréhensible
  avant d'être astucieux**. Commente le *pourquoi* des choix algorithmiques et des invariants, pas le
  *quoi* évident.
- Chaque fonction d'algorithme non trivial (negamax, PVS, TT, classification de motifs, évaluation
  incrémentale, double-trois, fin de partie par capture) doit être précédée d'un commentaire de
  quelques lignes expliquant son rôle, ses invariants et ses préconditions.
- Reste cohérent avec le ton du code existant : Rust idiomatique, sans abstraction gratuite, sans
  couche de généricité inutile. Pas de `trait` pour un seul implémenteur. Pas de macro maison.
- Pas d'emoji dans le code source. Les emoji du logger existant (`🎉`) peuvent rester dans les messages
  d'interface.
- **Commits** : travaille sur la branche `pasprime` et découpe en commits logiques et lisibles
  (représentation de la position, tables de motifs, règles, évaluation, recherche, TT, parallélisme,
  interface, ouvertures, tests, documentation). Messages en français, à l'impératif. Cela permettra
  une relecture par étapes.
- **Ne laisse aucun code mort.** L'ancien `src/board/ai/` et l'ancienne logique de captures doivent
  disparaître, pas cohabiter avec la nouvelle. Supprime `PROMPT_SONNET.md` de la racine à la fin, ou
  ajoute-le à `.gitignore`.

---

## 11. Ordre de travail recommandé

Avance dans cet ordre et **valide chaque étape par ses tests avant de passer à la suivante**. Ne
construis pas la recherche sur une position dont la réversibilité n'est pas prouvée.

1. `game/position.rs` : plateau matelassé, `make_move` / `unmake_move`, Zobrist, compteurs de
   voisinage. → test de réversibilité vert.
2. `game/patterns.rs` : les deux tables et la classification. → tests exhaustifs de motifs verts.
3. Évaluation incrémentale branchée sur `make/unmake`. → test de cohérence incrémentale vs complète vert.
4. `game/rules.rs` : captures, double-trois, alignement, fin de partie par capture, nul. → tous les
   tests de règles verts.
5. `ai/eval.rs` : barème complet, captures, vulnérabilité, convention de signe. → tests d'évaluation verts.
6. `ai/movegen.rs` : génération, filtrage de légalité, ordonnancement. → tests de légalité verts.
7. `ai/search.rs` : negamax + alpha-bêta d'abord, **puis seulement** TT, itératif, aspiration, PVS,
   quiescence, un élément à la fois, en mesurant le gain de chacun. Note ces mesures : elles nourrissent
   `DEFENSE.md`.
8. `ai/tt.rs` puis le parallélisme. Vérifie que le monothread et le multithread choisissent le même
   coup sur des positions non ambiguës.
9. Interface : goban, coordonnées, timer, panneau de débogage, heatmap, suggestion, menu.
10. `game/openings.rs` et l'intégration IA du choix de couleur.
11. Makefile, `Cargo.toml`, `.gitignore`, `README.md`, `DEFENSE.md`.
12. Passe finale : zéro warning, chasse aux `unwrap`, hook de panique, protocole de vérification complet.

---

## 12. Critères d'acceptation — la checklist finale

Ne conclus pas avant d'avoir coché chaque ligne, et **rapporte-la explicitement** :

- [ ] `make` produit `./Gomoku`, un second `make` ne relink pas, `clean` / `fclean` / `re` fonctionnent.
- [ ] `cargo build --release` : zéro warning.
- [ ] `cargo test --release` : tout vert, y compris réversibilité, cohérence incrémentale, règles,
      évaluation et recherche.
- [ ] Profondeur de la dernière itération terminée **≥ 10** sur les 5 positions de référence, temps
      **< 500 ms** sur chacune, moyenne rapportée.
- [ ] Timer du dernier coup, moyenne et maximum affichés en permanence.
- [ ] Panneau de débogage avec profondeur, nœuds, nœuds/s, score, variante principale, taux de TT, et
      heatmap activable.
- [ ] Humain vs IA avec choix de couleur ; hotseat avec suggestion de coup sur `[s]` pour les deux camps.
- [ ] Les 6 points de règles de la section 3 implémentés et testés, y compris `XX_XX` légal, absence
      d'autocapture, double-trois par capture autorisé, alignement cassable par capture, victoire par
      capture à 4 paires perdues.
- [ ] L'IA ne joue jamais un coup illégal.
- [ ] Les 5 règles d'ouverture jouables, contraintes affichées, et IA capable de choisir sa couleur.
- [ ] Aucun `unwrap` / `expect` / `panic!` / indexation non sûre hors des tests ; hook de panique qui
      restaure le terminal ; TT qui dégrade proprement en cas d'échec d'allocation ; terminal trop
      petit géré.
- [ ] `DEFENSE.md` complet, en français, avec l'arbre d'exemple d'alpha-bêta, le barème justifié, les
      compromis assumés, le tableau de mesures réelles et la section questions/réponses.
- [ ] `README.md` réécrit, `TODO.md` traité, `num-bigint` retiré, aucun code mort, aucune fonction
      dupliquée entre tests et production.
- [ ] Commits logiques sur `pasprime`.

---

## 13. Rapport final attendu

Termine par un rapport structuré contenant :

1. **Le tableau de performance réel** : par position de référence, profondeur atteinte, nœuds, nœuds/s,
   temps, coup choisi. Plus la comparaison avant/après du coût de l'évaluation (le point de départ
   mesuré est 1,3 µs par appel).
2. **Le gain apporté par chaque optimisation**, mesuré isolément : alpha-bêta seul, + ordonnancement,
   + TT, + itératif, + PVS, + quiescence, + threads. C'est le tableau qui impressionne en soutenance.
3. Les **écarts éventuels par rapport à cette spécification**, avec la raison.
4. Ce qui **reste fragile ou perfectible**, honnêtement.
5. Les **3 à 5 points que le propriétaire doit absolument maîtriser** avant sa soutenance, avec le
   renvoi vers la section correspondante de `DEFENSE.md`.
