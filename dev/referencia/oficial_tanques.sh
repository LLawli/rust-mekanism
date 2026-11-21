#!/bin/bash
# Roda no desktop, dentro de uma vaga que já é de quem chama (`lim-jogo`). Roda a `CenaTelas` no cliente oficial (`runGameTestClient`, sem janela, 1920 por 1080,
# 30 quadros por segundo) com uma lista de casos, guarda os prints e grava, com som, um vídeo de verdade do começo da cena (os casos que vêm depois do primeiro print).
# Uso: oficial_tanques.sh CenaTelas.java cena-telas SAIDA [SEGUNDOS] [DONO]
#   - pega a `mekanism-ref.lock` (sai com 9 se ela é de outro) e a solta no fim;
#   - o container do original nasce com o socket do gravador (`mekanism-ref-pw.sh`) e é parado no fim (com a montagem do socket quebrada o próximo gamescope trava nele);
#   - o jogo abre mudo; a `CenaTelas` só sobe o volume mestre e o da categoria `ui` com `dois-sync/som-video-ok`, que este script cria depois de o `audio_nulo.sh verificar`
#     dar `ok`, e o `som-video-mudo` (um link para fora do sink de teste, ou o fim da gravação) o devolve a 0;
#   - `SAIDA/oficial.fim` ganha o código de saída quando o script acaba, falhando ou não; os prints vão para `SAIDA/prints/` e o vídeo para `SAIDA/orig_tanques.mp4`;
#   - o `options.txt`, o `cena-cabo`, a `CenaTelas.java` e o `cena-telas` voltam ao que eram.
set -u
JAVA=$(readlink -f "$1"); CENA=$(readlink -f "$2"); SAIDA=$(mkdir -p "$3" && readlink -f "$3"); SEG=${4:-55}; DONO=${5:-tanques}
AQUI=$(dirname "$(readlink -f "$0")")
C=$HOME/.cache/rust-mekanism
R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client
N=$HOME/.local/share/jogo-isolado/mekanism-ref/src/src/gameTest/java/mekanism/common/tests/network
SYNC=$HOME/.local/share/jogo-isolado/mekanism-ref/dois-sync
GRAV=orig-$DONO
rm -f "$SAIDA/oficial.fim"
TRAVA=; VIGIA=; REC=; OF=
fim() {
  rc=$?
  [ -n "$VIGIA" ] && kill "$VIGIA" 2>/dev/null
  [ -n "$REC" ] && wait "$REC" 2>/dev/null
  [ -n "$OF" ] && wait "$OF" 2>/dev/null
  rm -f "$SYNC/som-video-ok" "$SYNC/som-video-mudo"
  "$C/gravar-jogo" stop "$GRAV" >/dev/null 2>&1
  docker stop mekanism-ref >/dev/null 2>&1
  if [ -n "$TRAVA" ]; then
    rm -f "$N/CenaTelas.java" "$R/cena-telas"
    [ -e "$SAIDA/cena-cabo.antes" ] && mv "$SAIDA/cena-cabo.antes" "$R/cena-cabo"
    [ -e "$SAIDA/options.txt.antes" ] && cp "$SAIDA/options.txt.antes" "$R/options.txt" && rm -f "$SAIDA/options.txt.antes"
    mkdir -p "$SAIDA/prints"
    mv "$R"/screenshots/oficial_*.png "$SAIDA/prints/" 2>/dev/null
    # Os rastros que a `CenaTelas` grava na pasta do cliente (7.ª rodada): os tooltips dos químicos e as abas criativas com os tanques cheios.
    mv "$R/chemical_tooltips.jsonl" "$R/creative_tab_tanks.jsonl" "$SAIDA/" 2>/dev/null
    rm -rf "$C/mekanism-ref.lock"
  fi
  echo "$rc" > "$SAIDA/oficial.fim"
}
trap fim EXIT
mkdir "$C/mekanism-ref.lock" 2>/dev/null || { echo "a mekanism-ref.lock é de $(cat "$C/mekanism-ref.lock/dono" 2>/dev/null)"; exit 9; }
TRAVA=1
echo "$DONO" > "$C/mekanism-ref.lock/dono"
cp "$R/options.txt" "$SAIDA/options.txt.antes"
# Mudo: o jogo de teste nunca toca nas caixas do usuário.
sed -i 's/^soundCategory_master:.*/soundCategory_master:0.0/' "$R/options.txt"
[ -e "$R/cena-cabo" ] && mv "$R/cena-cabo" "$SAIDA/cena-cabo.antes"
mkdir -p "$SYNC"; rm -f "$SYNC/som-video-ok" "$SYNC/som-video-mudo"
rm -f "$R"/screenshots/oficial_*.png
cp "$JAVA" "$N/CenaTelas.java"
cp "$CENA" "$R/cena-telas"
S=$("$C/gravar-jogo" start "$GRAV") || exit 1
echo "pipewire $S"
docker stop mekanism-ref >/dev/null 2>&1; sleep 2
JOGO_PW_SOCK=$S JOGO_CMD=true "$AQUI/mekanism-ref-pw.sh" offline || exit 1
docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref >/dev/null
JOGO_PW_SOCK=$S JOGO_CMD='gamescope --backend headless -W 1920 -H 1080 -w 1920 -h 1080 -r 30 -- env -u WAYLAND_DISPLAY sh -c "cd \"\$FONTE\" && ./gradlew --no-daemon --max-workers=4 runGameTestClient"' \
  "$AQUI/mekanism-ref-pw.sh" offline > "$SAIDA/oficial.log" 2>&1 &
OF=$!
# O primeiro print da cena: o cliente está no mundo e os casos começaram.
for i in $(seq 900); do
  ls "$R"/screenshots/oficial_*.png >/dev/null 2>&1 && break
  kill -0 $OF 2>/dev/null || break
  sleep 2
done
ls "$R"/screenshots/oficial_*.png >/dev/null 2>&1 || { echo "a cena não chegou ao primeiro print"; exit 1; }
ok=
for i in $(seq 20); do
  "$AQUI/audio_nulo.sh" ligar mekanism-ref >/dev/null 2>&1
  if "$AQUI/audio_nulo.sh" verificar mekanism-ref | grep -q '^ok'; then ok=1; break; fi
  sleep 0.5
done
echo "verificar: $("$AQUI/audio_nulo.sh" verificar mekanism-ref 2>&1 | head -1)"
"$C/gravar-jogo" rec "$GRAV" orig_tanques.mp4 "$SEG" mekanism-ref > "$SAIDA/rec.log" 2>&1 &
REC=$!
sleep 2
[ -n "$ok" ] && touch "$SYNC/som-video-ok"
( while kill -0 $REC 2>/dev/null; do "$AQUI/audio_nulo.sh" seguro mekanism-ref >/dev/null 2>&1 || { touch "$SYNC/som-video-mudo"; echo "som fora $(date +%T)"; }; sleep 0.5; done ) &
VIGIA=$!
wait $REC; REC=
kill $VIGIA 2>/dev/null; VIGIA=
touch "$SYNC/som-video-mudo"
echo "rec: $(tail -3 "$SAIDA/rec.log" | tr '\n' ' ')"
cp "$C/gravador/saida/orig_tanques.mp4" "$SAIDA/" && rm -f "$C/gravador/saida/orig_tanques.mp4"
wait $OF; OF=
echo "oficial: $(tail -3 "$SAIDA/oficial.log" | tr '\n' ' ')"
