#!/bin/sh
# Todos os gates do projeto. A CI roda este mesmo comando.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
