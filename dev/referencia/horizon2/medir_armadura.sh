#!/bin/sh
# Roda DENTRO do container `horizon` (docker exec horizon sh medir_armadura.sh ALVO MODO N): mede, em milissegundos, o tempo entre um `item replace ... armor.head` (comando do
# console do anfitrião, como "outro agente") e o slot 500 (cabeça) da tela do Heat Generator aberta no ALVO (`host` ou `guest`) mostrar o capacete. A tela já tem de estar aberta no
# ALVO. MODO: `corrido` (servidor andando) ou `parado` (`tick freeze` antes). Uma linha por rodada: `ms`.
set -u
ALVO=$1; MODO=$2; N=$3
V=$XDG_DATA_HOME/horizon-launcher/versions/$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' $XDG_DATA_HOME/horizon-launcher/instances.json | head -1)
H="$V/horizon-ctl --timeout 20"
G="$V/horizon-ctl --timeout 20 --socket /run/user/$(id -u)/horizon-control-b.sock"
if [ "$ALVO" = guest ]; then T="$G"; NOME=Convidado; else T="$H"; NOME=Player90366; fi
[ "$MODO" = parado ] && $H command "tick freeze" >/dev/null
i=0
while [ $i -lt "$N" ]; do
  $H command "item replace entity $NOME armor.head with minecraft:air" >/dev/null
  while $T screen | grep -q '"item":"minecraft:iron_helmet","slot":500'; do :; done
  sleep 0.5
  t0=$(date +%s%N)
  $H command "item replace entity $NOME armor.head with minecraft:iron_helmet" >/dev/null
  t1=$(date +%s%N)
  while ! $T screen | grep -q '"item":"minecraft:iron_helmet","slot":500'; do :; done
  t2=$(date +%s%N)
  echo "$(( (t2 - t0) / 1000000 )) $(( (t1 - t0) / 1000000 ))"
  i=$((i + 1))
done
[ "$MODO" = parado ] && $H command "tick unfreeze" >/dev/null
exit 0
