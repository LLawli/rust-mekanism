#!/bin/bash
# Roda no desktop, com a trava do `horizon` e a vaga na mão. O vídeo do port da 3.ª rodada da `mesa-seguranca`: a mira na metade de cima da Security Desk (o contorno da
# mesa inteira, que o print não mostra), o clique no bloco de apoio abrindo a tela, `Notch` posto pela caixa e a tela reaberta com ele, um Crusher travado por shift-clique
# e o Configurator em `ROTATE` girando a mesa. Um cliente só, a 30 quadros, gravado pela captura PipeWire do gamescope (`gravar-jogo rec`, com o som do jogo: a categoria
# `ui` e o mestre ligados só durante a gravação, com o `audio_nulo.sh seguro` a cada 0,5 s). Escreve PASTA/video.fim ao acabar, falhando ou não.
# Uso: port_mesa_video.sh [SEGUNDOS]   (o mp4 fica em ~/.cache/rust-mekanism/gravador/saida/port_mesa3.mp4)
C=$HOME/.cache/rust-mekanism; P=$C/mesa-seguranca-3; D=$HOME/.local/share/jogo-isolado/horizon/data/horizon; cd $P
rm -f video.fim; trap 'ctl options-set soundCategory_master 0.0 >/dev/null 2>&1; $C/gravar-jogo stop mesa3 >/dev/null 2>&1; echo $? > video.fim' EXIT
V=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/instances.json | head -1)
ctl() { docker exec horizon ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/$V/horizon-ctl --timeout 120 "$@"; }
say() { echo "$(date +%T.%N | cut -c1-12) $*"; }
cmd() { say "cmd $1: $(ctl command "$1" | tr -d '\n' | head -c 160)"; }
SEG=${1:-42}; MUNDO=${MUNDO:-mesa3v}; BASE=${BASE:-spike}; X=3; L=232; T=65
rm -rf "$D/saves/$MUNDO"; cp -a --reflink=auto "$D/saves/$BASE" "$D/saves/$MUNDO"; rm -rf "$D/saves/$MUNDO/data/mekanism/frequency"
S=$($C/gravar-jogo start mesa3); say "pipewire $S"
docker stop horizon >/dev/null 2>&1; sleep 2
JOGO_PW_SOCK=$S JOGO_CMD=true ~/.local/bin/horizon offline >/dev/null 2>&1
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
JOGO_PW_SOCK=$S FPS=30 PASTA=$P JOGAR_EXTRA="--name Dev" H_NAME=horizon ./abrir_horizon.sh "$MUNDO"
for i in $(seq 60); do sleep 3; ctl state 2>/dev/null | grep -q '"gamemode"' && break; done
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
for i in 1 2 3 4 5 6; do $C/audio_nulo.sh ligar horizon >/dev/null; $C/audio_nulo.sh verificar horizon && break; sleep 4; done
say "audio $($C/audio_nulo.sh seguro horizon)"
ctl state | grep -q '"gamemode"' || { say "o jogo não abriu"; exit 1; }
grep -a "stream available on node" game.log | tail -1
for c in "time set noon" "weather clear" "gamerule advance_time false" "gamerule advance_weather false" "difficulty peaceful" \
  "fill -3 -61 -6 9 -50 10 minecraft:air" "fill -3 -61 -6 9 -50 10 minecraft:smooth_stone hollow" "fill -3 -50 -6 9 -50 10 minecraft:air" \
  "gamemode creative Dev" "tp Dev $X.5 -60.0 0.5 0 0" "clear Dev" "give Dev mekanism:security_desk 1"; do cmd "$c"; sleep 0.4; done
for i in 1 2 3 4; do out=$(ctl place $X -60 3 --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "por: $(echo $out | head -c 120)"
cmd "clear Dev"; cmd "item replace entity Dev hotbar.0 with mekanism:crusher"; cmd "tp Dev $X.5 -60.0 0.5 0 0"
ctl options-set guiScale 3 >/dev/null
for k in music record weather block hostile neutral player ambient voice; do ctl options-set soundCategory_$k 0.0 >/dev/null; done
ctl options-set soundCategory_ui 1.0 >/dev/null
ctl look-at $X.5 -59.6 3.0 >/dev/null; sleep 2
# A gravação, com o vigia que zera o mestre se algum som sair do sink de teste.
$C/gravar-jogo rec mesa3 port_mesa3.mp4 "$SEG" horizon > rec.log 2>&1 & REC=$!
sleep 2; ctl options-set soundCategory_master 1.0 >/dev/null
( while kill -0 $REC 2>/dev/null; do $C/audio_nulo.sh seguro horizon >/dev/null 2>&1 || ctl options-set soundCategory_master 0.0 >/dev/null; sleep 0.5; done ) & VIGIA=$!
sleep 1.5
# 1. A mira desce da metade de baixo para a de cima: o contorno da mesa inteira.
ctl look-at $X.5 -58.7 3.7 >/dev/null; sleep 2.5; say "mira: $(ctl looking-at | tr -d '\n ' | head -c 160)"
# 2. O clique no apoio abre a tela.
ctl interact $X -59 3 north >/dev/null; sleep 1.5; say "tela: $(ctl screen | tr -d '\n ' | head -c 60)"
# 3. Notch pela caixa e o botão de confirmar (o clique dele é o som da categoria `ui`); a tela só mostra o nome reaberta.
ctl click-at $((L+60)) $((T+73)) >/dev/null; sleep 0.3
for ch in N o t c h; do ctl type "$ch" >/dev/null; sleep 0.12; done; sleep 0.5
ctl click-at $((L+128)) $((T+73)) >/dev/null; sleep 2.0
ctl close; sleep 0.8
ctl interact $X -59 3 north >/dev/null; sleep 2.5
# 4. O Crusher sem dono por shift-clique vai ao slot de travar.
ctl click-slot 0 shift >/dev/null; sleep 2.5
ctl close; sleep 0.8
say "itens: $(ctl command "data get block $X -60 3 Items" | tr -d '\n' | head -c 300)"
# 5. O Configurator em ROTATE gira a mesa duas vezes.
cmd "item replace entity Dev hotbar.1 with mekanism:configurator[mekanism:configurator_mode=\"rotate\"]"; ctl select mekanism:configurator >/dev/null
ctl look-at $X.5 -59.6 3.0 >/dev/null; sleep 1
ctl interact $X -60 3 north >/dev/null; sleep 1.5
ctl interact $X -60 3 north >/dev/null; sleep 1.5
say "bloco: $(ctl block $X -60 3 | tr -d '\n ' | head -c 160)"
ctl look-at $X.5 -58.7 3.7 >/dev/null
wait $REC; kill $VIGIA 2>/dev/null
ctl options-set soundCategory_master 0.0 >/dev/null
say "rec: $(tail -3 rec.log | tr '\n' ' ')"
# A pedra por shift-clique (que slot do inventário ela ocupa) e o pick no apoio (os componentes do item).
cmd "clear Dev"; cmd "item replace entity Dev hotbar.1 with minecraft:stone"
ctl interact $X -60 3 north >/dev/null; sleep 1; ctl click-slot 1 shift >/dev/null; sleep 0.8; ctl close
say "pedra: $(ctl inventory | python3 -I -c "import json,sys; d=json.load(sys.stdin); print([(s['slot'],s.get('item')) for k in ('hotbar','inventory','main') for s in d.get(k,[]) if s.get('item')=='minecraft:stone'])")"
cmd "gamerule advance_time true"; cmd "gamerule advance_weather true"
ctl quit; sleep 6
