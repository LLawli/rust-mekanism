#!/bin/bash
# Roda no desktop. Pega `~/.cache/rust-mekanism/horizon.lock` (laço de `mkdir` a cada 2 s, para não perder a corrida para quem sonda mais rápido; nunca `pgrep`), guarda os fontes que estão em ~/Games/Horizon/mods,
# põe os de `fontes.tar` (veja `enviar_fontes.sh`) e faz o `native-build` do container `horizon`, com 4 núcleos e 6 GB, apagando o build de teste anterior. A trava FICA
# com quem chamou: solte-a com `rm -rf ~/.cache/rust-mekanism/horizon.lock` ao fim do teste. Uso: construir_horizon.sh PASTA   (PASTA em ~/.cache/rust-mekanism)
set -eu
P=$HOME/.cache/rust-mekanism/$1
L=$HOME/.cache/rust-mekanism/horizon.lock
M=$HOME/Games/Horizon/mods
V=$HOME/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions
# REBUILD=1: a trava já é de quem chama (um segundo build no mesmo teste): só põe os fontes novos e compila.
if [ "${REBUILD:-}" != 1 ]; then
  until mkdir "$L" 2>/dev/null; do sleep 2; done
  echo "${DONO:-som}" > "$L/dono"
  echo "TRAVA $(date +%T)"
  ATUAL=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
  echo "$ATUAL" > "$P/versao-antes.txt"
  tar -C "$M" -cf "$P/mods-antes.tar" infuser conteudo
fi
rm -rf "$M/infuser" "$M/conteudo"
tar -C "$M" -xf "$P/fontes.tar"
JOGO_CMD=true "$HOME/.local/bin/horizon" offline
docker update --cpus 4 --memory 6g --memory-swap 6g horizon
"$HOME/.cache/rust-mekanism/lim-build" env JOGO_CMD="horizon-launcher native-build ~/Games/Horizon/mods/infuser --instance horizon --profile dev --trust-native-code" "$HOME/.local/bin/horizon" offline
echo "EXIT $?"
NOVO=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
echo "novo $NOVO"
# Só o build atual fica.
for d in "$V"/native-*; do b=$(basename "$d"); [ "$b" = "$NOVO" ] || { echo "apaga $b"; rm -rf "$d"; }; done
