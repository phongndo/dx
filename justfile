setup:
    cargo fetch --locked
    cargo build -p mark-cli --locked

check:
    scripts/check-architecture
    rust-analyzer diagnostics .
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

check-architecture:
    scripts/check-architecture

fmt:
    cargo fmt --all

test:
    cargo test --workspace --all-targets --all-features --locked

build:
    cargo build -p mark-cli --locked

hooks:
    git config core.hooksPath .githooks

pi-check:
    cd pi-mark && pnpm run check

pi-dev:
    pi -e ./pi-mark/extensions/pi-mark.ts
