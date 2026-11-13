#!/bin/bash
# Roda no desktop. Roda a `CenaSetups` (os setups reais da decisão 196: linhas longas de transmissores montadas por comando num mundo superplano, o mundo que sai e volta, o jogador que se afasta e volta) no cliente oficial (`runGameTestClient`, num gamescope
# headless: nenhuma janela abre no desktop; o som fica desligado) e devolve o `setups-reais.jsonl` e as linhas do log. A árvore volta como estava: a `CenaSetups.java` e o `cena-setups` saem, os
# mundos `SetupsA` e `SetupsB` são apagados, o `GameTestWorld` volta ao lugar e o `options.txt` também.
# Uso: oficial_setups.sh testes/CenaSetups.java cenas/setups_reais.txt PASTA_DA_SAIDA
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` com `mkdir` (espera em laço de `mkdir` a cada 5 s, nunca `pgrep`) e a solta ao sair. O gradle passa pelo `lim-build` (uma vaga, 4 núcleos).
# O container `mekanism-ref` recebe o teto da vaga (`docker update`) dentro dela; o pico de memória sai em `memoria.txt` (amostrado a cada 3 s).
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
DONO=${DONO:-setups-reais}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
MUNDO=$R/saves/GameTestWorld
ANTES=$R/saves/GameTestWorld.antes-setups
until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
MEM=
restore() {
  [ -n "$MEM" ] && kill "$MEM" 2>/dev/null
  rm -f "$N/CenaSetups.java" "$R/cena-setups" "$R/setups-reais.jsonl"
  rm -rf "$R/saves/SetupsA" "$R/saves/SetupsB"
  [ -e "$C/cena-cabo.antes-setups" ] && mv "$C/cena-cabo.antes-setups" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-setups" ] && cp "$C/options.txt.antes-setups" "$R/options.txt" && rm -f "$C/options.txt.antes-setups"
  if [ -e "$ANTES" ]; then rm -rf "$MUNDO"; mv "$ANTES" "$MUNDO"; fi
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
[ -e "$ANTES" ] && { echo "sobrou $ANTES de uma rodada anterior: confira antes de seguir"; exit 4; }
mkdir -p "$SAIDA"
cp "$R/options.txt" "$C/options.txt.antes-setups"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-setups"
[ -e "$MUNDO" ] && mv "$MUNDO" "$ANTES"
rm -rf "$R/saves/SetupsA" "$R/saves/SetupsB"
cp "$JAVA" "$N/CenaSetups.java"
cp "$CENA" "$R/cena-setups"
JOGO_CMD=true "$HOME/.local/bin/mekanism-ref" offline > /dev/null 2>&1
( while :; do docker stats --no-stream --format '{{.MemUsage}}' mekanism-ref 2>/dev/null >> "$SAIDA/memoria.txt"; sleep 3; done ) &
MEM=$!
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
export JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"'
# Dentro da vaga: o teto do container e o jogo.
"$C/lim-build" bash -c 'docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref > /dev/null; exec "$HOME/.local/bin/mekanism-ref"' > "$SAIDA/gradle.log" 2>&1
echo "EXIT $?"
cp "$R/setups-reais.jsonl" "$SAIDA/" 2>/dev/null
grep -aE "CENA-SETUPS|Dealing with|No networks found|Merging|Adding .* transmitters|orphan|invalid Transmitters" "$R/logs/latest.log" > "$SAIDA/log.txt" 2>/dev/null
ls -la "$SAIDA"
