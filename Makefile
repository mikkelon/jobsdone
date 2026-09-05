# Development entry points. `make check` is what CI runs.

DEV_DATA_DIR := $(CURDIR)/.dev

.PHONY: check fmt test run install

check:
	cargo fmt --check
	cargo clippy --all-targets -- -D warnings
	cargo test

fmt:
	cargo fmt

test:
	cargo test

# Runs against a scratch database in .dev, never the real one in
# $XDG_DATA_HOME/jobsdone.
run:
	JOBSDONE_DATA_DIR=$(DEV_DATA_DIR) cargo run

install:
	cargo install --path .
