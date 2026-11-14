#!/bin/bash
# Roda a `CenaForma` (a forma de seleção e de colisão dos transmissores: `cenas/forma.txt`) no cliente oficial (`runGameTestClient`, num gamescope headless: nenhuma janela abre no desktop) com as ações de
# `cenas/forma.txt` e devolve os prints e o `latest.log` (as linhas `cena:`). A árvore volta como estava: a `CenaForma.java` e o `cena-forma` saem, o `cena-cabo` e o `options.txt` voltam. Roda no desktop.
# Uso: oficial_forma.sh CenaForma.java cenas/forma.txt PASTA_DOS_PRINTS   (DONO=nome da frente)
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` (o checkout do original e o gradle) com `mkdir`, espera em laço de `mkdir` a cada 5 s (nunca `pgrep`) e a solta ao
# sair. O gradle passa pelo `lim-build` (uma vaga de build, 4 núcleos). O jogo é mudo: o volume mestre vai a 0 (o `options.txt` volta ao fim).
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
PRINTS=$3
DONO=${DONO:-cabo-forma}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
# JA_COM_TRAVA=1: quem chamou já tem a `mekanism-ref.lock`: este não a pega nem a solta.
if [ "${JA_COM_TRAVA:-}" != 1 ]; then
  until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
  echo "$DONO" > "$LOCK/dono"
  echo "TRAVA $(date +%T)"
fi
restore() {
  rm -f "$N/CenaForma.java" "$R/cena-forma" "$R"/screenshots/forma_oficial_*.png
  [ -e "$C/cena-cabo.antes-forma" ] && mv "$C/cena-cabo.antes-forma" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-forma" ] && cp "$C/options.txt.antes-forma" "$R/options.txt" && rm -f "$C/options.txt.antes-forma"
  [ "${JA_COM_TRAVA:-}" = 1 ] || rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-forma"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-forma"
rm -f "$R"/screenshots/forma_oficial_*.png
cp "$JAVA" "$N/CenaForma.java"
cp "$CENA" "$R/cena-forma"
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
mkdir -p "$PRINTS"; cp "$R"/screenshots/forma_oficial_*.png "$PRINTS"/ 2>/dev/null; cp "$R/logs/latest.log" "$PRINTS/latest.log" 2>/dev/null
ls "$PRINTS"
