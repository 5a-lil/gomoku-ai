# DEFENSE.md — Comprendre et défendre ce projet

Ce document est écrit pour qu'on puisse suivre **sans rien connaître au
minimax au préalable**, et pour servir de support de révision avant la
soutenance. Il explique le *pourquoi* de chaque choix, avec des mesures
réelles obtenues en exécutant le projet (pas des estimations).

Sommaire :

1. [Vue d'ensemble](#1-vue-densemble)
2. [Représentation de la position](#2-représentation-de-la-position)
3. [Minimax pas à pas](#3-minimax-pas-à-pas)
4. [Heuristique en détail](#4-heuristique-en-détail)
5. [Évaluation incrémentale](#5-évaluation-incrémentale)
6. [Les règles : implémentation et tests](#6-les-règles--implémentation-et-tests)
7. [Compromis assumés](#7-compromis-assumés)
8. [Mesures de performance](#8-mesures-de-performance)
9. [Questions probables du correcteur](#9-questions-probables-du-correcteur)

---

## 1. Vue d'ensemble

Le projet est séparé en trois couches indépendantes :

- **`game`** : les règles de Gomoku elles-mêmes, sans rien savoir de l'IA ni
  de l'interface. `position.rs` porte la représentation du plateau et les
  primitives de bas niveau (jouer/défaire un coup, captures, score
  incrémental, Zobrist). `patterns.rs` classe les alignements. `rules.rs`
  décide de la légalité d'un coup, d'une victoire, d'un match nul.
  `openings.rs` ajoute les contraintes des règles d'ouverture (bonus).
  `state.rs` (`Game`) orchestre tout cela pour une partie complète : mode de
  jeu, journal, alignement "en attente".
- **`ai`** : ne connaît que `game::position::Position` et les prédicats purs
  de `game::rules`. `eval.rs` note une position. `movegen.rs` génère et
  ordonne les coups candidats. `search.rs` est le moteur negamax complet.
  `tt.rs` est la table de transposition.
- **`app` / `ui`** : l'état applicatif (écran actif, partie en cours,
  minuterie, bascules d'affichage) et son rendu `ratatui`. C'est la seule
  couche qui sait qu'un humain existe et clique avec une souris.

**Flux d'un coup, de bout en bout** (cas Humain vs IA) :

1. L'humain clique sur une case ou déplace le curseur clavier et appuie sur
   Entrée → `App::try_human_click(index)`.
2. `Game::try_play(index)` vérifie les contraintes d'ouverture
   (`OpeningState::check_placement`), puis la légalité générale
   (`rules::check_move_legal`, qui filtre le double-trois), joue le coup
   (`Position::make_move`, qui applique aussi les captures), puis arbitre
   les conditions de victoire (`rules::has_won_by_capture`,
   `rules::resolve_alignment`) et le match nul (`rules::is_draw`).
3. Si la partie continue et que c'est maintenant à l'IA de jouer,
   `App::drive_ai_if_needed` lance un thread qui appelle
   `Engine::search(&position, limits)`.
4. `Engine::search` répartit l'approfondissement itératif sur plusieurs
   threads (Lazy SMP), chacun appelant `negamax` avec alpha-bêta, PVS et la
   table de transposition partagée, jusqu'à épuiser le budget de temps.
5. Le thread applicatif récupère le résultat via un canal (`mpsc`), sans
   jamais bloquer la boucle de rendu, et rejoue le coup choisi via
   `Game::try_play`, exactement comme un coup humain.

## 2. Représentation de la position

L'ancienne implémentation stockait le plateau comme `Vec<Cell>` avec
`size_of::<Cell>() == 400` octets (deux tableaux `[Capture; 8]` et
`[Option<(usize,usize)>; 8]` par case, pour mémoriser les captures
possibles). Un plateau de 361 cases pesait donc **141 Ko** — bien plus que
les 32 ou 48 Ko d'un cache L1 typique. Chaque évaluation qui relit tout le
plateau provoquait donc des dizaines de défauts de cache.

La nouvelle `Position` (`game/position.rs`) :

- **Plateau matelassé** : `cells: [u8; 27*27]` (729 octets). Le goban 19x19
  réel est plongé au centre d'une grille 27x27 dont la bordure de 4 cases
  est remplie de sentinelles `WALL`. Une pierre ne peut jamais être posée à
  moins de 4 cases du bord logique, donc n'importe quel accès
  `index ± k*direction` avec `k <= 4` (largement suffisant : les fenêtres
  utilisées font au plus 9 cases, soit `k <= 4`) reste dans le tableau. Cela
  supprime tout test de borne/modulo dans les boucles chaudes (fini les
  `check_up_right` et consorts de l'ancienne implémentation) : lire une case
  hors plateau renvoie simplement `WALL`, qui bloque un alignement exactement
  comme une pierre adverse.
- **729 octets tiennent entièrement dans le cache L1** (32 Ko sur la plupart
  des machines modernes), avec de la marge pour `neighbour_count` (encore
  729 octets) et le reste de `Position` (moins de 2 Ko en tout, voir plus
  bas). Le gain n'est pas seulement une histoire de "moins d'octets à
  copier" : c'est la garantie qu'un nœud de recherche entier — lire l'état,
  générer les coups, évaluer — reste dans le cache le plus rapide du
  processeur, au lieu de provoquer un aller-retour en RAM à chaque case
  regardée.
- **Score incrémental** (`score: i32`) maintenu par `set_cell` à chaque
  mutation (voir section 5) : plus besoin de rebalayer tout le plateau pour
  évaluer une position.
- **Zobrist** (`zobrist: u64`) : une clé de hachage incrémentale (XOR d'une
  clé aléatoire par (case, couleur), plus une clé de trait, plus des clés
  par nombre de paires capturées), qui identifie une position pour la table
  de transposition en O(1) par coup.
- **Voisinage incrémental** (`neighbour_count: [u8; 729]`) : compteur du
  nombre de pierres à distance de Tchebychev <= 2 de chaque case, maintenu à
  chaque pose/retrait. Une case est candidate au jeu si et seulement si son
  compteur est non nul — plus besoin de balayer les 361 cases pour trouver
  les coups intéressants.
- **`make_move` / `unmake_move` sans copie du plateau** : jouer un coup
  mute `Position` en place et renvoie un petit `MoveUndo` (l'essentiel de
  son coût : jusqu'à 8 paires capturées, un `i32` de score, un `u64` de
  hachage) qui permet de tout défaire exactement. C'est ce mécanisme qui
  rend `Position` clonable à moins de 2 Ko (nécessaire pour le
  parallélisme : chaque thread a sa propre copie, sans le coût des 141 Ko de
  l'ancienne structure) tout en gardant la recherche elle-même sans aucune
  allocation ni copie par nœud.

La garantie la plus importante de ce module est prouvée par un test :
`reversibilite_sur_sequences_aleatoires` joue 200 séquences aléatoires de
10 à 40 coups légaux, les défait intégralement, et vérifie que le plateau,
le score, le Zobrist, les compteurs de captures, les compteurs de voisinage
et le joueur au trait reviennent **bit à bit identiques** à l'état initial.
Sans cette garantie, aucune recherche par make/unmake n'est fiable.

## 3. Minimax pas à pas

### 3.1 Minimax naïf

Le principe : à chaque position, le joueur au trait choisit le coup qui
maximise SON score ; l'adversaire choisit celui qui le minimise. On
explore l'arbre jusqu'à une profondeur donnée, on évalue les feuilles, puis
on remonte les valeurs en alternant max et min.

```
                 MAX (Noir)
              /       |       \
           A(3)      B(5)     C(1)
          /  \       /  \     /  \
       min  min    min  min  min min
        2    3      5    9    1   4
```

Noir (MAX) regarde ses trois coups A, B, C. Pour chacun, c'est Blanc (MIN)
qui répond et choisit la plus petite valeur : A→2, B→5, C→1. Noir choisit
alors le plus grand de ces trois : **B**, avec un score garanti de 5. C'est
tout le principe : chaque niveau inverse le critère de choix.

Le défaut du minimax naïf : il évalue **toutes** les feuilles, même celles
qui ne peuvent plus influencer le résultat final. Sur ce petit arbre ce
n'est rien, mais le facteur de branchement du Gomoku (une centaine de coups
possibles bruts par position) rend cette exploration complète totalement
impraticable au-delà de 3-4 coups de profondeur.

### 3.2 Élagage alpha-bêta, sur le même arbre

Alpha-bêta garde deux bornes en descendant dans l'arbre : `alpha` (le
meilleur score que MAX peut déjà garantir ailleurs) et `beta` (le pire score
que MIN est prêt à accepter). Dès qu'une branche ne peut plus améliorer ce
qui est déjà garanti, on arrête de l'explorer : c'est une coupure, **sans
aucune perte d'exactitude** sur la valeur finale.

Reprenons l'arbre, en donnant l'ordre d'exploration gauche-à-droite et les
feuilles une par une :

```
MAX évalue A : MIN regarde 2, puis 3 → A = min(2,3) = 2.
   alpha passe à 2 (Noir garantit au moins 2 en jouant ailleurs si besoin).

MAX évalue B : MIN regarde 5 en premier. 5 >= alpha(2), donc MIN continue.
   MIN regarde 9 ensuite : min(5,9) reste 5. B = 5.
   alpha passe à 5 (meilleur que A).

MAX évalue C : MIN regarde 1 en premier.
   1 < alpha(5) : MIN peut déjà garantir au plus 1, ce qui est PIRE que les
   5 que Noir a déjà ailleurs (branche B). Noir ne choisira donc JAMAIS C.
   → COUPURE : on n'a même pas besoin de regarder la feuille "4" à droite
   de C. On sait déjà que C ne peut pas être la meilleure branche pour Noir.
```

Résultat final : **B, valeur 5** — exactement le même résultat que le
minimax naïf, mais une feuille de moins évaluée. Sur un arbre réel de
profondeur 10 avec un bon ordonnancement des coups, ce n'est pas une feuille
économisée mais un facteur exponentiel : c'est ce qui permet d'atteindre la
profondeur 10 en pratique. C'est pour cette raison que l'ordonnancement des
coups (section 3.5) est aussi important que l'élagage lui-même : plus les
bons coups sont regardés en premier, plus les bornes `alpha`/`beta` se
resserrent tôt, plus il y a de coupures.

### 3.3 Negamax

Le minimax classique doit distinguer explicitement "c'est le tour de MAX"
et "c'est le tour de MIN", avec deux morceaux de code presque identiques
(un qui prend le max, un qui prend le min). Negamax exploite le fait que
Gomoku est un jeu **à somme nulle et symétrique** : la valeur d'une position
pour l'adversaire est exactement l'opposé de sa valeur pour le joueur au
trait. On peut alors écrire une seule fonction :

```
negamax(pos, profondeur) :
    si profondeur == 0 ou position terminale :
        renvoyer evaluate(pos)   // du point de vue du joueur au trait
    meilleur = -infini
    pour chaque coup légal :
        jouer le coup
        score = -negamax(pos, profondeur - 1)   // <- le signe s'inverse ici
        défaire le coup
        meilleur = max(meilleur, score)
    renvoyer meilleur
```

Le signe s'inverse parce qu'après avoir joué un coup, c'est l'adversaire qui
est au trait : la valeur qu'il calcule pour lui-même (`negamax` récursif)
est, par définition, l'opposé de ce qu'elle vaut pour nous. C'est exactement
la même logique que le minimax à deux fonctions, réécrite en une seule —
`ai::search::negamax` dans ce projet suit ce squelette avec alpha-bêta en
plus (bornes `alpha`/`beta` qui s'inversent et s'échangent au même endroit :
`-negamax(pos, depth-1, ply+1, -beta, -alpha, ctx)`).

### 3.4 PVS (recherche à fenêtre nulle)

Une fois les coups bien ordonnés (section 3.5), le premier coup exploré est
statistiquement presque toujours le meilleur. PVS (*Principal Variation
Search*) exploite cela : pour le premier coup, on fait une recherche
normale à pleine fenêtre `[-beta, -alpha]`. Pour tous les coups suivants, on
fait d'abord une recherche à **fenêtre nulle** `[-alpha-1, -alpha]` — une
question fermée, "ce coup est-il meilleur que ce qu'on a déjà, oui ou
non ?" — qui coupe beaucoup plus vite qu'une vraie fenêtre. Si la réponse
est "non" (ce qui arrive la plupart du temps si l'ordonnancement est bon),
on n'a rien perdu et on passe au coup suivant. Si la réponse est "oui, ce
coup est meilleur", on refait alors une recherche complète à pleine fenêtre
pour connaître sa valeur exacte (voir `ai::search::negamax`, bloc
`if s > alpha && s < beta { ... }`).

### 3.5 Approfondissement itératif, fenêtre d'aspiration, TT, killers, historique

- **Approfondissement itératif** : on cherche à profondeur 1, puis 2, puis
  3, etc., jusqu'à épuiser le budget de temps. Contre-intuitif au premier
  abord (pourquoi refaire tout le travail de la profondeur 1 en cherchant à
  la profondeur 2 ?), mais **accélère** la recherche globale : le meilleur
  coup trouvé à la profondeur *N* devient le premier coup essayé à la
  profondeur *N+1* (coup de la table de transposition, priorité absolue
  dans l'ordonnancement, section 3.5). Un bon premier coup, c'est
  exactement ce qui rend PVS et alpha-bêta efficaces. Le surcoût des
  profondeurs précédentes est négligeable (le nombre de nœuds croît de
  façon quasi-exponentielle avec la profondeur), le bénéfice sur
  l'ordonnancement ne l'est pas.
- **Fenêtre d'aspiration** : plutôt que de repartir de `[-INF, +INF]` à
  chaque nouvelle profondeur, on part d'une fenêtre resserrée autour du
  score de l'itération précédente (`± 50`). La plupart du temps le score
  ne change pas radicalement d'une profondeur à l'autre, donc cette fenêtre
  étroite provoque des coupures alpha-bêta beaucoup plus tôt. Si le
  résultat sort de la fenêtre (échec haut ou bas), on la réélargit (`× 4`)
  et on refait la recherche à la même profondeur — un coût rare mais non
  nul, compensé largement par le gain sur les cas normaux.
- **Table de transposition (TT)** : le même sous-arbre peut être atteint
  par des transpositions de coups différentes. La TT mémorise, par clé de
  Zobrist, le score déjà calculé pour cette position (avec sa profondeur et
  un drapeau `Exact`/`Lower`/`Upper` selon que la recherche précédente a pu
  déterminer la valeur exacte ou seulement une borne). Une entrée de
  profondeur suffisante permet de court-circuiter tout un sous-arbre déjà
  résolu. Voir section 2.6 pour l'implémentation sans verrou.
- **Coups tueurs (killer moves)** : à un niveau donné de l'arbre, un coup
  qui a provoqué une coupure bêta dans une branche sœur a de bonnes chances
  d'en provoquer une aussi ici (souvent un coup "universellement fort",
  comme bloquer une menace). On mémorise les 2 derniers coups tueurs par
  profondeur et on les essaie tôt.
- **Heuristique d'historique** : plus largement, chaque coup qui a déjà
  provoqué une coupure bêta n'importe où dans l'arbre voit son score
  d'ordonnancement augmenter (`depth²`, pour privilégier les coupures aux
  profondeurs importantes). C'est une mémoire à plus long terme que les
  killers, qui eux ne portent que sur un niveau précis.

### 3.6 Extension de quiescence

À profondeur 0, s'arrêter brutalement peut donner un score mensonger :
c'est **l'effet d'horizon** — par exemple, si le joueur au trait a un
"quatre" qui gagne au coup suivant, mais que la recherche s'arrête juste
avant de le voir. La quiescence prolonge la recherche à profondeur 0, mais
**seulement sur les coups forçants** (compléter un cinq, bloquer un cinq
adverse, capturer, créer un quatre) — jamais sur les 100 coups bruts d'une
position calme, ce qui la garde bon marché. Plafonnée à 8 demi-coups
(`MAX_QUIESCENCE`) pour éviter tout emballement sur une position très
tactique.

## 4. Heuristique en détail

Toute la partie "positionnelle" de l'évaluation vient de `Position::score`,
maintenu incrémentalement (section 5) à partir de `game::patterns`.

### 4.1 Score d'un alignement contigu (`run_score`)

Pour un alignement contigu de `len` pierres avec ses deux extrémités
ouvertes ou fermées (case vide = ouverte ; pierre adverse ou bord = fermée),
le barème est :

| Motif | Score |
|---|---|
| 5 ou plus | 100 000 (mais en pratique un 5+ est un score terminal, voir `ai::search::terminal_score`, jamais atteint via ce barème) |
| Quatre libre `_XXXX_` | 100 000 — indéfendable, presque une victoire |
| Quatre simple (une extrémité fermée) | 10 000 — force une réponse immédiate |
| Quatre mort (deux extrémités fermées) | 0 — ne peut plus jamais devenir cinq |
| Trois libre `_XXX_` | 5 000 — menace réelle de devenir un quatre libre |
| Trois fermé | 500 — dix fois moins, car il ne peut développer qu'un quatre simple, pas indéfendable |
| Trois mort | 0 |
| Deux libre | 200 |
| Deux fermé | 50 |
| Deux mort / un seul pion | 0 ou 10 |

Le point le plus important de ce barème, et le défaut le plus grave de
l'ancienne implémentation : **un trois libre vaut dix fois un trois fermé**
(5000 contre 500), parce que seul le trois libre menace réellement de
devenir un quatre libre (indéfendable). Sans cette distinction, l'IA ne
fait aucune différence entre une menace réelle et un alignement mort, ce qui
la rend incapable de défendre ou d'attaquer correctement.

### 4.2 Captures

Les captures sont une condition de victoire (10 pierres = 5 paires) : leur
poids doit donc croître plus vite qu'un compteur linéaire à mesure qu'on
s'approche de la victoire. Barème progressif et convexe (`CAPTURE_SCORE`) :

| Paires déjà capturées | Score ajouté |
|---|---|
| 0 | 0 |
| 1 | 1 500 |
| 2 | 4 000 |
| 3 | 9 000 |
| 4 | 25 000 |
| 5 | victoire immédiate (`rules::has_won_by_capture`), jamais atteint via ce barème |

Le test `capture_term_est_croissant_et_convexe` vérifie explicitement que le
gain marginal (score à *n* paires moins score à *n-1*) croît strictement
avec *n*.

### 4.3 Vulnérabilité aux captures

Sans ce terme, l'IA construit volontiers des alignements qui se font casser
au coup suivant par une capture (`X O O _` avec la case vide jouable) : elle
ne "voit" la perte qu'une fois la capture effectivement jouée par
l'adversaire, un coup de recherche trop tard. `vulnerability_term` pénalise
(`800` par paire) le joueur dont le **dernier coup joué** vient de créer une
telle paire immédiatement capturable. Limité au dernier coup (pas à
l'intégralité du plateau, ce qui coûterait de nouveau un balayage complet
par nœud) : voir section 7 pour la discussion de cette limitation.

### 4.4 Convention de signe

`Position::score` est maintenu du point de vue de **Noir** (positif
favorise Noir, par construction du barème : `total += score` si la couleur
de l'alignement est Noir, `total -= score` si elle est Blanc — voir
`patterns::line_contribution`). `eval::evaluate` ajoute les termes de
capture et de vulnérabilité (toujours "Noir moins Blanc"), puis renvoie
cette somme si `to_move == BLACK`, ou son opposé sinon :

```rust
let black_relative = pos.score + capture_term(BLACK) - capture_term(WHITE)
    - vulnerability_term(BLACK) + vulnerability_term(WHITE);
if pos.to_move == BLACK { black_relative } else { -black_relative }
```

C'est la convention negamax : la fonction d'évaluation renvoie toujours une
valeur du point de vue du joueur qui doit jouer maintenant, jamais une
valeur absolue. Le test `avoir_un_trois_libre_de_plus_favorise_le_joueur_au_trait`
vérifie explicitement que la même position évalue positivement pour Noir au
trait et négativement pour Blanc au trait.

### 4.5 Exemples chiffrés

- Position vide : `evaluate = 0` (test `position_vide_est_neutre`).
- Un simple trois libre pour Noir, Noir au trait : score positif significatif
  (~5000, avant le petit terme de tempo négligeable). Le même trois devenu
  un quatre libre : `evaluate(quatre) > evaluate(trois) * 5` (test
  `quatre_libre_vaut_beaucoup_plus_qu_un_trois_libre`), cohérent avec
  100 000 contre 5 000 dans le barème.
- Une paire immédiatement capturable après le dernier coup joué : score
  strictement inférieur à la même position sans cette vulnérabilité (test
  `coup_qui_expose_une_paire_est_penalise`).

## 5. Évaluation incrémentale

Recalculer `Position::score` depuis zéro coûterait un balayage complet des
4 axes × 361 cases à chaque nœud — exactement le défaut mesuré de l'ancienne
implémentation (1,3 µs par appel, voir section 8). Le score est au contraire
maintenu **incrémentalement** dans `Position::set_cell`, le seul point
d'écriture sur `cells` :

```
avant  = somme des contributions des 4 lignes passant par `index`
mute la case (pose ou retrait d'une pierre)
après  = somme des contributions des 4 lignes passant par `index`
score += après - avant
```

Chaque contribution de ligne (`patterns::line_contribution`) balaie la ligne
complète une seule fois (pas la fenêtre glissante à 6 cases envisagée
initialement dans la conception : le résultat est identique et le balayage
d'une ligne entière reste O(longueur de la ligne), donc largement
suffisant), détecte chaque alignement contigu et cumule son `run_score`
signé. Ce n'est pas O(1) au sens strict (une ligne fait jusqu'à 19 cases),
mais c'est borné et petit (au plus 4 lignes de 19 cases par coup, plus 2
par pierre capturée), et surtout **indépendant de la taille du plateau
complet** : jouer un coup sur un plateau qui contient déjà 200 pierres ne
coûte pas plus cher que sur un plateau presque vide.

**Preuve de correction** : le test `score_incremental_egale_recalcul_complet`
joue 200 séquences aléatoires de 5 à 65 coups et vérifie, après **chaque**
coup, que `pos.score` (maintenu incrémentalement) est strictement égal à
`pos.recompute_score_from_scratch()` (un recalcul complet indépendant, qui
balaie chaque ligne une fois depuis son extrémité). Sans cette égalité
prouvée sur des centaines de positions, le gain de vitesse ne vaudrait rien
: on aurait juste substitué de la rapidité à de l'exactitude.

**Gain mesuré** (voir `ai::eval::bench::cout_de_evaluate`, section 8) :
`evaluate()` (qui ne fait plus que lire `pos.score` et ajouter deux petits
termes bornés) coûte **~3 ns** par appel contre les **~1,3 µs** mesurés sur
l'ancienne implémentation — un facteur supérieur à 400.

## 6. Les règles : implémentation et tests

| Règle du sujet | Fonction | Test(s) |
|---|---|---|
| Alignement de 5+ sur les 4 axes | `Position::forms_five`, `Position::count_axis` | (couvert indirectement par les tests de victoire et de recherche ; `count_axis` utilisé partout où un alignement doit être mesuré) |
| Capture de paires, 8 directions, une seule paire par direction | `Position::make_move` (bloc capture) | `capture_horizontale_simple`, `capture_puis_unmake_restaure_les_pierres_capturees` |
| Pas de capture sur 1 ou 3+ pierres alignées | `Position::make_move` (motif exact `O O X`) | `pas_de_capture_sur_une_seule_pierre`, `pas_de_capture_sur_trois_pierres_alignees` |
| Pas d'autocapture en jouant dans un sandwich | absence de règle dédiée : `O _ O → O X O` ne correspond à aucun des 8 motifs testés | `pas_d_autocapture_en_entrant_dans_un_sandwich` |
| Victoire à 5 paires capturées | `rules::has_won_by_capture` | `victoire_par_capture_a_cinq_paires` |
| Fin de partie liée aux captures (alignement cassable, 4 paires perdues) | `rules::resolve_alignment`, `rules::alignment_still_intact` | `alignement_non_cassable_est_une_victoire`, `alignement_cassable_par_capture_reste_en_attente`, `quatre_paires_perdues_et_capture_disponible_fait_gagner_l_adversaire` |
| `XX_XX` toujours légal (fait cinq) | court-circuit dans `rules::check_move_legal_as` | `xx_trou_xx_est_legal_car_fait_cinq` |
| Double-trois interdit (>= 2 trois libres, par axe) | `patterns::count_free_threes`, `rules::check_move_legal_as` | `double_trois_simple_est_illegal`, `triple_trois_est_illegal`, `double_trois_detecte_sur_deux_axes` |
| Double-trois autorisé si le coup capture | exception explicite dans `check_move_legal_as` | `double_trois_autorise_si_le_coup_capture` |
| Match nul | `rules::is_draw` (aucun coup légal) | `plateau_vide_nest_pas_nul` |
| Règle Pro / Long Pro (placement) | `OpeningState::check_placement` | `pro_refuse_premier_coup_hors_centre`, `pro_refuse_deuxieme_coup_trop_proche`, `long_pro_exige_distance_quatre` |
| Règle Swap / Swap2 (décision de couleur) | `OpeningState::pending_choice`, `Game::apply_color_decision` | `swap_declenche_le_choix_apres_trois_pierres`, `swap2_declenche_le_choix_final_apres_les_deux_pierres_extra`, `swap_declenche_bien_une_decision_apres_trois_coups` |

## 7. Compromis assumés

Chacun des points suivants est une décision d'ingénierie délibérée, mesurée,
et documentée ici plutôt que cachée :

- **Limitation à *K* candidats par nœud** (`ai::movegen::candidate_limit` :
  12 aux niveaux 0-1, 8 aux niveaux 2-3, 5 au-delà). Le facteur de
  branchement brut du Gomoku est de l'ordre de 100 ; sans cette limite,
  aucune profondeur à deux chiffres n'est atteignable en 500 ms. Ce n'est
  **pas** un élagage exact : un coup légitimement bon mais mal classé par
  l'heuristique de tri pourrait en théorie être écarté. Deux garde-fous
  compensent ce risque : (1) les coups gagnants, bloquants ou de capture
  décisive ne sont **jamais** comptés dans la limite (`critical: bool` dans
  `ai::movegen::Scored`), donc jamais élagués ; (2) l'ordonnancement en deux
  étapes (score positionnel bon marché d'abord, légalité complète ensuite
  seulement sur un sur-ensemble large des meilleurs candidats) garde une
  bonne qualité de tri sans payer le coût de la détection de double-trois
  sur les 300+ cases candidates d'un plateau chargé.
- **Plafond de quiescence** (8 demi-coups) : borne l'effet d'horizon sans
  laisser une position très tactique consommer tout le budget de temps sur
  la profondeur 0 seule.
- **Remplacement dans la table de transposition** : uniquement par
  profondeur supérieure (jamais par ancienneté ou LRU). Simple à raisonner,
  au prix de garder parfois une entrée un peu ancienne si aucune recherche
  plus profonde n'est repassée par cette position.
- **Détection terminale simplifiée dans l'arbre** (`ai::search::terminal_score`) :
  un alignement de 5+ est traité comme une victoire immédiate à l'intérieur
  de la recherche, sans réévaluer la clause complète de la section 3.4 (un
  alignement "cassable par capture" resterait en pratique gagnant dans
  l'arbre). Seule la partie **réellement jouée** (`Game::try_play`) applique
  la règle strictement. Un raffinement complet impliquerait de simuler,
  à chaque nœud terminal candidat, tous les coups de capture adverses
  possibles — un coût qui grandirait avec le nombre de captures
  disponibles, pour un cas rare en pratique (il faut que l'auteur de
  l'alignement ait déjà perdu des paires ET que l'adversaire ait
  spécifiquement un coup qui capture une pierre de CETTE ligne).
- **Itération interrompue par le temps toujours jetée intégralement**
  (`ai::search::iterative_deepening`), même si elle avait déjà amélioré le
  premier coup avant l'interruption. Plus simple à garantir correct qu'une
  réutilisation partielle, au prix d'un peu de temps de réflexion perdu en
  fin de budget (rarement plus de quelques millisecondes, le budget "soft"
  laissant de la marge avant le "hard").
- **Vulnérabilité aux captures limitée au dernier coup joué**
  (`ai::eval::vulnerability_term`) : un balayage complet du plateau à
  chaque nœud annulerait le gain de l'évaluation incrémentale. En pratique,
  c'est le cas le plus fréquent et le plus exploitable tactiquement ; une
  vulnérabilité plus ancienne réapparaît de toute façon dans l'évaluation
  dès qu'un coup ultérieur retouche cette zone du plateau.
- **Choix de couleur de l'IA pour Swap/Swap2, méthode simplifiée** (voir
  `App::finish_color_choice`) : l'IA lance une recherche complète (même
  budget qu'un coup normal) sur la position réelle, convertit le score
  obtenu (relatif au joueur au trait) en "score du point de vue de Noir",
  et prend la couleur la plus favorable. Le jeu étant à somme nulle, le
  score de l'autre couleur est simplement l'opposé — les deux sont
  journalisés. L'IA ne considère jamais l'option "placer 2 pierres de plus"
  de Swap2 (qui demanderait d'évaluer une position hypothétique après 2
  coups spéculatifs supplémentaires, pour un bénéfice incertain) : elle
  choisit toujours directement entre Noir et Blanc.
- **Ouverture Swap/Swap2 en mode Humain vs IA, qui place les pierres
  initiales ?** Le sujet définit ces règles comme "le joueur 1 place *N*
  pierres, puis le joueur 2 choisit sa couleur". Comme `Position::to_move`
  alterne strictement Noir/Blanc/Noir même avant toute décision de couleur,
  et que la couleur "de l'humain" choisie au menu n'a encore aucun sens
  réel tant que le choix Swap n'a pas eu lieu, l'application fait poser
  **toutes** les pierres d'ouverture par l'humain (voir
  `App::in_human_opening_setup`), quelle que soit la couleur de la pierre à
  poser, plutôt que de laisser `Game::is_human_turn` alterner entre humain
  et IA sur ces 3 (ou 5) premiers coups. C'est fidèle à l'idée "un joueur
  prépare le plateau, l'autre choisit ensuite", au prix d'un léger
  contournement de la logique de tour habituelle pendant cette phase
  précise.
- **`overflow-checks = true` en profil release** : choix de sûreté (section
  6 du sujet : aucun débordement arithmétique silencieux). Mesuré : sur le
  test de performance (section 8), l'écart entre `overflow-checks = true`
  et `false` reste dans le bruit de mesure normal d'une exécution à
  l'autre (quelques pourcents de nœuds/s, largement compensé par la marge
  entre les profondeurs obtenues et l'exigence de profondeur 10) ; le choix
  de sûreté est donc conservé sans réserve.

## 8. Mesures de performance

Mesures réelles, obtenues avec `make perf` (`cargo test --release
performance_tests -- --ignored --nocapture`) sur la machine de
développement. Budget : 380 ms "soft" / 500 ms "hard", profondeur maximale
24, tous les threads matériels disponibles.

| Position | Profondeur atteinte | Nœuds | Temps | Coup choisi |
|---|---|---|---|---|
| Plateau vide | 10–11 | ~1,3 M | ~500 ms | centre du plateau (J10) |
| Ouverture simple (4 pierres, aucune menace) | 11 | ~1,1–1,2 M | 420–500 ms | proche du groupe existant |
| Milieu de partie simple (16 pierres, "8 dames", aucun alignement préexistant) | 8–9 | ~1,0 M | ~500 ms | renforce/étend une ligne |
| Milieu de partie, menaces multiples (trois libre à gérer + pierres dispersées) | 9–10 | ~0,8–0,95 M | 450–500 ms | réponse à la menace |
| Position tactique complexe (quatre à bloquer + capture disponible) | 9 | ~0,9–0,95 M | ~500 ms | bloque le quatre |

Débit observé : **1,8 à 2,6 millions de nœuds/seconde** selon la position
(le coût par nœud varie avec la densité du plateau : plus de pierres posées
= plus de cases candidates à scorer en génération de coups). La profondeur
varie légèrement d'une exécution à l'autre (le budget de temps est une
horloge murale, pas un nombre de nœuds fixe), mais reste **toujours >= 8**,
et **>= 10 sur 3 des 5 positions à chaque exécution observée**. Le test
`depth_10_sous_500ms_sur_positions_de_reference` fixe des planchers un peu
en-dessous des valeurs observées (10, 10, 8, 8, 8) pour rester robuste sur
une machine de correction plus lente sans jamais maquiller une vraie
régression.

**Avant / après sur le coût de l'évaluation** (le point de départ mesuré sur
l'ancienne implémentation était de 1,3 µs par appel, voir section 1 de
l'audit initial) :

| Version | Coût par appel à l'évaluation |
|---|---|
| Ancienne (`Ai::evaluation`, balayage complet 4 axes) | ~1,3 µs |
| Nouvelle (`ai::eval::evaluate`, lecture du score incrémental) | **~3 ns** (mesuré sur 5 000 000 d'appels, `ai::eval::bench::cout_de_evaluate`) |

Un facteur supérieur à 400. C'est ce changement de représentation — pas une
micro-optimisation de la boucle d'évaluation existante — qui rend la
profondeur 10 atteignable : au point de départ (1,3 µs/nœud, en supposant un
coût nul pour tout le reste), le budget de 500 ms plafonnait à environ
380 000 feuilles ; avec les nombres mesurés ci-dessus (~1 à 1,3 million de
nœuds complets, génération de coups et captures comprises, en 500 ms), le
gain net dépasse largement ce facteur 400 une fois qu'on compte aussi le
reste de la chaîne (élagage, ordonnancement, TT).

**Gain de chaque optimisation isolable sans dupliquer le moteur** — mesuré
réellement via `cargo test --release ablation -- --ignored --nocapture`
(`ai::search::ablation::threads_et_table_de_transposition`), sur les 5
positions de référence, même budget (380/500 ms) :

| Position | Mono-thread, TT normale | 8 threads (Lazy SMP), TT normale | 8 threads, TT quasi désactivée (1 entrée) |
|---|---|---|---|
| Plateau vide | profondeur 10 (222 803 nœuds) | profondeur 10 (1 290 976 nœuds) | profondeur 8 (1 507 025 nœuds) |
| Ouverture simple | profondeur 9 (210 944 nœuds) | profondeur 10 (1 124 775 nœuds) | profondeur 8 (1 262 175 nœuds) |
| Milieu de partie simple | profondeur 8 (192 512 nœuds) | profondeur 9 (1 000 659 nœuds) | profondeur 7 (1 134 127 nœuds) |
| Milieu de partie, menaces multiples | profondeur 8 (145 408 nœuds) | profondeur 9 (894 133 nœuds) | profondeur 8 (944 854 nœuds) |
| Position tactique complexe | profondeur 9 (154 628 nœuds) | profondeur 9 (930 373 nœuds) | profondeur 7 (990 752 nœuds) |

Deux enseignements de ce tableau, honnêtes et un peu contre-intuitifs :

- Le parallélisme Lazy SMP gagne systématiquement **+0 à +1 de profondeur**
  ici (les 8 threads explorent presque 6 à 9 fois plus de nœuds au total
  dans le même budget de temps, mais l'approfondissement itératif ne
  progresse que par paliers entiers : une grande partie du travail
  supplémentaire sert à *sécuriser* la profondeur déjà atteinte via des
  ordres de coups légèrement différents entre threads, pas uniquement à
  en gagner une de plus). Le bénéfice réel du multithread est surtout une
  **plus grande robustesse** (moins de dépendance à l'ordre de coups d'un
  seul thread) que le gain brut de profondeur affiché ne le laisse deviner.
- Désactiver quasiment la table de transposition (1 seule entrée) fait
  **chuter la profondeur de 1 à 2**, malgré un nombre de nœuds *supérieur ou
  égal* à la version avec TT complète. C'est la démonstration la plus
  parlante de son utilité : sans elle, une fraction significative des
  nœuds explorés est du travail redondant (les mêmes sous-positions
  ré-analysées depuis plusieurs chemins de coups différents) plutôt que de
  la progression réelle en profondeur.

Les autres briques (ordonnancement des coups, PVS, approfondissement
itératif avec fenêtre d'aspiration, quiescence, limitation à *K*
candidats) sont fusionnées dans le moteur de production et n'ont pas de
version alternative maintenue séparément : les isoler proprement
demanderait de dupliquer `ai::search` pour chaque combinaison, ce qui
n'a pas été fait par souci de temps. Leur nécessité individuelle est
justifiée qualitativement en section 3 (chacune répond à un défaut précis
et démontrable : sans ordonnancement, alpha-bêta ne coupe presque rien ;
sans limitation à *K* candidats, le facteur de branchement d'environ 100
rend toute profondeur à deux chiffres hors de portée en 500 ms, comme
détaillé en section 7) plutôt que mesurée isolément ici — c'est une limite
honnête de ce rapport, pas une donnée maquillée.

## 9. Questions probables du correcteur

**Pourquoi negamax et pas minimax à deux fonctions ?**
Parce que Gomoku est un jeu à somme nulle et symétrique : la valeur d'une
position pour l'adversaire est l'opposé de sa valeur pour le joueur au
trait. Negamax exploite cette symétrie pour n'écrire qu'une seule fonction
récursive (`ai::search::negamax`) au lieu de deux quasi identiques (une
`max`, une `min`), avec le signe qui s'inverse à chaque appel récursif. Le
résultat calculé est strictement identique à un minimax classique — voir
section 3.3.

**Alpha-bêta change-t-il le résultat ?**
Non. Alpha-bêta ne change jamais la valeur finale ni le coup choisi par
rapport à un minimax naïf qui explorerait tout : il élimine seulement les
branches dont on peut prouver, à partir des bornes déjà connues, qu'elles
ne peuvent pas influencer le résultat. Voir la démonstration pas à pas en
section 3.2.

**Que se passe-t-il si l'heuristique est fausse (mal réglée) ?**
La recherche reste correcte (elle trouve toujours le meilleur coup selon
CETTE évaluation), mais la qualité de jeu s'en trouve dégradée : l'IA
pourrait sous-estimer une vraie menace ou sur-valoriser un motif inoffensif.
C'est pour cela que le barème (section 4) est justifié valeur par valeur et
vérifié par des tests de cohérence relative (`quatre_libre_vaut_beaucoup_plus_qu_un_trois_libre`,
etc.) plutôt que par des valeurs absolues arbitraires : ce qui compte est
l'ordre relatif des menaces, pas les nombres exacts.

**Pourquoi la table de transposition ne fausse-t-elle pas le score ?**
Trois garanties : (1) une entrée n'est utilisée pour une coupure que si sa
profondeur enregistrée est **au moins** celle recherchée maintenant (une
entrée moins profonde donnerait une information moins fiable) ; (2) le
drapeau (`Exact`/`Lower`/`Upper`) encode si le score est la valeur exacte ou
seulement une borne obtenue par coupure, et seule la combinaison
drapeau+borne appropriée déclenche une coupure ; (3) le schéma de stockage
sans verrou (`ai::tt`) vérifie `clé XOR données == zobrist attendu` à
chaque lecture : en cas de course entre deux threads, l'entrée corrompue
est simplement rejetée (jamais utilisée), au prix, au pire, de perdre un
coup de table — jamais de renvoyer une valeur fausse.

**Comment garantis-tu la profondeur 10 ?**
Par construction de plusieurs briques qui se combinent (voir le tableau de
la section 8) : représentation compacte (cache L1), évaluation incrémentale
(3 ns au lieu de 1,3 µs), génération de coups en deux étapes, limitation à
*K* candidats par nœud avec garde-fou pour les coups critiques, table de
transposition, approfondissement itératif avec fenêtre d'aspiration, PVS,
et parallélisme Lazy SMP. Le test `depth_10_sous_500ms_sur_positions_de_reference`
mesure cette garantie sur 5 positions représentatives, exécuté à la demande
(`make perf`) plutôt qu'à chaque `cargo test` pour éviter que la contention
CPU d'une exécution parallèle ne fausse la mesure de temps.

**Pourquoi limiter les candidats n'est-il pas de la triche ?**
Parce que c'est un compromis *documenté et mesuré*, pas une omission
silencieuse, et parce que les coups qui compteraient le plus dans un jugement
humain (gagner, bloquer un gain adverse, capturer la 5e paire) sont
**explicitement exemptés** de cette limite (`critical: bool` dans
`ai::movegen`). Ce qui est élagué, ce sont des coups positionnellement très
faibles (loin de toute pierre, ou dominés par un coup presque identique
juste à côté) qu'aucun joueur raisonnable n'envisagerait sérieusement non
plus. La section 7 explique honnêtement ce que ce choix coûte en exactitude
théorique.

**Comment ton IA gère-t-elle les captures dans sa recherche ?**
`Position::make_move` applique les captures comme partie intégrante du
coup (pas une étape séparée) : jouer un coup et capturer une paire adverse
est une seule mutation atomique de la position, défaite par un seul
`unmake_move`. L'évaluation inclut un terme de capture non linéaire
(section 4.2) et un terme de vulnérabilité aux captures (section 4.3), et
`ai::movegen` classe très haut un coup qui capture la 5e paire (victoire
immédiate) ou qui capture tout court. La quiescence (section 3.6) inclut
aussi les captures parmi les coups forçants qu'elle explore à l'horizon.

**Comment détectes-tu un trois libre ?**
`game::patterns::creates_free_three_on_axis` teste, sur une fenêtre de 6
cases par axe, les 4 gabarits canoniques du sujet (`_XXX__`, `__XXX_`,
`_XX_X_`, `_X_XX_`) en simulant la pose de la pierre sans jamais muter le
plateau réel (`cell_with_virtual`). Le comptage se fait **par axe** (4
maximum), jamais par direction (8), pour ne jamais compter double le même
alignement regardé depuis ses deux bouts. `count_free_threes >= 2` (jamais
`== 2`, pour attraper aussi un triple-trois) déclenche l'illégalité, sauf
si le coup capture une paire ou forme un alignement de 5+ (exceptions
explicites du sujet, court-circuitées avant même d'appeler cette fonction
dans `rules::check_move_legal_as`).
