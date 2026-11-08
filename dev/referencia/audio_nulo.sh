#!/bin/bash
# Roda no desktop. Tira o som de um jogo de teste das caixas do usuário e o grava: cria um sink nulo (`som_nulo`, nada toca), liga nele o stream do container e grava o que
# chega com `pw-record`. O container só reproduz, mas o stream aparece no PipeWire do host (`client.config.name` termina em `<container>/pulse.conf`).
#   audio_nulo.sh ligar NOME               cria o sink se faltar e move os streams cujo `application.name` tem NOME (`horizon-client`) ou cujo `client.config.name` termina em `/NOME/pulse.conf` (repita até o stream existir; imprime quantos moveu)
#   audio_nulo.sh gravar ARQUIVO.wav       grava o sink nulo até receber SIGINT/SIGTERM
#   audio_nulo.sh remover                  apaga o sink nulo
set -u
export XDG_RUNTIME_DIR=/run/user/$(id -u)
SINK=som_nulo
criar() {
  pw-cli ls Node 2>/dev/null | grep -q "node.name = \"$SINK\"" && return
  pw-cli create-node adapter "{ factory.name=support.null-audio-sink node.name=$SINK media.class=Audio/Sink object.linger=true audio.position=[FL FR] }" >/dev/null
  sleep 1
}
case "$1" in
  ligar)
    criar
    # O stream do jogo aparece com `application.name` "ALSA plug-in [horizon-client]" (o container usa o plugin ALSA do pulse) ou com o `client.config.name`
    # terminado em `/<container>/pulse.conf`; o nome das portas tem espaços e colchetes, então a ligação é feita em Python, sem passar pelo shell.
    python3 - "$2" "$SINK" <<'PY'
import json, subprocess, sys
alvo, sink = sys.argv[1], sys.argv[2]
movidos = 0
for o in json.loads(subprocess.run(["pw-dump"], capture_output=True, text=True).stdout):
    if o.get("type") != "PipeWire:Interface:Node":
        continue
    p = o["info"]["props"]
    if p.get("media.class") != "Stream/Output/Audio":
        continue
    if not (alvo in str(p.get("application.name", "")) or str(p.get("client.config.name", "")).endswith(f"/{alvo}/pulse.conf")):
        continue
    name = p["node.name"]
    for side in ("FL", "FR"):
        port = f"{name}:output_{side}"
        links = subprocess.run(["pw-link", "-l", port], capture_output=True, text=True).stdout
        for line in links.splitlines():
            if "|->" in line:
                subprocess.run(["pw-link", "-d", port, line.split("|->", 1)[1].strip()], capture_output=True)
        subprocess.run(["pw-link", port, f"{sink}:playback_{side}"], capture_output=True)
    movidos += 1
print(f"movidos {movidos}")
PY
    ;;
  gravar)
    exec pw-record --target "$SINK" -P '{ stream.capture.sink=true }' --rate 48000 --channels 2 "$2"
    ;;
  remover)
    id=$(pw-cli ls Node | awk '/^\tid /{id=$2} /node.name = "'$SINK'"/{print id}' | tr -d ',')
    [ -n "$id" ] && pw-cli destroy "$id"
    ;;
esac
