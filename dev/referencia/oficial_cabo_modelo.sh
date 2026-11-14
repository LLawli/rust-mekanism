#!/bin/bash
# Roda no desktop. A `CenaBlocos` com o roteiro `cenas/cabo_modelo.json` (os 22 transmissores com `NORMAL`, `PUSH`, `PULL` e `NONE`, e os trechos retos) no cliente do original, duas vezes: com a opção
# de cliente `opaqueTransmitters` desligada (a padrão) e ligada (`Mekanism/client.toml`, `.gameRestart()`: o arquivo é lido ao abrir o jogo), para ver o que ela muda no desenho do bloco. Pega a
# `mekanism-ref.lock` (laço de `mkdir` a cada 5 s, nunca `pgrep`), devolve o `client.toml` como estava e solta a trava. Os prints ficam em SAIDA/opaco_false e SAIDA/opaco_true
# (`oficial_<tiro>.png`, de 1920 por 1080). Escreve `SAIDA.fim` (o código de saída) também quando falha. Uso: oficial_cabo_modelo.sh CenaBlocos.java cabo_modelo.json oficial_blocos.sh SAIDA   (DONO=nome da frente)
set -u
JAVA=$(readlink -f "$1")
CENA=$(readlink -f "$2")
RUN=$(readlink -f "$3")
SAIDA=$4
DONO=${DONO:-frente}
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
TOML=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client/config/Mekanism/client.toml
FIM=$SAIDA.fim
rm -f "$FIM"
mkdir -p "$SAIDA"
trap 'echo $? > "$FIM"' EXIT
until mkdir "$LOCK" 2>/dev/null; do sleep 5; done
echo "$DONO" > "$LOCK/dono"
echo "TRAVA $(date +%T)"
cp "$TOML" "$C/client.toml.antes-cabo-modelo"
restore() {
  cp "$C/client.toml.antes-cabo-modelo" "$TOML" && rm -f "$C/client.toml.antes-cabo-modelo"
  rm -rf "$LOCK"
  echo "RESTAURADO $(date +%T)"
}
trap 'rc=$?; restore; echo $rc > "$FIM"' EXIT
for valor in false true; do
  sed -i "s/^\topaqueTransmitters = .*/\topaqueTransmitters = $valor/" "$TOML"
  grep -c "opaqueTransmitters = $valor" "$TOML"
  bash "$RUN" "$JAVA" "$CENA" "$SAIDA/opaco_$valor"
  echo "FIM $valor $?"
done
