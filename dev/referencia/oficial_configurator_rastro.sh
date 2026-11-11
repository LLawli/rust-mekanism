#!/bin/bash
# Roda a `ConfiguratorTraceTest` no original (`runGameTestServer`, sem janela e sem som) e devolve os `configurator_*.jsonl` em SAIDA. Pega a `mekanism-ref.lock` (laço de
# `mkdir` a cada 5 s, nunca `pgrep`), tira `InventoryNetworkTest` e `TransmitterNetworkTest` da pasta dos testes enquanto roda, passa o gradle pelo `lim-build` (uma vaga de
# build, 4 núcleos) e devolve a árvore como estava. Roda no desktop. Uso: oficial_configurator_rastro.sh ConfiguratorTraceTest.java SAIDA
set -u
JAVA=$(readlink -f "$1")
SAIDA=$2
DONO=${DONO:-efeitos-configurator}
F=$HOME/.local/share/jogo-isolado/mekanism-ref/src
N=$F/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
if [ "${JA_COM_TRAVA:-}" != 1 ]; then
  until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
  echo "$DONO" > "$LOCK/dono"
  echo "TRAVA $(date +%T)"
fi
restore() {
  rm -f "$N/ConfiguratorTraceTest.java"
  for t in InventoryNetworkTest TransmitterNetworkTest; do [ -e "$C/$t.antes" ] && mv "$C/$t.antes" "$N/$t.java"; done
  rm -rf "$F/runs/gameTestServer/trace"
  [ "${JA_COM_TRAVA:-}" = 1 ] || rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
for t in InventoryNetworkTest TransmitterNetworkTest; do [ -e "$N/$t.java" ] && mv "$N/$t.java" "$C/$t.antes"; done
rm -rf "$F/runs/gameTestServer/trace"
cp "$JAVA" "$N/ConfiguratorTraceTest.java"
"$C/lim-build" env JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon --max-workers=4 runGameTestServer' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
mkdir -p "$SAIDA"
cp "$F"/runs/gameTestServer/trace/configurator_*.jsonl "$SAIDA"/ 2>/dev/null
ls -la "$SAIDA"
