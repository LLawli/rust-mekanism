#!/bin/bash
# Roda no desktop. Pega `~/.cache/rust-mekanism/horizon.lock` (laço de `mkdir` a cada 2 s, para não perder a corrida para quem sonda mais rápido; nunca `pgrep`), guarda os fontes que estão em ~/Games/Horizon/mods,
# põe os de `fontes.tar` (veja `enviar_fontes.sh`) e faz o `native-build` do container `horizon`, com 4 núcleos e 6 GB, apagando o build de teste anterior. A trava FICA
# com quem chamou: solte-a com `rm -rf ~/.cache/rust-mekanism/horizon.lock` ao fim do teste. Uso: [H_NAME=horizon2] construir_horizon.sh PASTA   (PASTA em ~/.cache/rust-mekanism;
# `H_NAME` escolhe o container, a trava, a pasta de mods e a de versões dele; `H_NAME=auto` pega a trava que estiver livre, a do `horizon2` primeiro, e grava o nome em `PASTA/container`). O `fim`
# da PASTA ganha o código de saída quando o script acaba, falhando ou não. `JOBS` dá os jobs do cargo dentro do container, 2 por padrão: com 4 o build passa de 6 GB e morre (`OOMKilled`, três frentes em 2026-11-14).
set -eu
NAME=${H_NAME:-horizon}
P=$HOME/.cache/rust-mekanism/$1
rm -f "$P/fim"
trap 'echo $? > "$P/fim"' EXIT
# REBUILD=1: a trava já é de quem chama (um segundo build no mesmo teste): só põe os fontes novos e compila.
if [ "${REBUILD:-}" != 1 ]; then
  if [ "$NAME" = auto ]; then
    NAME=
    while [ -z "$NAME" ]; do
      for candidato in horizon2 horizon; do
        if mkdir "$HOME/.cache/rust-mekanism/$candidato.lock" 2>/dev/null; then NAME=$candidato; break; fi
      done
      [ -n "$NAME" ] || sleep 2
    done
  else
    until mkdir "$HOME/.cache/rust-mekanism/$NAME.lock" 2>/dev/null; do sleep 2; done
  fi
  echo "$NAME" > "$P/container"
  L=$HOME/.cache/rust-mekanism/$NAME.lock
  if [ "$NAME" = horizon ]; then M=$HOME/Games/Horizon/mods; else M=$HOME/Games/Horizon2/mods; fi
  V=$HOME/.local/share/jogo-isolado/$NAME/data/horizon-launcher/versions
  echo "${DONO:-som}" > "$L/dono"
  echo "TRAVA $(date +%T)"
  ATUAL=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
  echo "$ATUAL" > "$P/versao-antes.txt"
  tar -C "$M" -cf "$P/mods-antes.tar" infuser conteudo
else
  NAME=$(cat "$P/container" 2>/dev/null || echo "$NAME")
  if [ "$NAME" = horizon ]; then M=$HOME/Games/Horizon/mods; else M=$HOME/Games/Horizon2/mods; fi
  V=$HOME/.local/share/jogo-isolado/$NAME/data/horizon-launcher/versions
fi
rm -rf "$M/infuser" "$M/conteudo"
tar -C "$M" -xf "$P/fontes.tar"
# O mtime vem de quem enviou (`enviar_fontes.sh`). Se outra frente enviou e compilou depois, o rlib dela é mais novo que estes
# fontes e o cargo o reusa com o código errado (o cache Rust é um só para `horizon` e `horizon2`). O mtime da extração vence.
find "$M/infuser" -name '*.rs' -exec touch {} +
# O launcher compila uma cópia: `native/stage/native-crates` e `native/patched/<hash>/native-crates`. Uma pasta `patched` de mesmo hash
# é reaproveitada com o mtime antigo, e o cargo reusa o rlib mais novo de outra frente (achado pela `cabo-conteudo` em 2026-11-14).
find "$(dirname "$V")/native/stage" "$(dirname "$V")"/native/patched/*/native-crates -name '*.rs' -exec touch {} + 2>/dev/null || true
JOGO_CMD=true "$HOME/.local/bin/$NAME" offline
docker update --cpus 4 --memory 6g --memory-swap 6g "$NAME"
# `SEM_LIM=1`: a vaga já é de quem chama (o `lim-jogo` a segura para o build e para o jogo do teste); passar de novo por `lim-build` esperaria por ela para sempre.
LIM="$HOME/.cache/rust-mekanism/lim-build"; [ "${SEM_LIM:-}" = 1 ] && LIM=
# O mtime vale a partir do momento em que a vaga é nossa: a espera na fila do `lim-build` pode durar horas, e uma frente que compila nesse meio tempo deixa um rlib mais novo que os fontes carimbados
# acima (o cargo o reusa com o código dela: 2026-11-15, `rede-reabrir`, `installed()` com a assinatura de outra frente). Por isso o carimbo se repete dentro da vaga, logo antes do `native-build`.
export M NAME V JOBS
$LIM sh -c 'find "$M/infuser" -name "*.rs" -exec touch {} +; find "$(dirname "$V")/native/stage" "$(dirname "$V")"/native/patched/*/native-crates -name "*.rs" -exec touch {} + 2>/dev/null; exec env JOGO_CMD="env CARGO_BUILD_JOBS=${JOBS:-2} horizon-launcher native-build ~/Games/Horizon/mods/infuser --instance horizon --profile dev --trust-native-code" "$HOME/.local/bin/$NAME" offline'
echo "EXIT $?"
NOVO=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' "$V/../instances.json" | head -1)
echo "novo $NOVO"
# Só o build atual fica.
for d in "$V"/native-*; do b=$(basename "$d"); [ "$b" = "$NOVO" ] || { echo "apaga $b"; rm -rf "$d"; }; done
