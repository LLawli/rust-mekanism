#!/bin/bash
# Roda o `runGameTestServer` do original (sem janela, sem som) com arquivos de configuração de servidor escritos de propósito errados e devolve os arquivos
# como o NeoForge os corrigiu e escreveu de novo: é a referência dos testes de leitura e correção (`server_config.rs`). Os arquivos de `runs/gameTestServer/config/Mekanism/`
# voltam ao que eram. Roda no desktop. Uso: oficial_config.sh PASTA_ENTRADA PASTA_SAIDA   (os `*.toml` da entrada trocam os de mesmo nome)
# Pega `~/.cache/rust-mekanism/mekanism-ref.lock` (o checkout do original e o gradle) com `mkdir`, espera em laço de `test -d` a cada 60 s (nunca `pgrep`) e a
# solta ao sair. O gradle passa pelo `lim-build` (1 build por vez, 4 núcleos).
set -u
ENTRADA=$(readlink -f "$1")
SAIDA=$(readlink -m "$2")
DONO=${DONO:-config}
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/gameTestServer
D=$R/config/Mekanism
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
BACKUP=$C/config-antes-$DONO
until mkdir "$LOCK" 2>/dev/null; do sleep 60; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
restore() {
  if [ -d "$BACKUP" ]; then
    for f in "$BACKUP"/*; do [ -e "$f" ] && cp "$f" "$D/$(basename "$f")"; done
    rm -rf "$BACKUP"
  fi
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap restore EXIT
# O container do dono anterior pode ainda estar de pé, ocioso (o `idle-watch` o fecha 5 minutos depois do último processo): só conta como ocupado se há outro
# processo dentro dele além do `docker-init`, do `idle-watch` e do `sleep`. Espera até 10 minutos, de 20 em 20 s, e desiste se continuar (nunca o mata).
ocupado() { docker ps --format '{{.Names}}' | grep -qx mekanism-ref && docker top mekanism-ref -o args | tail -n +2 | grep -vE 'docker-init|idle-watch|^sleep ' | grep -q .; }
for i in $(seq 30); do ocupado || break; sleep 20; done
if ocupado; then echo OCUPADO; exit 3; fi
mkdir -p "$BACKUP" "$SAIDA"
for f in "$ENTRADA"/*.toml; do
  n=$(basename "$f")
  cp "$D/$n" "$BACKUP/$n"
  cp "$f" "$D/$n"
done
"$C/lim-build" env JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon --max-workers=4 runGameTestServer' "$HOME/.local/bin/mekanism-ref"
echo "EXIT $?"
for f in "$ENTRADA"/*.toml; do
  n=$(basename "$f" .toml)
  cp "$D/$n.toml" "$SAIDA/"
  # O NeoForge guarda o arquivo de antes da correção em `<nome>-1.toml.bak` (até 5, `ConfigFileTypeHandler.backUpConfig`): o conteúdo dele é o da entrada, e ele sai com a execução.
  cp "$D/$n-1.toml.bak" "$SAIDA/$n-1.toml.bak" 2>/dev/null || true
  rm -f "$D/$n"-[0-9]*.toml.bak
done
