
all:
	cargo run

clean:

fclean:
	cargo clean

re: fclean all

.PHONY: all clean fclean re