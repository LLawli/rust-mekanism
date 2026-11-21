#!/bin/bash
# Roda no desktop, com a `mekanism-ref.lock` e a vaga do `lim-jogo` na mão. O vídeo com som do original na cena do vídeo do port (`cenas/video_mesa5.txt`, a mesma de
# `horizon2/port_mesa_video.sh`), pela captura PipeWire do gamescope do original: o container nasce pelo `mekanism-ref-pw.sh` com o socket do `gravar-jogo start`,
# o roteiro roda pelo `oficial_dois.sh` (um jogador, 30 quadros), e quem grava espera a linha `dois: bloco inicio`, manda o som do jogo ao sink nulo, confere e só então
# cria `som-video-ok` (a `CenaDois` liga o volume depois disso); a cada 0,5 s o `audio_nulo.sh seguro`, e qualquer som fora dos sinks de teste cria `som-video-mudo`
# (o volume volta a 0 no tick seguinte). Escreve SAIDA/video.fim ao acabar, falhando ou não.
# Uso: oficial_mesa_video.sh CenaDois.java video_mesa5.txt SAIDA [SEGUNDOS]
set -u
JAVA=$(readlink -f "$1"); ROTEIRO=$(readlink -f "$2"); SAIDA=$(mkdir -p "$3" && readlink -f "$3"); SEG=${4:-45}
C=$HOME/.cache/rust-mekanism; R=$HOME/.local/share/jogo-isolado/mekanism-ref/src/runs/client; SYNC=$HOME/.local/share/jogo-isolado/mekanism-ref/dois-sync
AQUI=$(dirname "$(readlink -f "$0")")
rm -f "$SAIDA/video.fim"
VIGIA=; REC=; OF=
fim() { rc=$?; [ -n "$VIGIA" ] && kill "$VIGIA" 2>/dev/null; [ -n "$REC" ] && wait "$REC" 2>/dev/null; [ -n "$OF" ] && wait "$OF" 2>/dev/null
  rm -f "$SYNC/som-video-ok" "$SYNC/som-video-mudo"; $C/gravar-jogo stop orig5 >/dev/null 2>&1
  # O container nasceu com o socket de gravação montado, que o `gravar-jogo stop` apaga: um gamescope aberto depois nele trava ao iniciar (medido em 2026-11-21:
  # o cliente parou em `Backend library` por 13 minutos). Ele sai junto, e o próximo `mekanism-ref` cria um container sem o socket.
  docker stop mekanism-ref >/dev/null 2>&1; echo $rc > "$SAIDA/video.fim"; }
trap fim EXIT
S=$($C/gravar-jogo start orig5) || exit 1; echo "pipewire $S"
docker stop mekanism-ref >/dev/null 2>&1; sleep 2
JOGO_PW_SOCK=$S JOGO_CMD=true "$AQUI/mekanism-ref-pw.sh" offline || exit 1
docker update --cpus 4 --memory 6g --memory-swap 6g mekanism-ref >/dev/null
FPS=30 LOCK_MEU=1 SEM_LIM=1 DONO=mesa-seguranca-5 "$AQUI/oficial_dois.sh" "$JAVA" "$ROTEIRO" "$SAIDA" > "$SAIDA/oficial.log" 2>&1 & OF=$!
for i in $(seq 360); do grep -qas "dois: bloco inicio" "$R/logs/latest.log" && break; kill -0 $OF 2>/dev/null || break; sleep 2; done
grep -qas "dois: bloco inicio" "$R/logs/latest.log" || { echo "a cena não chegou ao início"; exit 1; }
ok=
for i in $(seq 10); do $C/audio_nulo.sh ligar mekanism-ref >/dev/null 2>&1; if $C/audio_nulo.sh verificar mekanism-ref | grep -q '^ok'; then ok=1; break; fi; sleep 0.5; done
echo "verificar: $($C/audio_nulo.sh verificar mekanism-ref 2>&1 | head -1)"
$C/gravar-jogo rec orig5 orig_mesa5.mp4 "$SEG" mekanism-ref > "$SAIDA/rec.log" 2>&1 & REC=$!
sleep 2
[ -n "$ok" ] && touch "$SYNC/som-video-ok"
( while kill -0 $REC 2>/dev/null; do $C/audio_nulo.sh seguro mekanism-ref >/dev/null 2>&1 || { touch "$SYNC/som-video-mudo"; echo "som fora $(date +%T)"; }; sleep 0.5; done ) & VIGIA=$!
wait $REC; REC=
kill $VIGIA 2>/dev/null; VIGIA=
touch "$SYNC/som-video-mudo"
echo "rec: $(tail -3 "$SAIDA/rec.log" | tr '\n' ' ')"
cp "$C/gravador/saida/orig_mesa5.mp4" "$SAIDA/" && rm -f "$C/gravador/saida/orig_mesa5.mp4"
wait $OF; OF=
echo "oficial: $(tail -3 "$SAIDA/oficial.log" | tr '\n' ' ')"
