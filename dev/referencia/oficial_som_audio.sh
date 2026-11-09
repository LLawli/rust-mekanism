#!/bin/bash
# Roda no desktop. Como `oficial_som.sh`, mas grava em WAV o áudio do `mekanism-ref` enquanto a cena roda, e o grava só pelo sink nulo: nada toca nas caixas do usuário.
# Ordem (a primeira versão deste script nunca achou o stream, e o som do original foi para as caixas; ver `audio_nulo.sh`):
#   1. o jogo sobe com o volume mestre em 0 (`oficial_som.sh` com AUDIO=1 cria `som-audio-espera`, e a cena espera `som-audio-ok`);
#   2. este script cria o sink nulo, manda o stream `java` do container para ele e confere (`audio_nulo.sh verificar`: todo link de saída vai para `som_nulo`);
#   3. com o gravador de pé, cria `som-audio-ok` e a cena sobe o volume mestre a 1;
#   4. enquanto o jogo roda, confere a cada segundo; se algum stream sair do sink nulo, cria `som-audio-mudo` (a cena volta o volume a 0 na hora) e o move de novo.
# Uso: oficial_som_audio.sh CenaSomMedida.java cenas/som_audio.txt SAIDA.jsonl SAIDA.wav
set -u
D=$(dirname "$(readlink -f "$0")")
JAVA=$1; CENA=$2; SAIDA=$3; WAV=$4
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
CONT=mekanism-ref
rm -f "$WAV"
AUDIO=1 "$D/oficial_som.sh" "$JAVA" "$CENA" "$SAIDA" > "${SAIDA%.jsonl}.log" 2>&1 &
JOGO=$!
echo "$JOGO" > "${SAIDA%.jsonl}.pid"
REC=
limpar() { [ -n "$REC" ] && kill -TERM "$REC" 2>/dev/null; wait "$REC" 2>/dev/null; rm -f "$R"/som-audio-ok "$R"/som-audio-mudo "$R"/som-audio-espera; "$D/audio_nulo.sh" remover; }
trap limpar EXIT
# 1 e 2: o stream do jogo (existe desde que o jogo abre o dispositivo, com a cena ainda esperando) vai para o sink nulo.
ok=0
while kill -0 "$JOGO" 2>/dev/null; do
  if [ -e "$R/som-audio-espera" ] && "$D/audio_nulo.sh" ligar "$CONT" | grep -qv '^movidos 0'; then
    sleep 1
    if "$D/audio_nulo.sh" verificar "$CONT" | grep -q '^ok'; then ok=1; break; fi
  fi
  sleep 1
done
[ "$ok" = 1 ] || { echo "o jogo acabou antes do stream"; exit 1; }
echo "stream no sink nulo: $("$D/audio_nulo.sh" verificar "$CONT")"
"$D/audio_nulo.sh" gravar "$WAV" > "${WAV%.wav}.rec.log" 2>&1 &
REC=$!
sleep 2
touch "$R/som-audio-ok"
echo "liberado $(date +%T)"
# 4: o vigia.
while kill -0 "$JOGO" 2>/dev/null; do
  "$D/audio_nulo.sh" ligar "$CONT" > /dev/null
  if "$D/audio_nulo.sh" verificar "$CONT" | grep -q '^ok'; then rm -f "$R/som-audio-mudo"; else touch "$R/som-audio-mudo"; echo "stream fora do sink nulo $(date +%T)"; fi
  sleep 1
done
echo "gravado $WAV"
