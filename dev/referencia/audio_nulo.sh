#!/bin/bash
# Roda no desktop. Tira o som de um jogo de teste das caixas do usuário e o grava: cria um sink nulo (`som_nulo`, nada toca), liga nele o stream do container e grava o que
# chega com `pw-record`. O container só reproduz, mas o stream aparece no PipeWire do host. Como achar o stream de cada jogo (medido em 2026-11-08):
#   - o `horizon` toca por `ALSA plug-in [horizon-client]`, sem `target.object` (segue o sink padrão);
#   - o `mekanism-ref` toca como `java`, com `target.object` fixo nas caixas do usuário (o OpenAL pede o sink padrão pelo nome); o `client.config.name` que a primeira versão deste
#     script procurava não existe no nó (só o objeto Client tem `config.name`), e por isso a primeira gravação do original não achou o stream.
# Os dois têm `application.process.host` igual ao hostname do container (o id de 12 caracteres), que é a chave daqui.
#   audio_nulo.sh ligar CONTAINER          cria o sink se faltar e manda os streams do CONTAINER para ele (metadata `target.object`, o que o `pactl move-sink-input` faz, e tira o link
#                                           que já houver para outro sink); imprime quantos moveu. Repita até o stream existir
#   audio_nulo.sh verificar CONTAINER      imprime `ok N` se há ao menos um stream do CONTAINER e todos os links de saída dele vão só para o sink nulo; senão `ruim ...` e sai com 1
#   audio_nulo.sh gravar ARQUIVO.wav [PID] grava o sink nulo até receber SIGINT/SIGTERM (o pid do gravador vai para o arquivo PID, se dado)
#   audio_nulo.sh remover                  apaga o sink nulo
set -u
export XDG_RUNTIME_DIR=/run/user/$(id -u)
SINK=som_nulo
criar() {
  pw-cli ls Node 2>/dev/null | grep -q "node.name = \"$SINK\"" && return
  pw-cli create-node adapter "{ factory.name=support.null-audio-sink node.name=$SINK media.class=Audio/Sink object.linger=true audio.position=[FL FR] }" >/dev/null
  sleep 1
}
host_de() { docker inspect -f '{{.Config.Hostname}}' "$1" 2>/dev/null; }
case "$1" in
  ligar)
    criar
    python3 - "$(host_de "$2")" "$SINK" <<'PY'
import json, subprocess, sys
host, sink = sys.argv[1], sys.argv[2]
dump = json.loads(subprocess.run(["pw-dump"], capture_output=True, text=True).stdout)
movidos = 0
for o in dump:
    if o.get("type") != "PipeWire:Interface:Node":
        continue
    p = o["info"]["props"]
    if p.get("media.class") != "Stream/Output/Audio" or not host or p.get("application.process.host") != host:
        continue
    name, nid = p["node.name"], o["id"]
    # O alvo pelo metadata, como o `pactl move-sink-input`: vale mesmo para um stream com `target.object` próprio.
    subprocess.run(["pw-metadata", "-n", "default", str(nid), "target.object", sink], capture_output=True)
    for side in ("FL", "FR"):
        port = f"{name}:output_{side}"
        links = subprocess.run(["pw-link", "-l", port], capture_output=True, text=True).stdout
        for line in links.splitlines():
            if "|->" in line and not line.split("|->", 1)[1].strip().startswith(sink + ":"):
                subprocess.run(["pw-link", "-d", port, line.split("|->", 1)[1].strip()], capture_output=True)
    movidos += 1
print(f"movidos {movidos}")
PY
    ;;
  verificar)
    python3 - "$(host_de "$2")" "$SINK" <<'PY'
import json, subprocess, sys
host, sink = sys.argv[1], sys.argv[2]
dump = json.loads(subprocess.run(["pw-dump"], capture_output=True, text=True).stdout)
nodes = {o["id"]: o["info"]["props"] for o in dump if o.get("type") == "PipeWire:Interface:Node"}
streams = [i for i, p in nodes.items() if p.get("media.class") == "Stream/Output/Audio" and host and p.get("application.process.host") == host]
destinos = []
for o in dump:
    if o.get("type") == "PipeWire:Interface:Link" and o["info"].get("output-node-id") in streams:
        destinos.append(nodes.get(o["info"].get("input-node-id"), {}).get("node.name", "?"))
if streams and destinos and all(d == sink for d in destinos):
    print(f"ok {len(streams)} streams, {len(destinos)} links")
else:
    print(f"ruim streams={streams} destinos={destinos}")
    sys.exit(1)
PY
    ;;
  gravar)
    [ -n "${3:-}" ] && echo $$ > "$3"
    exec pw-record --target "$SINK" -P '{ stream.capture.sink=true }' --rate 48000 --channels 2 "$2"
    ;;
  remover)
    id=$(pw-cli ls Node | awk '/^\tid /{id=$2} /node.name = "'$SINK'"/{print id}' | tr -d ',')
    [ -n "$id" ] && pw-cli destroy "$id"
    ;;
esac
