#!/bin/bash
# Roda no desktop, com a trava do `horizon` e a vaga na mão. O port: a mesa posta pelo anfitrião `Dev` num mundo copiado, `Notch`, um nome que não existe e `dev` pela tela,
# com o relógio do desktop em cada passo e um vigia do `usercache.json`. Escreve PASTA/port.fim ao acabar, falhando ou não.
C=$HOME/.cache/rust-mekanism; P=$C/mesa-seguranca-3; D=$HOME/.local/share/jogo-isolado/horizon/data/horizon; cd $P
rm -f port.fim; trap 'echo $? > port.fim' EXIT
V=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/instances.json | head -1)
ctl() { docker exec horizon ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/$V/horizon-ctl --timeout 120 "$@"; }
now() { date +%s.%N; }
say() { echo "$(now) $*"; }
cmd() { say "cmd $1: $(ctl command "$1" | head -c 300)"; }
MUNDO=${MUNDO:-mesa3}; BASE=${BASE:-spike}
rm -rf "$D/saves/$MUNDO"; cp -a --reflink=auto "$D/saves/$BASE" "$D/saves/$MUNDO"; rm -rf "$D/saves/$MUNDO/data/mekanism/frequency"
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
PASTA=$P JOGAR_EXTRA="--name Dev" H_NAME=horizon ./abrir_horizon.sh "$MUNDO"
for i in $(seq 60); do sleep 3; ctl state 2>/dev/null | grep -q '"gamemode"' && break; done
docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
for i in 1 2 3 4 5 6; do $C/audio_nulo.sh ligar horizon >/dev/null; $C/audio_nulo.sh verificar horizon && break; sleep 4; done
say "audio $($C/audio_nulo.sh seguro horizon)"
ctl state | grep -q '"gamemode"' || { say "o jogo não abriu"; exit 1; }
( prev=x; while [ ! -e port.fim ]; do m=$(stat -c '%.9Y %s' $D/usercache.json 2>/dev/null); [ "$m" != "$prev" ] && { echo "$(now) usercache $m"; prev=$m; }; sleep 0.02; done ) > port_watch.log 2>&1 &
say "arquivos antes: $(ls $D/usercache.json $D/usernamecache.json 2>&1 | tr '\n' ' ')"
X=3; L=232; T=65
for c in "time set noon" "weather clear" "gamerule advance_time false" "gamerule advance_weather false" "difficulty peaceful" \
  "fill -3 -61 -6 9 -50 10 minecraft:air" "fill -3 -61 -6 9 -50 10 minecraft:black_concrete hollow" "tp Dev 3.0 -60.0 0.0 0 0" "clear Dev" "give Dev mekanism:security_desk 1"; do cmd "$c"; sleep 0.5; done
for i in 1 2 3 4; do out=$(ctl place $X -60 3 --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "por: $(echo $out | head -c 200)"
cmd "clear Dev"; cmd "tp Dev 3.0 -60.0 0.0 0 0"; sleep 1
say "nbt: $(ctl command "data get block $X -60 3" | head -c 600)"
ctl options-set guiScale 3
abrir() { for i in 1 2 3 4; do ctl interact $X -60 3 >/dev/null; sleep 0.8; ctl screen | tr -d ' ' | grep -q '"screen":"container"' && { ctl look 0 0 >/dev/null; return; }; done; say "a mesa não abriu"; }
nome() {
  ctl click-at $((L+60)) $((T+73)) >/dev/null
  for ch in $(echo "$1" | grep -o .); do ctl type "$ch" >/dev/null; sleep 0.05; done
  sleep 0.5
  say "enter $1"
  ctl key enter tap >/dev/null
  say "enter voltou $1"
  # O cliente responde durante a consulta? O tempo de cada `state` nos 3 s seguintes.
  for i in $(seq 12); do a=$(now); ctl state >/dev/null; b=$(now); echo "$a state levou $(echo "$b - $a" | bc -l 2>/dev/null || python3 -c "print($b-$a)")"; sleep 0.1; done
  sleep ${2:-9}
}
abrir; sleep 1
nome Notch
nome zz_nao_existe_77
nome dev 3
ctl close; sleep 0.8; abrir; sleep 1
ctl scroll-at $((L+2)) $((T+2)) 0 >/dev/null; sleep 0.6
mkdir -p $HOME/.local/share/jogo-isolado/horizon/data/shots/mesa3
ctl screenshot $HOME/.local/share/jogo-isolado/horizon/data/shots/mesa3/port_mojang_lista_3.png; sleep 0.6
say "tela: $(ctl screen | head -c 400)"
ctl close; sleep 0.5
# Passo 4: o mesmo anfitrião com o mundo aberto à LAN: que UUID é o dono de uma mesa posta agora, e quem `dev` vira.
say "lan: $(ctl lan open 0 commands | head -c 200)"; sleep 2
cmd "give Dev mekanism:security_desk 1"; sleep 0.5
out=$(ctl place 6 -60 3 --item mekanism:security_desk); say "por lan: $(echo $out | head -c 200)"; sleep 1
say "nbt lan: $(ctl command "data get block 6 -60 3" | head -c 600)"
say "usernamecache com lan: $(cat $D/usernamecache.json 2>&1 | tr -d '\n')"
X=6; cmd "tp Dev 6.0 -60.0 0.0 0 0"; sleep 1; abrir; sleep 1; nome dev 3; ctl close; X=3
cmd "gamerule advance_time true"; cmd "gamerule advance_weather true"
ctl quit; sleep 8
say "fechado"
F="$D/saves/$MUNDO/data/mekanism/frequency/security/public.dat"
say "freq disco: $(python3 -I $C/nbt_ler.py "$F" 2>&1 | tail -1 | head -c 3000)"
say "usercache: $(cat $D/usercache.json 2>&1)"
say "usernamecache: $(cat $D/usernamecache.json 2>&1 | tr -d '\n')"
say "log: $(grep -a "machinery_basis" game.log | grep -ai "profile\|connect\|cache" | tail -8)"
