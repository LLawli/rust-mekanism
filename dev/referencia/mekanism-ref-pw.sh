#!/bin/sh
# Roda no desktop. O `~/.local/bin/mekanism-ref` com o socket do PipeWire de gravação montado no container do original, como o `horizon` faz com `JOGO_PW_SOCK`
# (o gamescope de dentro publica a tela nesse PipeWire, e o `gravar-jogo rec` a grava). O script original não é tocado: este gera uma cópia dele com a linha do
# `--mount` a mais, depois da do cache, e a executa. Só vale quando é ele que cria o container (pare o `mekanism-ref` ocioso antes).
# Uso: JOGO_PW_SOCK=<socket> mekanism-ref-pw.sh offline   (com JOGO_CMD, como o original)
set -eu
orig=$HOME/.local/bin/mekanism-ref
copia=$HOME/.cache/rust-mekanism/mekanism-ref-pw.copia
sed 's|^\( *\)-v "\$cache:\$HOME/\.cache" \\$|&\n\1${JOGO_PW_SOCK:+--mount type=bind,src=$JOGO_PW_SOCK,dst=$rt/pipewire-0} \\|' "$orig" > "$copia"
grep -q 'JOGO_PW_SOCK:+--mount' "$copia" || { echo "mekanism-ref-pw: a linha do cache mudou no $orig" >&2; exit 1; }
chmod +x "$copia"
exec "$copia" "$@"
