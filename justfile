setup:
    cargo fetch --locked
    cargo build -p dx-cli --locked

check:
    rust-analyzer diagnostics .
    cargo fmt --all --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

build:
    cargo build -p dx-cli --locked

pi-dev:
  pi -e ./pi-dx/extensions/pi-dx.ts
