#!/bin/bash
# Roda no desktop. Como `oficial_som.sh`, mas tira o som do container `mekanism-ref` das caixas do usuário (sink nulo) e o grava em WAV enquanto a cena roda.
# Uso: oficial_som_audio.sh CenaSomMedida.java cenas/som_audio.txt SAIDA.jsonl SAIDA.wav
set -u
D=$(dirname "$(readlink -f "$0")")
JAVA=$1; CENA=$2; SAIDA=$3; WAV=$4
"$D/oficial_som.sh" "$JAVA" "$CENA" "$SAIDA" > "${SAIDA%.jsonl}.log" 2>&1 &
JOGO=$!
REC=
until kill -0 "$JOGO" 2>/dev/null && "$D/audio_nulo.sh" ligar mekanism-ref | grep -qv '^movidos 0'; do
  kill -0 "$JOGO" 2>/dev/null || { echo "o jogo acabou antes do stream"; exit 1; }
  sleep 1
done
"$D/audio_nulo.sh" gravar "$WAV" &
REC=$!
# O stream pode ser refeito: confere de 2 em 2 s até o jogo acabar.
while kill -0 "$JOGO" 2>/dev/null; do "$D/audio_nulo.sh" ligar mekanism-ref >/dev/null; sleep 2; done
kill -INT "$REC" 2>/dev/null; wait "$REC" 2>/dev/null
echo "gravado $WAV"
