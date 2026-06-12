export NAME := 'cosmic-paste'

default: test

clean:
    cargo clean

build:
    cargo build --workspace

build-release:
    cargo build --release --workspace

test:
    cargo test --workspace

check:
    cargo clippy --workspace --all-targets -- -D warnings

run-daemon:
    cargo run -p cosmic-paste-daemon

run-applet:
    cargo run -p cosmic-paste-applet