#!/bin/bash
# Roda a `CenaTelas` no cliente oficial (`runGameTestClient`, sem janela) com uma lista de casos e devolve a árvore como estava: o `cena-cabo` volta, o
# `options.txt` volta, e a `CenaTelas.java` e o `cena-telas` saem. Roda no desktop. Uso: oficial_fora.sh CenaTelas.java cena-telas SAIDA.jsonl
# `cena-telas` vem de `cena_telas.py oficial ARQUIVO ESCALA --so casos...`; SAIDA é o `fora_rastro.jsonl` que os casos com `fora=1` gravam (`rastros/fora.jsonl`).
# Confere antes que o `mekanism-ref` está ocioso, e passa o gradle pelo `lim-build` (1 build por vez, 4 núcleos); a janela é um gamescope headless.
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
if docker ps --format '{{.Names}}' | grep -qx mekanism-ref; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-cena"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-cena"
rm -f "$R/fora_rastro.jsonl"
restore() {
  rm -f "$N/CenaTelas.java" "$R/cena-telas"
  [ -e "$C/cena-cabo.antes-cena" ] && mv "$C/cena-cabo.antes-cena" "$R/cena-cabo"
  cp "$C/options.txt.antes-cena" "$R/options.txt" && rm -f "$C/options.txt.antes-cena"
  echo RESTAURADO
}
trap restore EXIT
cp "$JAVA" "$N/CenaTelas.java"
cp "$CENA" "$R/cena-telas"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless -W 1280 -H 720 -w 1280 -h 720 -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
[ -s "$R/fora_rastro.jsonl" ] && cp "$R/fora_rastro.jsonl" "$SAIDA" && rm -f "$R/fora_rastro.jsonl"
# Os prints que a cena tira ficam em screenshots/ com o nome `oficial_<caso>_e<escala>.png`: apague-os pelo nome.
