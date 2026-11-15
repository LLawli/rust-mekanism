#!/bin/bash
# Roda um teste de rastro (`TESTE.java`) no original (`runGameTestServer`, sem janela e sem som) e devolve os `GLOB` de `trace/` em SAIDA. Pega a `mekanism-ref.lock` (laço de
# `mkdir` a cada 5 s, nunca `pgrep`), tira `InventoryNetworkTest` e `TransmitterNetworkTest` da pasta dos testes enquanto roda, passa o gradle pelo `lim-build` (uma vaga de
# build, 4 núcleos e 6 GB no container) e devolve a árvore como estava. Escreve `SAIDA.fim` (o código de saída) também quando falha. Roda no desktop.
# Uso: oficial_gametest_rastro.sh TESTE.java 'GLOB' SAIDA   (DONO=nome da frente)
set -u
JAVA=$(readlink -f "$1")
GLOB=$2
SAIDA=$3
NOME=$(basename "$JAVA")
DONO=${DONO:-frente}
F=$HOME/.local/share/jogo-isolado/mekanism-ref/src
N=$F/src/gameTest/java/mekanism/common/tests/network
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
FIM=$SAIDA.fim
rm -f "$FIM"
mkdir -p "$SAIDA"
trap 'echo $? > "$FIM"' EXIT
# `LOCK_MEU=1`: quem chama já pegou a trava (e a vaga) e a deixa para este script soltar; `SEM_LIM=1`: a vaga já é de quem chama (o `lim-jogo` a segura).
if [ "${LOCK_MEU:-}" != 1 ]; then until mkdir "$LOCK" 2>/dev/null; do sleep 5; done; fi
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  rm -f "$N/$NOME"
  for t in InventoryNetworkTest TransmitterNetworkTest; do [ -e "$C/$t.antes" ] && mv "$C/$t.antes" "$N/$t.java"; done
  rm -rf "$F/runs/gameTestServer/trace"
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap 'rc=$?; restore; echo $rc > "$FIM"' EXIT
for t in InventoryNetworkTest TransmitterNetworkTest; do [ -e "$N/$t.java" ] && mv "$N/$t.java" "$C/$t.antes"; done
rm -rf "$F/runs/gameTestServer/trace"
cp "$JAVA" "$N/$NOME"
LIM="$C/lim-build"; [ "${SEM_LIM:-}" = 1 ] && LIM=
$LIM sh -c 'JOGO_CMD=true "$HOME/.local/bin/mekanism-ref" && docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref && env JOGO_CMD="cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestServer" "$HOME/.local/bin/mekanism-ref"'
echo "EXIT $?"
cp "$F"/runs/gameTestServer/trace/$GLOB "$SAIDA"/ 2>/dev/null
ls -la "$SAIDA"
