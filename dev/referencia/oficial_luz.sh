#!/bin/bash
# Roda a `CenaLuz` no cliente oficial (`runGameTestClient`, num gamescope headless, sem som) com a lista de blocos de `cenas/luz.txt` e devolve `luz_trace.jsonl` (o sombreamento de vértice do
# vanilla e do NeoForge para cada quad) em SAIDA. Devolve a árvore como estava: a `CenaLuz.java`, o `cena-luz` e o `luz_trace.jsonl` saem, o `cena-cabo` e o `options.txt` voltam.
# Roda no desktop, com a `mekanism-ref.lock` na mão. Uso: oficial_luz.sh CenaLuz.java luz.txt SAIDA
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
if docker ps --format '{{.Names}}' | grep -qx mekanism-ref; then echo OCUPADO; exit 3; fi
mkdir -p "$SAIDA"
cp "$R/options.txt" "$C/options.txt.antes-luz"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-luz"
restore() {
  rm -f "$N/CenaLuz.java" "$R/cena-luz" "$R/luz_trace.jsonl"
  [ -e "$C/cena-cabo.antes-luz" ] && mv "$C/cena-cabo.antes-luz" "$R/cena-cabo"
  cp "$C/options.txt.antes-luz" "$R/options.txt" && rm -f "$C/options.txt.antes-luz"
  echo RESTAURADO
}
trap restore EXIT
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
cp "$JAVA" "$N/CenaLuz.java"
cp "$CENA" "$R/cena-luz"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
cp "$R/luz_trace.jsonl" "$SAIDA"/ 2>/dev/null
ls -la "$SAIDA"
