#!/bin/bash
# Roda no desktop, com a `horizon.lock` (ou a `horizon2.lock`, com `H_NAME=horizon2`) na mão. Abre o `horizon` SEM janela (gamescope headless) e SEM som (volume mestre 0 no options.txt, que o chamador restaura), para o teste do som em condições controladas.
# Uso: [LARGURA=1920 ALTURA=1080 FPS=15] abrir_som_plano.sh titulo|MUNDO [PASTA]   (`titulo`: sobe na tela de título, sem abrir mundo, para `h2_mundo_plano.py` criar o mundo; MUNDO: reabre o mundo que já existe)
# O pid do processo `horizon` fica em PASTA/game.pid e o stderr em PASTA/game.log (PASTA padrão ~/.cache/rust-mekanism/som-plano/saida).
set -eu
MUNDO=$1
NAME=${H_NAME:-horizon}
D=$HOME/.local/share/jogo-isolado/$NAME/data/horizon
C=${2:-$HOME/.cache/rust-mekanism/som-plano/saida}
M=$HOME/Games/Horizon/mods
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$D/options.txt"
if [ "$MUNDO" = titulo ]; then ONDE="--quickPlaySingleplayer mundo-que-nao-existe"; else ONDE="--world $MUNDO --creative"; fi
cd "$C"
( JOGO_CMD="gamescope --backend headless -W ${LARGURA:-1920} -H ${ALTURA:-1080} -w ${LARGURA:-1920} -h ${ALTURA:-1080} -r ${FPS:-15} -- env -u WAYLAND_DISPLAY jogar $ONDE --fullscreen --width ${LARGURA:-1920} --height ${ALTURA:-1080} --control-socket \$HORIZON_CONTROL --dev-mod $M/conteudo/mekanism --dev-mod $M/conteudo/mekanismgenerators" nohup "$HOME/.local/bin/$NAME" offline > game.log 2>&1 & echo $! > game.pid )
