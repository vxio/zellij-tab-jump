# zellij-tab-jump — common dev commands.
# Run `just` with no args to see this list.

plugin_path := "~/.config/zellij/plugins/tab-jump.wasm"
plugin_url  := "file:" + plugin_path
wasm_artifact := "target/wasm32-wasip1/release/zellij-tab-jump.wasm"

default:
    @just --list

# Build the wasm artifact in release mode.
build:
    cargo build --release

# Run rustfmt --check + clippy + build, same as CI.
check:
    cargo fmt --check
    cargo clippy --release --target wasm32-wasip1 -- -D warnings
    cargo build --release

# Auto-fix what `check` would flag (fmt + clippy --fix).
fix:
    cargo fmt
    cargo clippy --release --target wasm32-wasip1 --fix --allow-dirty --allow-staged

# Copy the freshly-built wasm into the live zellij plugin dir.
install: build
    mkdir -p {{ parent_directory(plugin_path) }}
    cp {{ wasm_artifact }} {{ plugin_path }}

# Hot-reload every running plugin instance against the installed wasm.
reload:
    zellij action start-or-reload-plugin {{ plugin_url }}

# Full dev cycle: build → install → reload. Run after each edit.
dev: install reload

# Auto-run `just dev` whenever src/ changes.
# Requires `cargo install cargo-watch`.
watch:
    cargo watch -w src -s 'just dev'

# Install the repo-tracked pre-commit hook (one-time per clone).
install-hooks:
    git config core.hooksPath hooks
    @echo "Pre-commit hook enabled. Disable with: git config --unset core.hooksPath"
