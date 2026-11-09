#!/bin/bash
# Roda no notebook. Repete a gravação do port (`h2_som_plano.py`) em jogos novos: para cada rodada fecha o jogo (`quit`, que salva), espera o processo acabar (por PID, nunca por `pgrep`),
# reabre o mundo (`abrir_som_plano.sh MUNDO`), espera o jogador no mundo e grava. A primeira rodada (a que cria o mundo, `h2_mundo_plano.py`) é feita à parte.
# Uso: h2_som_plano_rodadas.sh CENA PREFIXO_LOCAL PREFIXO_DESKTOP MUNDO DE ATE   (grava PREFIXO_<i>.jsonl no notebook e PREFIXO_DESKTOP_<i>.wav no desktop, de i = DE até ATE)
set -u
D=$(dirname "$(readlink -f "$0")")
CENA=$1; LOCAL=$2; REMOTO=$3; MUNDO=$4; DE=$5; ATE=$6
export H_NAME=horizon
PASTA=.cache/rust-mekanism/som-plano/saida
for i in $(seq "$DE" "$ATE"); do
  echo "=== rodada $i $(date +%T)"
  "$D/ctl.sh" quit > /dev/null 2>&1
  PID=$(ssh desktop "cat $PASTA/game.pid")
  for _ in $(seq 60); do ssh desktop "kill -0 $PID 2>/dev/null" || break; sleep 2; done
  ssh desktop "kill -0 $PID 2>/dev/null" && { echo "o jogo não fechou"; exit 1; }
  ssh desktop "cd ~/.cache/rust-mekanism/som-plano && ./abrir_som_plano.sh $MUNDO"
  for _ in $(seq 90); do "$D/ctl.sh" state 2>/dev/null | grep -q '"world":"'"$MUNDO"'"' && break; sleep 2; done
  "$D/ctl.sh" wait-ready > /dev/null 2>&1
  sleep 5
  python3 -I "$D/h2_som_plano.py" "$CENA" "${LOCAL}_$i.jsonl" "${REMOTO}_$i.wav"
done
echo "fim $(date +%T)"
