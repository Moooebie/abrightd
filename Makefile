# abrightd — convenience targets.
#
#   make build            release build (FEATURES=tui by default)
#   make install          install + start the daemon for the current user
#   make install-desktop  install desktop components (KDE Plasma applet)
#   make uninstall        remove the daemon
#   make test / fmt / clippy
FEATURES ?= tui

.PHONY: all build release install install-desktop uninstall test fmt clippy clean

all: build

build:
	FEATURES=$(FEATURES) ./build.sh

release: build

install:
	./install.sh

install-desktop:
	./install-desktop.sh

uninstall:
	./uninstall.sh

test:
	cargo test --all-features

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-features --all-targets

clean:
	cargo clean
