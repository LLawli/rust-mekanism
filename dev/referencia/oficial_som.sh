#!/bin/bash
# Roda a `CenaSomMedida` no cliente oficial (`runGameTestClient`, num gamescope headless: nenhuma janela abre no desktop) com uma lista de cenários e
# devolve a árvore como estava: a `CenaSomMedida.java` e o `cena-som-medida` saem, o `cena-cabo` e o `options.txt` voltam. Roda no desktop.
# Uso: oficial_som.sh CenaSomMedida.java cenas/som_medida.txt SAIDA.jsonl
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` (o checkout do original e o gradle) com `mkdir`, espera em laço de `test -d` a cada 60 s (nunca
# `pgrep`), e a solta ao sair. O gradle passa pelo `lim-build` (1 build por vez, 4 núcleos).
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
SAIDA=$3
DONO=${DONO:-som}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
until mkdir "$LOCK" 2>/dev/null; do sleep 60; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  rm -f "$N/CenaSomMedida.java" "$R/cena-som-medida" "$R/som-medido.jsonl"
  [ -e "$C/cena-cabo.antes-som" ] && mv "$C/cena-cabo.antes-som" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-som" ] && cp "$C/options.txt.antes-som" "$R/options.txt" && rm -f "$C/options.txt.antes-som"
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
# O container do dono anterior pode ainda estar de pé, ocioso (o `idle-watch` o fecha 5 minutos depois do último processo): só conta como ocupado se há outro
# processo dentro dele além do `docker-init`, do `idle-watch` e do `sleep`. Espera até 10 minutos, de 20 em 20 s, e desiste se continuar (nunca o mata).
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
cp "$R/options.txt" "$C/options.txt.antes-som"
# Regra: o jogo de teste nunca toca nas caixas do usuário. Esta cena mede o estado do motor de som, não o áudio: o volume mestre vai a 0 (o `options.txt` volta ao fim).
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-som"
rm -f "$R/som-medido.jsonl"
cp "$JAVA" "$N/CenaSomMedida.java"
cp "$CENA" "$R/cena-som-medida"
LARGURA=${LARGURA:-1920}; ALTURA=${ALTURA:-1080}; JANELA="-W $LARGURA -H $ALTURA -w $LARGURA -h $ALTURA"
"$C/lim-build" env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
[ -s "$R/som-medido.jsonl" ] && cp "$R/som-medido.jsonl" "$SAIDA"
