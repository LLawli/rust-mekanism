#!/bin/bash
# Roda no desktop. Abre UM cliente do Horizon no container `horizon`, SEM janela (gamescope headless) e SEM som (volume mestre 0 no options.txt do cliente),
# para os testes que pedem dois clientes (decisão 14 do plano). Uso:
#   abrir_dois.sh anfitriao MUNDO            cliente A, mundo MUNDO, socket de controle padrão ($HORIZON_CONTROL); `ANF_EXTRA="--name Dev"` dá ao anfitrião o nome do do original
#   abrir_dois.sh convidado HOST:PORTA NOME  cliente B, entra no servidor HOST:PORTA com o nome NOME, socket /run/user/UID/horizon-control-b.sock,
#                                            arquivos do jogador em `data/horizon-b` (options.txt copiado do A, mestre 0)
# `DOIS_DIR` muda a pasta dos pid e dos logs. `H_NAME=horizon2` abre no container `horizon2` (o padrão é `horizon`), `FPS=30` muda a taxa do gamescope (padrão 15; 30 para gravar vídeo). O pid de cada um fica em ~/.cache/rust-mekanism/dois-clientes/{a,b}.pid e o log em {a,b}.log.
set -eu
PAPEL=$1
NAME=${H_NAME:-horizon}
D=$HOME/.local/share/jogo-isolado/$NAME/data/horizon
C=${DOIS_DIR:-$HOME/.cache/rust-mekanism/dois-clientes}
M=$HOME/Games/Horizon/mods
DEV="--dev-mod $M/conteudo/mekanism --dev-mod $M/conteudo/mekanismgenerators"
GS="gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r ${FPS:-15} --"
cd "$C"
case "$PAPEL" in
  anfitriao)
    sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$D/options.txt"
    ( JOGO_CMD="$GS env -u WAYLAND_DISPLAY jogar --world $2 --creative ${ANF_EXTRA:-} --fullscreen --width 1920 --height 1080 --control-socket \$HORIZON_CONTROL $DEV" nohup "$HOME/.local/bin/$NAME" offline > a.log 2>&1 & echo $! > a.pid ) ;;
  convidado)
    B=$HOME/.local/share/jogo-isolado/$NAME/data/horizon-b
    mkdir -p "$B"
    cp "$D/options.txt" "$B/options.txt"
    sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$B/options.txt"
    ( JOGO_CMD="$GS env -u WAYLAND_DISPLAY jogar --server $2 --name $3 --fullscreen --width 1920 --height 1080 --player-dir $B --control-socket /run/user/\$(id -u)/horizon-control-b.sock $DEV" nohup "$HOME/.local/bin/$NAME" offline > b.log 2>&1 & echo $! > b.pid ) ;;
esac
