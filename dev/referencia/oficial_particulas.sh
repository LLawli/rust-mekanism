#!/bin/bash
# Roda a `CenaParticulas` no cliente oficial (`runGameTestClient`, num gamescope headless: nenhuma janela abre no desktop) com uma lista de etapas e
# devolve a árvore como estava: a `CenaParticulas.java` e o `cena-particulas` saem, o `cena-cabo` e o `options.txt` voltam. Roda no desktop.
# Uso: oficial_particulas.sh CenaParticulas.java cenas/particulas.txt SAIDA.jsonl [PASTA_DOS_PRINTS]
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` (o checkout do original e o gradle) com `mkdir`, espera em laço de `mkdir` a cada 5 s (nunca
# `pgrep`; quem solta e quer pegar de novo perde a corrida para o laço de 60 s dos outros), e a solta ao sair. O gradle passa pelo `lim-build` (1 build por vez, 4 núcleos). O jogo é mudo: o volume mestre vai a 0 (o `options.txt` volta ao fim).
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
PRINTS=${4:-}
DONO=${DONO:-particulas}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  rm -f "$N/CenaParticulas.java" "$R/cena-particulas" "$R/particulas.jsonl" "$R"/screenshots/particulas_oficial_*.png
  [ -e "$C/cena-cabo.antes-particulas" ] && mv "$C/cena-cabo.antes-particulas" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-particulas" ] && cp "$C/options.txt.antes-particulas" "$R/options.txt" && rm -f "$C/options.txt.antes-particulas"
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-particulas"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-particulas"
rm -f "$R/particulas.jsonl" "$R"/screenshots/particulas_oficial_*.png
cp "$JAVA" "$N/CenaParticulas.java"
cp "$CENA" "$R/cena-particulas"
LARGURA=${LARGURA:-1920}; ALTURA=${ALTURA:-1080}; JANELA="-W $LARGURA -H $ALTURA -w $LARGURA -h $ALTURA"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
[ -s "$R/particulas.jsonl" ] && cp "$R/particulas.jsonl" "$SAIDA"
if [ -n "$PRINTS" ]; then mkdir -p "$PRINTS"; cp "$R"/screenshots/particulas_oficial_*.png "$PRINTS"/ 2>/dev/null; fi
