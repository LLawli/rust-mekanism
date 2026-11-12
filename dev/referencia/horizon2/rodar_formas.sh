#!/bin/bash
# Roda no desktop. Grava `energy_cube_shapes.jsonl` (EnergyCubeShapeTraceTest) no checkout do original: espera a `mekanism-ref.lock` (laço de `mkdir`), copia o teste, roda o
# `runGameTestServer` na vaga (lim-build), copia o rastro para ~/.cache/rust-mekanism/cubo-forma/ e apaga o teste e a trava. Uso: rodar_formas.sh PASTA_COM_O_TESTE
set -u
D=$HOME/.cache/rust-mekanism; L=$D/mekanism-ref.lock
SRC=$HOME/.local/share/jogo-isolado/mekanism-ref/src
T=$SRC/src/gameTest/java/mekanism/common/tests/network
until mkdir "$L" 2>/dev/null; do sleep 30; done
echo cubo-forma > "$L/dono"
echo "TRAVA $(date +%T)"
cp "$1/EnergyCubeShapeTraceTest.java" "$T/"
"$D/lim-build" env JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon --max-workers=4 runGameTestServer' "$HOME/.local/bin/mekanism-ref" offline
echo "EXIT $?"
cp "$SRC/runs/gameTestServer/trace/energy_cube_shapes.jsonl" "$1/" && echo copiado
rm -f "$T/EnergyCubeShapeTraceTest.java"
rm -rf "$L"
echo FIM
