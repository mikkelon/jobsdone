# Development entry points. `make check` is what CI runs.

DEV_DATA_DIR := $(CURDIR)/.dev

.PHONY: check fmt test run install uninstall

check:
	cargo fmt --check
	cargo clippy --locked --all-targets -- -D warnings
	cargo test --locked
	bash tests/launcher.sh
	bash tests/release.sh
	python3 tests/release_channels.py

fmt:
	cargo fmt

test:
	cargo test --locked
	bash tests/launcher.sh
	bash tests/release.sh
	python3 tests/release_channels.py

# Runs against a scratch database in .dev, never the real one in
# $XDG_DATA_HOME/jobsdone.
run:
	JOBSDONE_DATA_DIR=$(DEV_DATA_DIR) cargo run

# The binary, a launcher entry, and on Omarchy the window rule and keybind.
install:
	scripts/install

uninstall:
	scripts/uninstall
