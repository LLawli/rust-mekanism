#!/bin/bash
# Grava vídeo de verdade do jogo de teste, pela captura PipeWire do gamescope, num PipeWire privado
# (container gravador-<nome>), nunca o da sessão do usuário.
#   gravar-jogo start <nome>            sobe o PipeWire privado; exporte JOGO_PW_SOCK=<saída> antes de criar o container do jogo
#   gravar-jogo rec <nome> <arq.mp4> <s> grava <s> segundos (o arquivo fica em ~/.cache/rust-mekanism/gravador/saida/)
#   gravar-jogo stop <nome>             derruba o gravador
set -e
base=~/.cache/rust-mekanism/gravador; cmd=$1; nome=$2; run=$base/run-$nome; c=gravador-$nome
mkdir -p "$base/saida"
case "$cmd" in
start)
  docker rm -f "$c" >/dev/null 2>&1 || true
  rm -rf "$run"; mkdir -p "$run"; chmod 700 "$run"
  docker run -d --rm --init --name "$c" --user "$(id -u):$(id -g)" --cap-drop ALL --security-opt no-new-privileges \
    --memory 1g --cpus 2 -e XDG_RUNTIME_DIR=/run/pw -e HOME=/tmp \
    -v "$run:/run/pw" -v "$base/saida:/saida" gravador-jogo:1 \
    sh -c "pipewire & sleep 1; DBUS_SESSION_BUS_ADDRESS=disabled: wireplumber & wait" >/dev/null
  for i in $(seq 30); do [ -S "$run/pipewire-0" ] && break; sleep 0.5; done
  [ -S "$run/pipewire-0" ] || { echo "PipeWire privado não subiu" >&2; docker logs "$c" >&2; exit 1; }
  echo "$run/pipewire-0" ;;
rec)
  # rec <nome> <arq.mp4> <segundos> [container]: com o container, grava também o som do jogo num sink nulo próprio
  # (som_grav_<nome>), que só recebe o stream desse container, e junta ao vídeo. Nada toca nas caixas.
  arq=$3; seg=$4; cont=${5:-}; nomearq=$(basename "$arq")
  id=$(docker exec "$c" sh -c "pw-cli ls Node 2>/dev/null" | awk "/^\tid/{id=\$2} /node.name = \"gamescope\"/{print id}" | tr -d , | head -1)
  [ -n "$id" ] || { echo "sem o nó do gamescope no PipeWire privado" >&2; docker exec "$c" pw-cli ls Node >&2; exit 1; }
  an=~/.cache/rust-mekanism/audio_nulo.sh; sink=som_grav_$nome; wav=$base/saida/${nomearq%.mp4}.wav; pr=
  # O sink próprio some em todo caminho de saída (erro, sinal, fim): o `trap` devolve o stream ao `som_nulo` ANTES de apagar o sink (nunca cai no sink padrão, as caixas) e depois o apaga.
  # Sem `set -e` daqui em diante: um `wait` ou um `remover` com status diferente de 0 não pode sair do script antes da limpeza.
  limpar() {
    set +e
    trap - EXIT INT TERM HUP
    if [ -n "$pr" ]; then kill -INT "$pr" 2>/dev/null; wait "$pr" 2>/dev/null; pr=; fi
    if [ -n "$cont" ]; then
      bash "$an" ligar "$cont" >/dev/null 2>&1
      bash "$an" verificar "$cont" >/dev/null 2>&1 || echo "AVISO: confira o som de $cont" >&2
      AUDIO_SINK=$sink bash "$an" remover
    fi
  }
  set +e
  if [ -n "$cont" ]; then
    trap limpar EXIT
    trap "exit 1" INT TERM HUP
    AUDIO_SINK=$sink bash "$an" ligar "$cont" >/dev/null
    AUDIO_SINK=$sink bash "$an" verificar "$cont" >/dev/null || { echo "o som de $cont não foi para $sink" >&2; exit 1; }
    AUDIO_SINK=$sink bash "$an" gravar "$wav" & pr=$!
  fi
  docker exec "$c" timeout -s INT "$seg" gst-launch-1.0 -e pipewiresrc path="$id" do-timestamp=true ! videoconvert ! videorate ! video/x-raw,framerate=30/1 \
    ! x264enc speed-preset=veryfast tune=zerolatency bitrate=6000 ! video/x-h264,profile=high ! h264parse ! mp4mux ! filesink location="/saida/$nomearq" >/dev/null 2>&1
  if [ -n "$cont" ]; then
    limpar
    ffmpeg -v error -y -i "$base/saida/$nomearq" -i "$wav" -c:v copy -c:a aac -b:a 160k -shortest "$base/saida/com_som_$nomearq" \
      && mv "$base/saida/com_som_$nomearq" "$base/saida/$nomearq" && rm -f "$wav" || { echo "o ffmpeg não juntou o som" >&2; exit 1; }
  fi
  ls -la "$base/saida/$nomearq" ;;
stop)
  docker rm -f "$c" >/dev/null 2>&1 || true; rm -rf "$run" ;;
esac
