#!/bin/bash
# Roda no desktop. Pega a trava de container que estiver livre (`horizon.lock` ou `horizon2.lock`, laço de `mkdir` a cada 2 s; nunca `pgrep`), guarda os fontes que estão em
# `Games/Horizon[2]/mods`, põe os de `PASTA/fontes.tar` (veja `enviar_fontes.sh`), faz o `native-build` pelo `lim-build` (uma vaga, 4 núcleos, o container com 6 GB:
# `docker update --cpus 4 --memory 6g --memory-swap 6g`), amostra a memória do container em `PASTA/mem.log` e apaga os builds de teste anteriores (fica só o atual).
# A trava FICA com quem chamou: `PASTA/ambiente` diz qual é; solte-a com `rm -rf` ao fim do teste. `PASTA/build.fim` aparece quando acaba.
# Uso: construir_efeitos.sh PASTA   (PASTA em ~/.cache/rust-mekanism)
set -u
P=$HOME/.cache/rust-mekanism/$1
C=$HOME/.cache/rust-mekanism
DONO=${DONO:-efeitos-configurator}
rm -f "$P/build.fim" "$P/mem.log"
# REBUILD=1: a trava já é de quem chama (um segundo build no mesmo teste): só põe os fontes novos e compila.
if [ "${REBUILD:-}" = 1 ]; then
  N=$(cat "$P/ambiente")
else
  while :; do
    for N in horizon horizon2; do
      if mkdir "$C/$N.lock" 2>/dev/null; then echo "$DONO" > "$C/$N.lock/dono"; echo "$N" > "$P/ambiente"; break 2; fi
    done
    sleep 2
  done
  echo "TRAVA $N $(date +%T)"
fi
if [ "$N" = horizon ]; then M=$HOME/Games/Horizon/mods; else M=$HOME/Games/Horizon2/mods; fi
V=$HOME/.local/share/jogo-isolado/$N/data/horizon-launcher/versions
ATUAL=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
if [ "${REBUILD:-}" != 1 ]; then
  echo "$ATUAL" > "$P/versao-antes.txt"
  tar -C "$M" -cf "$P/mods-antes.tar" infuser conteudo
fi
rm -rf "$M/infuser" "$M/conteudo"
tar -C "$M" -xf "$P/fontes.tar"
JOGO_CMD=true "$HOME/.local/bin/$N" offline
docker update --cpus 4 --memory 6g --memory-swap 6g "$N"
( while [ ! -e "$P/build.fim" ]; do docker stats --no-stream --format '{{.MemUsage}}' "$N" >> "$P/mem.log" 2>/dev/null; sleep 3; done ) &
"$C/lim-build" env JOGO_CMD="horizon-launcher native-build ~/Games/Horizon/mods/infuser --instance horizon --profile dev --trust-native-code" "$HOME/.local/bin/$N" offline > "$P/build.log" 2>&1
echo "EXIT $?" >> "$P/build.log"
NOVO=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
echo "$NOVO" > "$P/versao-nova.txt"
for d in "$V"/native-*; do b=$(basename "$d"); [ "$b" = "$NOVO" ] || { echo "apaga $b" >> "$P/build.log"; rm -rf "$d"; }; done
touch "$P/build.fim"
