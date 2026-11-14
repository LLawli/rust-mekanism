#!/bin/bash
# Roda no notebook. Fecha o jogo (`quit`, que salva), espera o processo acabar (por PID, nunca por `pgrep`) e reabre o mesmo mundo no mesmo container, com o socket do gravador e a mesma pasta de log
# (o `game.log` recomeça: as sondas do jogo reaberto saem nele). A vaga e a trava de container seguem com quem chamou: este script não pega nenhuma. Uso: reabrir_jogo.sh PASTA_NO_DESKTOP NOME_DO_GRAVADOR MUNDO
# (`H_NAME` escolhe o container; `PASTA_NO_DESKTOP` é a pasta de `abrir_horizon.sh`, a do `game.pid` e do `game.log`, relativa a ~/.cache/rust-mekanism).
set -u
D=$(dirname "$(readlink -f "$0")")
PASTA=.cache/rust-mekanism/$1; NOME=$2; MUNDO=$3
export H_NAME=${H_NAME:-horizon}
"$D/ctl.sh" quit > /dev/null 2>&1
PID=$(ssh desktop "cat $PASTA/game.pid")
for _ in $(seq 60); do ssh desktop "kill -0 $PID 2>/dev/null" || break; sleep 2; done
ssh desktop "kill -0 $PID 2>/dev/null" && { echo "o jogo não fechou"; exit 1; }
ssh desktop "cd ~/$PASTA && JOGO_PW_SOCK=\$HOME/.cache/rust-mekanism/gravador/run-$NOME/pipewire-0 H_NAME=$H_NAME FPS=30 PASTA=\$HOME/$PASTA bash \$HOME/.cache/rust-mekanism/cabo-forma/abrir_horizon.sh $MUNDO"
for _ in $(seq 90); do "$D/ctl.sh" state 2>/dev/null | grep -q '"world":"' && break; sleep 2; done
"$D/ctl.sh" wait-ready > /dev/null 2>&1
echo "reaberto $(date +%T)"
