# Projet Gomoku (42) — voir DEFENSE.md pour l'explication complète du
# moteur et README.md pour l'utilisation.
#
# Règle centrale du sujet : un second `make` consécutif ne doit RIEN
# reconstruire. On s'appuie sur les timestamps de make lui-même : la cible
# $(NAME) ne dépend que des sources réelles (fichiers .rs + Cargo.toml/lock),
# donc si aucune n'a changé, make considère la cible à jour et n'invoque même
# pas `cargo`. `cargo build` seul ne suffirait pas à garantir cela de façon
# fiable pour la règle "pas de relink" : c'est make, pas cargo, qui décide de
# rebuild ou non ici.

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

# Test de performance (profondeur >= 10 en moins de 500 ms sur 5 positions
# de référence, voir ai/search.rs). Séparé de `test` : il est marqué
# `#[ignore]` par défaut car sa mesure de temps serait faussée par la
# contention CPU d'une exécution en parallèle des autres tests.
perf:
	cargo test --release performance_tests -- --ignored --nocapture

clippy:
	cargo clippy --release --all-targets

.PHONY: all clean fclean re test perf clippy
