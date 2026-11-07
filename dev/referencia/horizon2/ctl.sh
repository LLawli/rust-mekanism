#!/bin/bash
# horizon-ctl de um dos containers de teste (a versão em uso vem do instances.json dele). Roda no desktop via ssh; os argumentos chegam intactos.
# O container é o horizon2 por padrão; `H_NAME=horizon` aponta para o `horizon`, o único que a regra de builds e jogo deixa subir.
name=${H_NAME:-horizon2}
args=$(printf '%q ' "$@")
exec ssh desktop "V=\$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' ~/.local/share/jogo-isolado/$name/data/horizon-launcher/instances.json | head -1); docker exec $name ~/.local/share/jogo-isolado/$name/data/horizon-launcher/versions/\$V/horizon-ctl --timeout 120 $args"
