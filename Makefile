# abrightd — convenience targets.
#
#   make configure VARIANT=kde   choose the build variant (or ./configure kde)
#   make build                   release build (FEATURES=tui by default)
#   make install                 install + start the daemon (+ desktop if configured)
#   make install-desktop         install desktop components on their own
#   make uninstall               remove the daemon
#   make test / fmt / clippy
FEATURES ?= tui
VARIANT ?=

.PHONY: all configure build release install install-desktop uninstall test fmt clippy clean

all: build

configure:
	./configure $(VARIANT)

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
