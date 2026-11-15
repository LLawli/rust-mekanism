#!/bin/bash
# Roda a `CenaTelas` no cliente oficial (`runGameTestClient`, sem janela) com uma lista de casos e devolve a árvore como estava: o `cena-cabo` volta, o
# `options.txt` volta, e a `CenaTelas.java` e o `cena-telas` saem. Roda no desktop. Uso: oficial_fora.sh CenaTelas.java cena-telas SAIDA.jsonl
# `cena-telas` vem de `cena_telas.py oficial ARQUIVO ESCALA --so casos...`; SAIDA é o `fora_rastro.jsonl` que os casos com `fora=1` gravam (`rastros/fora.jsonl`).
# `LARGURA` e `ALTURA` (padrão 1280 por 720) mudam a janela: os prints que o `comparar_lote.py` compara com os do port são de 1920 por 1080.
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
# Mudo: o jogo de teste nunca toca nas caixas do usuário.
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
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
LARGURA=${LARGURA:-1280}; ALTURA=${ALTURA:-720}; JANELA="-W $LARGURA -H $ALTURA -w $LARGURA -h $ALTURA"
# `SEM_LIM=1`: a vaga já é de quem chama (o `lim-jogo` a segura); passar de novo por `lim-build` esperaria por ela para sempre.
LIM="$C/lim-build"; [ "${SEM_LIM:-}" = 1 ] && LIM=
$LIM env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
[ -s "$R/fora_rastro.jsonl" ] && cp "$R/fora_rastro.jsonl" "$SAIDA" && rm -f "$R/fora_rastro.jsonl"
# Os prints que a cena tira ficam em screenshots/ com o nome `oficial_<caso>_e<escala>.png`: apague-os pelo nome.
