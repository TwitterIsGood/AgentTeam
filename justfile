default:
    just --list

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

check:
    cargo check --workspace

doctor:
    cargo run -p forgeflow-cli -- doctor

dry-run:
    cargo run -p forgeflow-cli -- workflow run --dry-run
