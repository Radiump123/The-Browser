.PHONY: help bootstrap build browser lint test budgie-check budgie-build budgie-run-control budgie-run-theme budgie-ui screenshot clean

help:
	@echo "Targets:"
	@echo "  make bootstrap       # install JS deps"
	@echo "  make build           # build main browser sources"
	@echo "  make browser         # run browser from source checkout"
	@echo "  make lint            # lint browser code"
	@echo "  make test            # run browser test harness"
	@echo "  make budgie-check    # check Budgie Rust/Python tools"
	@echo "  make budgie-build    # build Budgie Rust tools"
	@echo "  make budgie-run-control # run Budgie Control daemon"
	@echo "  make budgie-run-theme   # run Budgie Theme Engine"
	@echo "  make budgie-ui       # open Budgie Control panel in browser"

bootstrap:
	npm install

build:
	npm run build

browser:
	npm run start

lint:
	npm run lint

test:
	npm run test

budgie-check:
	cargo check --manifest-path tools/budgie-control-daemon/Cargo.toml
	cargo check --manifest-path tools/budgie-theme-engine/Cargo.toml
	python3 scripts/budgie_app_bridge.py --help

budgie-build:
	cargo build --manifest-path tools/budgie-control-daemon/Cargo.toml
	cargo build --manifest-path tools/budgie-theme-engine/Cargo.toml

budgie-run-control:
	cargo run --manifest-path tools/budgie-control-daemon/Cargo.toml -- --process-name budgie-browser --bind 127.0.0.1:47831

budgie-run-theme:
	cargo run --manifest-path tools/budgie-theme-engine/Cargo.toml -- --theme configs/themes/midnight-budgie.json --watch

budgie-ui:
	xdg-open http://127.0.0.1:47831/

clean:
	cargo clean --manifest-path tools/budgie-control-daemon/Cargo.toml
	cargo clean --manifest-path tools/budgie-theme-engine/Cargo.toml
