#!/bin/bash
# Roda a `CenaLeitor` (o chat do Network Reader depois do clique) no cliente oficial (`runGameTestClient`, num gamescope headless: nenhuma janela abre no desktop) com as etapas de
# `cenas/leitor.txt` e devolve os prints. A árvore volta como estava: a `CenaLeitor.java` e o `cena-leitor` saem, o `cena-cabo` e o `options.txt` voltam. Roda no desktop.
# Uso: oficial_leitor.sh testes/CenaLeitor.java cenas/leitor.txt PASTA_DOS_PRINTS
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` (o checkout do original e o gradle) com `mkdir`, espera em laço de `mkdir` a cada 5 s (nunca `pgrep`) e a solta ao
# sair. O gradle passa pelo `lim-build` (uma vaga de build, 4 núcleos). O jogo é mudo: o volume mestre vai a 0 (o `options.txt` volta ao fim).
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
PRINTS=$3
DONO=${DONO:-fase3-faltas}
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
  rm -f "$N/CenaLeitor.java" "$R/cena-leitor" "$R"/screenshots/leitor_oficial_*.png
  [ -e "$C/cena-cabo.antes-leitor" ] && mv "$C/cena-cabo.antes-leitor" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-leitor" ] && cp "$C/options.txt.antes-leitor" "$R/options.txt" && rm -f "$C/options.txt.antes-leitor"
  [ "${JA_COM_TRAVA:-}" = 1 ] || rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-leitor"
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-leitor"
rm -f "$R"/screenshots/leitor_oficial_*.png
cp "$JAVA" "$N/CenaLeitor.java"
cp "$CENA" "$R/cena-leitor"
JANELA="-W 1920 -H 1080 -w 1920 -h 1080"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
mkdir -p "$PRINTS"; cp "$R"/screenshots/leitor_oficial_*.png "$PRINTS"/ 2>/dev/null
ls "$PRINTS"
