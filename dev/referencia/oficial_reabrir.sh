#!/bin/bash
# Roda no desktop. Roda a `CenaReabrir` (a cena do cabo com cubos e outro cabo, o mundo que sai para o menu e é reaberto no mesmo processo) no cliente oficial (`runGameTestClient`, num gamescope
# headless; o som fica desligado) e devolve o `latest.log` (as linhas `cena: reabrir ...`). A árvore volta como estava: a `CenaReabrir.java` e o `cena-reabrir` saem, o mundo `ReabrirA` é apagado, o
# `GameTestWorld` volta ao lugar e o `options.txt` também. Uso: oficial_reabrir.sh testes/CenaReabrir.java PASTA_DA_SAIDA   (DONO=nome da frente)
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` com `mkdir` (laço a cada 5 s, nunca `pgrep`) e a solta ao sair. O gradle passa pelo `lim-build` (uma vaga, 4 núcleos).
set -u
JAVA=$(readlink -f "$1")
SAIDA=$2
DONO=${DONO:-cabo-forma}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
MUNDO=$R/saves/GameTestWorld
ANTES=$R/saves/GameTestWorld.antes-reabrir
mkdir -p "$SAIDA"
trap 'echo $? > "$SAIDA/fim"' EXIT
until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  rm -f "$N/CenaReabrir.java" "$R/cena-reabrir"
  rm -rf "$R/saves/ReabrirA"
  [ -e "$C/cena-cabo.antes-reabrir" ] && mv "$C/cena-cabo.antes-reabrir" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-reabrir" ] && cp "$C/options.txt.antes-reabrir" "$R/options.txt" && rm -f "$C/options.txt.antes-reabrir"
  if [ -e "$ANTES" ]; then rm -rf "$MUNDO"; mv "$ANTES" "$MUNDO"; fi
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap 'restore; echo $? > "$SAIDA/fim"' EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
[ -e "$ANTES" ] && { echo "sobrou $ANTES de uma rodada anterior: confira antes de seguir"; exit 4; }
cp "$R/options.txt" "$C/options.txt.antes-reabrir"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-reabrir"
[ -e "$MUNDO" ] && mv "$MUNDO" "$ANTES"
rm -rf "$R/saves/ReabrirA"
cp "$JAVA" "$N/CenaReabrir.java"
touch "$R/cena-reabrir"
JOGO_CMD=true "$HOME/.local/bin/mekanism-ref" offline > /dev/null 2>&1
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
export JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"'
"$C/lim-build" bash -c 'docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref > /dev/null; exec "$HOME/.local/bin/mekanism-ref"' > "$SAIDA/gradle.log" 2>&1
echo "EXIT $?"
cp "$R/logs/latest.log" "$SAIDA/latest.log" 2>/dev/null
grep -a "cena:" "$SAIDA/latest.log" > "$SAIDA/cena.txt" 2>/dev/null
ls -la "$SAIDA"
