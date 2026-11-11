#!/bin/bash
# Roda no desktop. Tira uma rajada de screenshots do container (`NOME`, horizon ou horizon2) o mais rápido que o `horizon-ctl` der, DENTRO do container (sem o ssh entre um quadro e
# outro), e mede no anfitrião o instante do primeiro e do último (`PREFIXO_t.txt`: duas linhas, em segundos) para a montagem do vídeo no ritmo medido (quadros por segundo = N / (fim - começo)). O `date` do container não tem `%N`.
# Uso: rajada.sh NOME PASTA_NO_CONTAINER PREFIXO N     (PASTA_NO_CONTAINER: `/home/luka/.local/share/jogo-isolado/horizon/data/shots/...`)
set -u
N=$1; D=$2; P=$3; COUNT=$4
V=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ~/.local/share/jogo-isolado/$N/data/horizon-launcher/instances.json | head -1)
CTL=/home/luka/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/$V/horizon-ctl
HD=$HOME/.local/share/jogo-isolado/$N/data/shots/${D##*/shots/}
T=$HD/${P}_t.txt
docker exec "$N" sh -c "mkdir -p $D; rm -f $D/${P}_*.png"
rm -f "$T"; date +%s.%N >> "$T"
docker exec "$N" sh -c "i=0; while [ \$i -lt $COUNT ]; do $CTL screenshot $D/${P}_\$(printf %04d \$i).png >/dev/null 2>&1; i=\$((i+1)); done"
date +%s.%N >> "$T"
