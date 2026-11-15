#!/bin/bash
# Roda a `CenaItem` no cliente oficial (`runGameTestClient`, sem janela, sem som) com o roteiro `cenas/item_conteudo.json` e devolve a árvore como estava: o `cena-cabo` volta, o
# `options.txt` volta, e a `CenaItem.java` e o `cena-item.json` saem. Roda no desktop, com a `mekanism-ref.lock` na mão. Os prints ficam em SAIDA (uma pasta):
# `oficial_<tiro>.png`, de 1920 por 1080. Uso: oficial_item.sh CenaItem.java item_conteudo.json SAIDA
# LUZ_MELHORADA=0 desliga `enhancedLighting` no `config/neoforge-client.toml` do cliente (o NeoForge troca o `BlockModelLighter` do vanilla pelo `EnhancedBlockModelLighter`; o arquivo volta ao fim).
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
cp "$R/options.txt" "$C/options.txt.antes-item"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-item"
restore() {
  rm -f "$N/CenaItem.java" "$R/cena-item.json"
  [ -e "$C/cena-cabo.antes-item" ] && mv "$C/cena-cabo.antes-item" "$R/cena-cabo"
  cp "$C/options.txt.antes-item" "$R/options.txt" && rm -f "$C/options.txt.antes-item"
  echo RESTAURADO
}
CONFIG=$R/config/neoforge-client.toml
if [ "${LUZ_MELHORADA:-1}" = 0 ]; then
  cp "$CONFIG" "$C/neoforge-client.toml.antes-item"
  sed -i 's/^enhancedLighting = .*/enhancedLighting = false/' "$CONFIG"
  grep -c '^enhancedLighting = false' "$CONFIG"
fi
trap 'restore; [ -e "$C/neoforge-client.toml.antes-item" ] && mv "$C/neoforge-client.toml.antes-item" "$CONFIG"' EXIT
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
cp "$JAVA" "$N/CenaItem.java"
cp "$CENA" "$R/cena-item.json"
# `SEM_LIM=1`: a vaga já é de quem chama (o `lim-jogo` a segura); passar de novo por `lim-build` esperaria por ela para sempre.
LIM="$C/lim-build"; [ "${SEM_LIM:-}" = 1 ] && LIM=
$LIM env JOGO_CMD='gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
cp "$R"/screenshots/oficial_*.png "$SAIDA"/ 2>/dev/null; rm -f "$R"/screenshots/oficial_*.png
ls "$SAIDA" | wc -l
