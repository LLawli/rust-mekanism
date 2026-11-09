#!/bin/bash
# Roda a `CenaBlocos` no cliente oficial (`runGameTestClient`, sem janela, sem som) com o roteiro `cenas/blocos.json` e devolve a árvore como estava: o `cena-cabo` volta, o
# `options.txt` volta, e a `CenaBlocos.java` e o `cena-blocos.json` saem. Roda no desktop, com a `mekanism-ref.lock` na mão. Os prints ficam em SAIDA (uma pasta):
# `oficial_<tiro>.png`, de 1920 por 1080. Uso: oficial_blocos.sh CenaBlocos.java blocos.json SAIDA
# O volume mestre vai a 0 no `options.txt` do cliente (que volta ao fim): o jogo de teste nunca toca pelas caixas do usuário.
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
if docker ps --format '{{.Names}}' | grep -qx mekanism-ref; then echo OCUPADO; exit 3; fi
mkdir -p "$SAIDA"
cp "$R/options.txt" "$C/options.txt.antes-blocos"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-blocos"
restore() {
  rm -f "$N/CenaBlocos.java" "$R/cena-blocos.json"
  [ -e "$C/cena-cabo.antes-blocos" ] && mv "$C/cena-cabo.antes-blocos" "$R/cena-cabo"
  cp "$C/options.txt.antes-blocos" "$R/options.txt" && rm -f "$C/options.txt.antes-blocos"
  echo RESTAURADO
}
trap restore EXIT
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
cp "$JAVA" "$N/CenaBlocos.java"
cp "$CENA" "$R/cena-blocos.json"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
cp "$R"/screenshots/oficial_*.png "$SAIDA"/ 2>/dev/null; rm -f "$R"/screenshots/oficial_*.png
ls "$SAIDA" | wc -l
