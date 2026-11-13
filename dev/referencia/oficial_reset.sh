#!/bin/bash
# Roda no desktop. Roda a `CenaReset` (o mundo que sai para o menu e volta, um segundo mundo e outra dimensão, no mesmo processo) no cliente oficial (`runGameTestClient`, num gamescope
# headless: nenhuma janela abre no desktop; o som fica desligado) e devolve o `reset-mundo.jsonl` e as linhas do log. A árvore volta como estava: a `CenaReset.java` e o `cena-reset` saem, os
# mundos `ResetA` e `ResetB` são apagados, o `GameTestWorld` volta ao lugar e o `options.txt` também.
# Uso: oficial_reset.sh testes/CenaReset.java PASTA_DA_SAIDA
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` com `mkdir` (espera com `test -d` a cada 60 s, nunca `pgrep`) e a solta ao sair. O gradle passa pelo `lim-build` (uma vaga, 4 núcleos).
# O container `mekanism-ref` recebe o teto da vaga (`docker update`) dentro dela; o pico de memória sai em `memoria.txt` (amostrado a cada 3 s).
set -u
JAVA=$(readlink -f "$1")
SAIDA=$2
DONO=${DONO:-reset-mundo}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
MUNDO=$R/saves/GameTestWorld
ANTES=$R/saves/GameTestWorld.antes-reset
until mkdir "$LOCK" 2>/dev/null; do sleep 60; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
MEM=
restore() {
  [ -n "$MEM" ] && kill "$MEM" 2>/dev/null
  rm -f "$N/CenaReset.java" "$R/cena-reset" "$R/reset-mundo.jsonl"
  rm -rf "$R/saves/ResetA" "$R/saves/ResetB"
  [ -e "$C/cena-cabo.antes-reset" ] && mv "$C/cena-cabo.antes-reset" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-reset" ] && cp "$C/options.txt.antes-reset" "$R/options.txt" && rm -f "$C/options.txt.antes-reset"
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
cp "$R/options.txt" "$C/options.txt.antes-reset"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-reset"
[ -e "$MUNDO" ] && mv "$MUNDO" "$ANTES"
rm -rf "$R/saves/ResetA" "$R/saves/ResetB"
cp "$JAVA" "$N/CenaReset.java"
touch "$R/cena-reset"
JOGO_CMD=true "$HOME/.local/bin/mekanism-ref" offline > /dev/null 2>&1
( while :; do docker stats --no-stream --format '{{.MemUsage}}' mekanism-ref 2>/dev/null >> "$SAIDA/memoria.txt"; sleep 3; done ) &
MEM=$!
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
export JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"'
# Dentro da vaga: o teto do container e o jogo.
"$C/lim-build" bash -c 'docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref > /dev/null; exec "$HOME/.local/bin/mekanism-ref"' > "$SAIDA/gradle.log" 2>&1
echo "EXIT $?"
cp "$R/reset-mundo.jsonl" "$SAIDA/" 2>/dev/null
grep -aE "CENA-RESET|Dealing with|No networks found|Merging|Adding .* transmitters|orphan|invalid Transmitters" "$R/logs/latest.log" > "$SAIDA/log.txt" 2>/dev/null
ls -la "$SAIDA"
