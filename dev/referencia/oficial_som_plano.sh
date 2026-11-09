#!/bin/bash
# Roda no desktop. Grava o áudio da Enrichment Chamber ligada no cliente oficial, em condições controladas, N vezes: cada vez é um jogo novo (um `runGameTestClient`) que cria o mundo
# na primeira e o reabre nas outras. A cena é a `testes/CenaSomPlano.java` (mundo superplano novo com a semente da cena, máquina por `fill`, jogador por `tp`, só a categoria `block`
# ligada). O áudio vai SÓ para o sink nulo (`audio_nulo.sh`): o jogo sobe com o volume mestre em 0, o stream `java` é movido para o sink e conferido, o gravador sobe e só então a cena
# recebe `som-audio-ok` (volume mestre 1); um vigia confere a cada segundo e cria `som-audio-mudo` se algum stream sair do sink nulo.
#
# Uso: oficial_som_plano.sh CenaSomPlano.java cenas/som_plano.txt PREFIXO [N]   (gera PREFIXO_1.jsonl, PREFIXO_1.wav, PREFIXO_1.stream.txt, ... e PREFIXO.log)
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` com `mkdir` (espera em laço de `test -d` a cada 60 s, nunca `pgrep`) e a solta ao sair, devolvendo a árvore como estava: tira a cena, o
# `options.txt` volta, o mundo novo é apagado e o `GameTestWorld` antigo volta ao lugar. O gradle passa pelo `lim-build` (1 build por vez, 4 núcleos).
set -u
D=$(dirname "$(readlink -f "$0")")
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
PREFIXO=$3
N=${4:-3}
DONO=${DONO:-som-plano}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
NT=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
MUNDO=$R/saves/GameTestWorld
ANTES=$R/saves/GameTestWorld.antes-som-plano
CONT=mekanism-ref
AN=$D/audio_nulo.sh
REC=
JOGO=
until mkdir "$LOCK" 2>/dev/null; do sleep 60; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  [ -n "$REC" ] && kill -TERM "$REC" 2>/dev/null
  rm -f "$NT/CenaSomPlano.java" "$R/cena-som-plano" "$R/som-plano.jsonl" "$R/som-audio-espera" "$R/som-audio-ok" "$R/som-audio-mudo"
  [ -e "$C/cena-cabo.antes-som" ] && mv "$C/cena-cabo.antes-som" "$R/cena-cabo"
  [ -e "$C/options.txt.antes-som-plano" ] && cp "$C/options.txt.antes-som-plano" "$R/options.txt" && rm -f "$C/options.txt.antes-som-plano"
  if [ -e "$ANTES" ]; then rm -rf "$MUNDO"; mv "$ANTES" "$MUNDO"; fi
  "$AN" remover
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
# O container do dono anterior pode ainda estar de pé, ocioso: só conta como ocupado se há outro processo além do `docker-init`, do `idle-watch` e do `sleep`.
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
[ -e "$ANTES" ] && { echo "sobrou $ANTES de uma rodada anterior: confira antes de seguir"; exit 4; }
cp "$R/options.txt" "$C/options.txt.antes-som-plano"
# O mesmo áudio de cliente nos dois jogos: dispositivo padrão, áudio direcional desligado; o mestre vai a 0 até o sink nulo estar conferido.
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/; s/^directionalAudio:.*/directionalAudio:false/; s/^soundDevice:.*/soundDevice:""/' "$R/options.txt"
grep -E '^(soundDevice|directionalAudio|soundCategory_master|guiScale):' "$R/options.txt" > "$PREFIXO.options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$C/cena-cabo.antes-som"
[ -e "$MUNDO" ] && mv "$MUNDO" "$ANTES"
cp "$JAVA" "$NT/CenaSomPlano.java"
cp "$CENA" "$R/cena-som-plano"
LARGURA=1920; ALTURA=1080; JANELA="-W $LARGURA -H $ALTURA -w $LARGURA -h $ALTURA"
for i in $(seq "$N"); do
  S=${PREFIXO}_$i
  echo "=== rodada $i de $N $(date +%T)"
  rm -f "$R/som-plano.jsonl" "$R/som-audio-ok" "$R/som-audio-mudo" "$S.wav" "$S.pid"
  touch "$R/som-audio-espera"
  "$C/lim-build" env JOGO_CMD='gamescope --backend headless '"$JANELA"' -r 15 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' "$HOME/.local/bin/mekanism-ref" > "$S.gradle.log" 2>&1 &
  JOGO=$!
  # O stream do jogo (existe desde que ele abre o dispositivo, com a cena ainda esperando) vai para o sink nulo e é conferido.
  ok=0
  while kill -0 "$JOGO" 2>/dev/null; do
    if [ -s "$R/som-plano.jsonl" ] || [ -e "$R/som-audio-espera" ]; then
      if "$AN" ligar "$CONT" | grep -qv '^movidos 0'; then
        sleep 1
        if "$AN" verificar "$CONT" | grep -q '^ok'; then ok=1; break; fi
      fi
    fi
    sleep 1
  done
  if [ "$ok" != 1 ]; then echo "o jogo acabou antes do stream"; kill -0 "$JOGO" 2>/dev/null && kill -TERM "$JOGO"; exit 1; fi
  echo "stream no sink nulo: $("$AN" verificar "$CONT")"
  "$AN" info "$CONT" > "$S.stream.txt"
  "$AN" gravar "$S.wav" "$S.pid" > "$S.rec.log" 2>&1 &
  REC=$!
  sleep 2
  touch "$R/som-audio-ok"
  echo "liberado $(date +%T)"
  while kill -0 "$JOGO" 2>/dev/null; do
    "$AN" ligar "$CONT" > /dev/null
    if "$AN" verificar "$CONT" | grep -q '^ok'; then rm -f "$R/som-audio-mudo"; else touch "$R/som-audio-mudo"; echo "stream fora do sink nulo $(date +%T)"; fi
    sleep 1
  done
  wait "$JOGO"; echo "EXIT $?"
  kill -TERM "$REC" 2>/dev/null; wait "$REC" 2>/dev/null; REC=
  [ -s "$R/som-plano.jsonl" ] && cp "$R/som-plano.jsonl" "$S.jsonl"
  rm -f "$S.pid"
  echo "gravado $S.wav $(stat -c %s "$S.wav" 2>/dev/null) bytes; jsonl $(wc -l < "$S.jsonl" 2>/dev/null) linhas"
done
