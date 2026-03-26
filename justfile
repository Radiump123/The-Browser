set shell := ["bash", "-cu"]

default:
    @just --list

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
