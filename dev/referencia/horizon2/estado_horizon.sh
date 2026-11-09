#!/bin/bash
# Roda no desktop, com a `horizon.lock` na mão. Guarda (`antes`) e devolve (`depois`) o que um teste em jogo mexe no `horizon`: o `options.txt`, o `horizon-graphics.txt`, o `horizon-player.txt`,
# o `config/mekanism/client.toml`, e o mundo de teste, que é uma cópia do mundo MODELO (`antes`) apagada no `depois`. Um arquivo que não existia antes some no `depois`.
# Uso: estado_horizon.sh antes|depois PASTA MUNDO [MODELO]   (PASTA: onde ficam as cópias; MUNDO: o nome do mundo de teste; MODELO: o mundo copiado, `spike` por padrão)
set -eu
D=$HOME/.local/share/jogo-isolado/horizon/data/horizon
ARQ="options.txt horizon-graphics.txt horizon-player.txt config/mekanism/client.toml"
P=$2/backup; MUNDO=$3; MODELO=${4:-spike}
case "$1" in
  antes)
    mkdir -p "$P/config/mekanism"
    for a in $ARQ; do if [ -e "$D/$a" ]; then cp -p "$D/$a" "$P/$a"; else rm -f "$P/$a"; echo "ausente $a" >> "$P/ausentes.txt"; fi; done
    [ -e "$D/saves/$MUNDO" ] && { echo "o mundo $MUNDO já existe"; exit 1; }
    cp -a --reflink=always "$D/saves/$MODELO" "$D/saves/$MUNDO"
    echo "guardado"
    ;;
  depois)
    for a in $ARQ; do if [ -e "$P/$a" ]; then cp -p "$P/$a" "$D/$a"; else rm -f "$D/$a"; fi; done
    rm -rf "$D/saves/$MUNDO"
    echo "devolvido"
    ;;
esac
