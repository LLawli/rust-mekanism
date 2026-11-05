#!/bin/sh
# Todos os gates do projeto. A CI roda este mesmo comando.
set -eu
cd "$(dirname "$0")/../mods/machinery_basis/core"
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo audit
