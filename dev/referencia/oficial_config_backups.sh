#!/bin/bash
# Mede a rotação das cópias `.bak` do NeoForge: sete correções seguidas do `general.toml` (cada uma com um `blockDeactivationDelay` fora da faixa diferente, 5001 a 5007), pelo
# `runGameTestServer` do original. Deixa as cópias `general-1.toml.bak` a `general-5.toml.bak` em `~/.cache/rust-mekanism/config-saida-backups` (`rastros/config_correcao/backups`), devolve o
# `general.toml` e apaga as cópias de dentro do original. Pega `mekanism-ref.lock` como `oficial_config.sh`. Roda no desktop.
set -u
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/gameTestServer
D=$R/config/Mekanism
C=$HOME/.cache/rust-mekanism
LOCK=$C/mekanism-ref.lock
BACKUP=$C/config-antes-backups
SAIDA=$C/config-saida-backups
until mkdir "$LOCK" 2>/dev/null; do sleep 60; done
echo config > "$LOCK/dono"
restore() {
  cp "$BACKUP/general.toml" "$D/general.toml"
  rm -f "$D"/general-[0-9]*.toml.bak
  rm -rf "$BACKUP" "$LOCK"
  echo RESTAURADO
}
trap restore EXIT
mkdir -p "$BACKUP" "$SAIDA"
cp "$D/general.toml" "$BACKUP/general.toml"
rm -rf "$SAIDA"/*
for i in 1 2 3 4 5 6 7; do
  sed "s/^blockDeactivationDelay = .*/blockDeactivationDelay = 500$i/" "$BACKUP/general.toml" > "$D/general.toml"
  "$C/lim-build" env JOGO_CMD='cd "$FONTE" && ./gradlew --no-daemon --max-workers=4 runGameTestServer' "$HOME/.local/bin/mekanism-ref" > "$SAIDA/log$i.txt" 2>&1
  echo "rodada $i: $(grep -h '^blockDeactivationDelay' "$D"/general.toml) $(ls "$D" | grep -c 'general-.*bak')"
done
for f in "$D"/general-[0-9]*.toml.bak; do cp "$f" "$SAIDA/"; done
ls "$SAIDA"
