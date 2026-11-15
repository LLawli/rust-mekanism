#!/bin/bash
# Roda no desktop. Abre o jogo no container `horizon` SEM janela (gamescope headless) e SEM som (o volume mestre vai a 0 no options.txt, que o chamador restaura), com o mundo MUNDO
# e o conteúdo dos dois mods (`--dev-mod`). Uso: [H_NAME=horizon2] abrir_horizon.sh MUNDO (`H_NAME` escolhe o container, o script e a pasta de mods dele). O pid do processo `horizon` fica em `game.pid`, e o stderr em `game.log`, na pasta `PASTA` (padrão ~/.cache/rust-mekanism/restos).
set -eu
MUNDO=$1
NAME=${H_NAME:-horizon}
D=$HOME/.local/share/jogo-isolado/$NAME/data/horizon
C=${PASTA:-$HOME/.cache/rust-mekanism/restos}
M=$HOME/Games/Horizon/mods  # dentro do container o horizon2 vê a pasta dele neste caminho
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$D/options.txt"
cd "$C"
( JOGO_CMD="gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r ${FPS:-15} -- env -u WAYLAND_DISPLAY jogar --world $MUNDO --creative ${JOGAR_EXTRA:-} --fullscreen --width 1920 --height 1080 --control-socket \$HORIZON_CONTROL --dev-mod $M/conteudo/mekanism --dev-mod $M/conteudo/mekanismgenerators" nohup "$HOME/.local/bin/$NAME" offline > game.log 2>&1 & echo $! > game.pid )
