#!/bin/bash
# Roda no desktop. Abre o jogo no container `horizon` SEM janela (gamescope headless) e SEM som (o volume mestre vai a 0 no options.txt, que o chamador restaura), com o mundo MUNDO
# e o conteúdo dos dois mods (`--dev-mod`). Uso: abrir_horizon.sh MUNDO. O pid do processo `horizon` fica em ~/.cache/rust-mekanism/restos/game.pid.
set -eu
MUNDO=$1
D=$HOME/.local/share/jogo-isolado/horizon/data/horizon
C=$HOME/.cache/rust-mekanism/restos
M=$HOME/Games/Horizon/mods
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$D/options.txt"
cd "$C"
( JOGO_CMD="gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r 15 -- env -u WAYLAND_DISPLAY jogar --world $MUNDO --creative --fullscreen --width 1920 --height 1080 --control-socket \$HORIZON_CONTROL --dev-mod $M/conteudo/mekanism --dev-mod $M/conteudo/mekanismgenerators" nohup "$HOME/.local/bin/horizon" offline > game.log 2>&1 & echo $! > game.pid )
