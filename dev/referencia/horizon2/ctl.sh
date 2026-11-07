#!/bin/bash
# horizon-ctl do horizon2 (a versão em uso vem do instances.json dele). Roda no desktop via ssh; os argumentos chegam intactos.
args=$(printf '%q ' "$@")
exec ssh desktop "V=\$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' ~/.local/share/jogo-isolado/horizon2/data/horizon-launcher/instances.json | head -1); docker exec horizon2 ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/\$V/horizon-ctl --timeout 120 $args"
