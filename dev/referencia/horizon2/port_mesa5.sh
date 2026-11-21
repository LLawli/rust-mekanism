#!/bin/bash
# Roda no desktop, com a trava do `horizon` e a vaga na mão. A 5.ª rodada da `mesa-seguranca` no port, um cliente só, headless e mudo, com o PipeWire de gravação:
# o contorno desenhado do apoio (um quadro sem perda da captura do gamescope, `contorno_port.png`, na mesma pose da foto `contorno_apoio` do original em `cenas/dois_mesa5.txt`,
# porque o `screenshot` do `horizon-ctl` não desenha o contorno), o principal que sai no mesmo comando que troca o apoio (uma função de data pack: a troca e, logo depois,
# a leitura da mesa), o `redstone` do tile do apoio, o pick com e sem o Ctrl no apoio, e onde o Configurator em `EMPTY` solta o item. Uma linha `hora rótulo: valor` por
# leitura em PASTA/port5.log; escreve PASTA/port5.fim ao acabar, falhando ou não. Uso: PASTA=... port_mesa5.sh
C=$HOME/.cache/rust-mekanism; P=${PASTA:-$C/mesa-seguranca-5}; D=$HOME/.local/share/jogo-isolado/horizon/data/horizon; cd $P
rm -f port5.fim; trap 'ctl options-set soundCategory_master 0.0 >/dev/null 2>&1; ctl quit >/dev/null 2>&1; sleep 5; $C/gravar-jogo stop mesa5 >/dev/null 2>&1; echo $? > port5.fim' EXIT
V=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/instances.json | head -1)
ctl() { docker exec horizon ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/$V/horizon-ctl --timeout 120 "$@"; }
say() { echo "$(date +%T.%N | cut -c1-12) $*"; }
cmd() { say "cmd $1: $(ctl command "$1" | tr -d '\n' | head -c 400)"; }
MUNDO=${MUNDO:-mesa5}; BASE=${BASE:-spike}; X=3
rm -rf "$D/saves/$MUNDO"; cp -a --reflink=auto "$D/saves/$BASE" "$D/saves/$MUNDO"; rm -rf "$D/saves/$MUNDO/data/mekanism/frequency"
# A função: troca o apoio por pedra e, no comando seguinte, marca de verde (a mesa saiu) ou de vermelho (a mesa ficou) um bloco ao lado.
F="$D/saves/$MUNDO/datapacks/mesa5"; mkdir -p "$F/data/mesa5/function"
echo '{"pack":{"description":"mesa5","min_format":121,"max_format":121}}' > "$F/pack.mcmeta"
printf '%s\n' "setblock $X -59 3 minecraft:stone" \
  "execute if block $X -60 3 mekanism:security_desk run setblock 6 -60 0 minecraft:red_wool" \
  "execute unless block $X -60 3 mekanism:security_desk run setblock 6 -60 0 minecraft:lime_wool" > "$F/data/mesa5/function/troca.mcfunction"
S=$($C/gravar-jogo start mesa5); say "pipewire $S"
docker stop horizon >/dev/null 2>&1; sleep 2
JOGO_PW_SOCK=$S JOGO_CMD=true ~/.local/bin/horizon offline >/dev/null 2>&1
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
JOGO_PW_SOCK=$S FPS=30 PASTA=$P JOGAR_EXTRA="--name Dev" H_NAME=horizon $P/abrir_horizon.sh "$MUNDO"
for i in $(seq 60); do sleep 3; ctl state 2>/dev/null | grep -q '"gamemode"' && break; done
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
for i in 1 2 3 4 5 6; do $C/audio_nulo.sh ligar horizon >/dev/null; $C/audio_nulo.sh verificar horizon && break; sleep 4; done
say "audio $($C/audio_nulo.sh seguro horizon)"
ctl state | grep -q '"gamemode"' || { say "o jogo não abriu"; exit 1; }
grep -a "stream available on node" game.log | tail -1
# A caixa e o mundo da foto do original (`caixa 300` da `CenaDois`, x deslocado de 297, com o teto aberto em cima da mesa como `cenas/contorno_mesa5.txt`); o chat
# escondido, como no original, onde os comandos não vão ao chat.
for c in "time set 6000" "weather clear" "gamerule advance_time false" "gamerule advance_weather false" "difficulty peaceful" \
  "fill -3 -61 -6 9 -50 10 minecraft:air" "fill -3 -61 -6 9 -50 10 minecraft:black_concrete hollow" "fillbiome -3 -61 -6 9 -50 10 minecraft:plains" \
  "fill 3 -50 -5 8 -50 9 minecraft:air" \
  "gamemode survival Dev" "tp Dev $X.0 -60.0 0.0 0 0" "clear Dev" "give Dev mekanism:security_desk 1"; do cmd "$c"; sleep 0.4; done
sleep 1
for i in 1 2 3 4; do out=$(ctl place $X -60 3 --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "por: $(echo $out | head -c 120)"
cmd "clear Dev"; cmd "setblock 4 -59 1 minecraft:light[level=15]"; cmd "setblock 2 -59 1 minecraft:light[level=15]"
ctl options-set guiScale 2 >/dev/null; ctl options-set chatVisibility 2 >/dev/null
cmd "tp Dev $X.5 -60.0 0.5 0 6.5"; sleep 3
say "mira: $(ctl looking-at | tr -d '\n ' | head -c 200)"
id=$(docker exec gravador-mesa5 sh -c "pw-cli ls Node 2>/dev/null" | awk "/^\tid/{id=\$2} /node.name = \"gamescope\"/{print id}" | tr -d , | head -1)
docker exec gravador-mesa5 sh -c "rm -f /saida/contorno_port_*.png; gst-launch-1.0 -q pipewiresrc path=$id num-buffers=8 ! videoconvert ! video/x-raw,format=RGB ! pngenc ! multifilesink location=/saida/contorno_port_%d.png" >/dev/null 2>&1
cp "$C/gravador/saida/contorno_port_7.png" "$P/contorno_port.png" 2>/dev/null; rm -f $C/gravador/saida/contorno_port_*.png; say "quadro: $(ls -la $P/contorno_port.png 2>&1 | head -c 120)"
ctl options-set chatVisibility 0 >/dev/null
# O principal sai no mesmo comando que troca o apoio.
cmd 'datapack enable "file/mesa5"'; sleep 0.5
cmd "function mesa5:troca"; sleep 0.5
say "mesma_operacao mesa: $(ctl block $X -60 3 | tr -d '\n ' | head -c 120)"
say "mesma_operacao marca: $(ctl block 6 -60 0 | tr -d '\n ' | head -c 120)"
# A mesa de novo, para o redstone e o pick.
cmd "setblock $X -59 3 minecraft:air"; cmd "setblock 6 -60 0 minecraft:air"; cmd "kill @e[type=item]"
cmd "tp Dev $X.0 -60.0 0.0 0 0"; cmd "give Dev mekanism:security_desk 1"; sleep 0.6
for i in 1 2 3 4; do out=$(ctl place $X -60 3 --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "por: $(echo $out | head -c 120)"
cmd "data get block $X -59 3"
cmd "setblock 4 -59 3 minecraft:redstone_block"; sleep 0.3
cmd "data get block $X -59 3"
cmd "setblock 4 -59 3 minecraft:air"; sleep 0.3
cmd "data get block $X -59 3"
# O pick no apoio, sem e com o Ctrl, em criativo, com o bloco de redstone encostado (como o passo `ctrl_pick_bounding` do rastro).
cmd "setblock 4 -59 3 minecraft:redstone_block"; cmd "gamemode creative Dev"; cmd "clear Dev"
cmd "tp Dev $X.5 -60.0 0.5 0 6.5"; sleep 1.5
say "mira: $(ctl looking-at | tr -d '\n ' | head -c 200)"
ctl select 0 >/dev/null 2>&1; ctl mouse middle click >/dev/null; sleep 0.8
cmd "data get entity Dev SelectedItem"
cmd "clear Dev"; sleep 0.3
ctl key ctrl down >/dev/null; sleep 0.2; ctl mouse middle click >/dev/null; sleep 0.3; ctl key ctrl up >/dev/null; sleep 0.8
cmd "data get entity Dev SelectedItem"
cmd "clear Dev"; cmd "setblock 4 -59 3 minecraft:air"; cmd "gamemode survival Dev"
# O Configurator em EMPTY, agachado, na face norte da mesa: o Crusher sem dono que estava no slot de travar sai pela face (três vezes).
for n in 1 2 3; do
  cmd "kill @e[type=item]"; cmd "clear Dev"; cmd "item replace entity Dev hotbar.0 with mekanism:crusher"; ctl select 0 >/dev/null 2>&1
  cmd "tp Dev $X.5 -60.0 0.5 0 20"; sleep 0.8
  ctl interact $X -60 3 north >/dev/null; sleep 1.2; ctl click-slot 0 shift >/dev/null; sleep 0.8; ctl close; sleep 0.5
  cmd "data get block $X -60 3 Items"
  cmd "item replace entity Dev hotbar.0 with mekanism:configurator[mekanism:configurator_mode=\"empty\"]"; sleep 0.4
  # O servidor parado: a posição e a velocidade lidas são as do nascimento (o rastro do original as lê no mesmo tick).
  cmd "tick freeze"; sleep 0.3
  ctl key shift down >/dev/null; sleep 0.4; ctl interact $X -60 3 north >/dev/null; sleep 0.2; ctl key shift up >/dev/null; sleep 0.3
  cmd "data get entity @e[type=item,limit=1] Pos"
  cmd "data get entity @e[type=item,limit=1] Motion"
  cmd "tick unfreeze"
  say "itens $n: $(ctl entities 8 item | tr -d '\n' | head -c 400)"
done
cmd "gamerule advance_time true"; cmd "gamerule advance_weather true"
