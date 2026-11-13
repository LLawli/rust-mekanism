#!/bin/bash
# Monta, no notebook, o tar dos fontes do workspace no layout que o desktop espera em ~/Games/Horizon/mods (infuser/ e conteudo/), com `TRACE = true` no
# `tile_sound.rs` (a cópia de teste do som; o repositório fica com `false`) e `FX_TRACE = true` e `TRANSMITTER_PROBE = true` no `glue.rs` da `machinery_basis` (o rastro das partículas e a sonda dos eventos dos transmissores), e o envia para ~/.cache/rust-mekanism/<pasta>/fontes.tar.
# Uso: enviar_fontes.sh WORKSPACE PASTA_NO_DESKTOP [trace|sem-trace|setups]   (`setups`: só a sonda leve `SETUPS_PROBE` do `glue.rs`, sem uma linha por evento: o servidor anda a 20 por segundo)
set -eu
WS=$(readlink -f "$1"); DEST=$2; TRACE=${3:-trace}
S=$(mktemp -d "${TMPDIR:-/tmp}/fontes.XXXXXX")
mkdir -p "$S/infuser" "$S/conteudo"
cp -a "$WS/mods/machinery_basis" "$S/infuser/machinery_basis"
cp -a "$WS/mods/mekanism/native" "$S/infuser/mekanism_native"
cp -a "$WS/mods/mekanismgenerators/native" "$S/infuser/mekanismgenerators_native"
for x in mekanism mekanismgenerators; do cp -a "$WS/mods/$x/content" "$S/conteudo/$x"; done
find "$S" -name target -prune -exec rm -rf {} + 2>/dev/null || true
# Os fontes saem com o mtime de agora: o cache Rust do desktop é um só para `horizon` e `horizon2`, e um fonte com mtime antigo faz o cargo reusar o rlib que outra frente compilou
# (`machinery_basis_core` sem o `hopper`, medido em 2026-11-12), sem recompilar.
find "$S" -name '*.rs' -exec touch {} +
if [ "$TRACE" = setups ]; then sed -i 's/^const SETUPS_PROBE: bool = false;/const SETUPS_PROBE: bool = true;/' "$S/infuser/machinery_basis/glue.rs"; grep -c '^const SETUPS_PROBE: bool = true;' "$S/infuser/machinery_basis/glue.rs"; sed -i 's/^const DROP_PROBE: bool = false;/const DROP_PROBE: bool = true;/' "$S/infuser/machinery_basis/transmitter_events.rs"; grep -c '^const DROP_PROBE: bool = true;' "$S/infuser/machinery_basis/transmitter_events.rs"; fi
if [ "$TRACE" = perfil ]; then sed -i 's/^const PERF_PROBE: bool = false;/const PERF_PROBE: bool = true;/' "$S/infuser/machinery_basis/glue.rs"; grep -c '^const PERF_PROBE: bool = true;' "$S/infuser/machinery_basis/glue.rs"; fi
if [ "$TRACE" = trace ]; then sed -i 's/^const SOURCE_TRACE: bool = false;/const SOURCE_TRACE: bool = true;/' "$S/infuser/machinery_basis/transmitter_events.rs"; grep -c '^const SOURCE_TRACE: bool = true;' "$S/infuser/machinery_basis/transmitter_events.rs"; sed -i 's/^const TRACE: bool = false;/const TRACE: bool = true;/' "$S/infuser/machinery_basis/tile_sound.rs"; grep -c '^const TRACE: bool = true;' "$S/infuser/machinery_basis/tile_sound.rs"; sed -i 's/^const FX_TRACE: bool = false;/const FX_TRACE: bool = true;/' "$S/infuser/machinery_basis/glue.rs"; grep -c '^const FX_TRACE: bool = true;' "$S/infuser/machinery_basis/glue.rs"; sed -i 's/^const TRANSMITTER_PROBE: bool = false;/const TRANSMITTER_PROBE: bool = true;/' "$S/infuser/machinery_basis/glue.rs"; grep -c '^const TRANSMITTER_PROBE: bool = true;' "$S/infuser/machinery_basis/glue.rs"; fi
tar -C "$S" -cf "$S.tar" infuser conteudo
ssh desktop "mkdir -p ~/.cache/rust-mekanism/$DEST" && scp -q "$S.tar" "desktop:.cache/rust-mekanism/$DEST/fontes.tar"
rm -rf "$S" "$S.tar"
