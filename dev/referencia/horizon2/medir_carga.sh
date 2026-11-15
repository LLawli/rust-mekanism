#!/bin/bash
# Roda no notebook (fala com o desktop por ssh), com a vaga e a trava do container na mão. Mede o tempo de parede do `abrir_horizon.sh MUNDO` até o mundo responder (`time query gametime` pelo
# `horizon-ctl`), N vezes, e fecha o jogo a cada vez. Uso: H_NAME=horizon medir_carga.sh MUNDO N ROTULO. Escreve `ROTULO <segundos> ok=1` em `carga.txt` (na pasta local `SAIDA`, padrão `.`).
# A pasta `PASTA` do desktop (padrão `~/.cache/rust-mekanism/transportador-sobras`) tem o `abrir_horizon.sh` e guarda o `game.log`.
set -u
MUNDO=$1; N=$2; ROTULO=$3
export H_NAME=${H_NAME:-horizon}
AQUI=$(cd "$(dirname "$0")" && pwd)
PASTA=${PASTA:-'$HOME/.cache/rust-mekanism/transportador-sobras'}
SAIDA=${SAIDA:-.}
for i in $(seq 1 "$N"); do
  t0=$(date +%s%N)
  ssh desktop "PASTA=$PASTA H_NAME=$H_NAME $PASTA/abrir_horizon.sh $MUNDO" > "$SAIDA/abrir.log" 2>&1
  ok=
  for s in $(seq 1 600); do
    if "$AQUI/ctl.sh" command "time query gametime" 2>/dev/null | grep -q '"ok":true'; then ok=1; break; fi
    sleep 0.5
  done
  t1=$(date +%s%N)
  echo "$ROTULO $(( (t1 - t0) / 1000000 )) ms ok=$ok" >> "$SAIDA/carga.txt"
  "$AQUI/ctl.sh" quit >/dev/null 2>&1
  sleep 20
done
