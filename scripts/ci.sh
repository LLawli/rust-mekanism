#!/bin/sh
# Todos os gates do projeto. A CI roda este mesmo comando.
set -eu
cd "$(dirname "$0")/.."
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo audit
# O comparador de telas de `dev/referencia`, só com a biblioteca padrão do Python.
python3 -I -B -m unittest discover -s dev/referencia -p 'test_*.py'
