#!/bin/bash
# (Dentro do container o caminho é sempre o do `horizon`: o `horizon2` monta a pasta dele ali.)
# horizon-ctl de um dos containers de teste (a versão em uso vem do instances.json dele). Roda no desktop via ssh; os argumentos chegam intactos.
# O container é o horizon2 por padrão; `H_NAME=horizon` aponta para o `horizon`, o único que a regra de builds e jogo deixa subir.
# `H_SOCKET=/run/user/1000/horizon-control-b.sock` fala com o segundo cliente do mesmo container (o teste com dois clientes); sem ele, com o primeiro.
name=${H_NAME:-horizon2}
sock=${H_SOCKET:+--socket $H_SOCKET}
args=$(printf '%q ' "$@")
exec ssh desktop "V=\$(sed -n 's/.*\"version\": *\"\\([^\"]*\\)\".*/\\1/p' ~/.local/share/jogo-isolado/$name/data/horizon-launcher/instances.json | head -1); docker exec $name ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/\$V/horizon-ctl $sock --timeout 120 $args"
