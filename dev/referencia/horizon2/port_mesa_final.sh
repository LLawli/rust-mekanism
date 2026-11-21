#!/bin/bash
# Roda no desktop, com a trava do `horizon` e a vaga na mão. O port: o bloco de apoio da Security Desk (a mira, o clique, o pick, a colisão, a explosão, o apoio trocado
# por comando, o mundo reaberto) e os itens da mesa (o shift-clique nos dois slots, o Heat Generator, o item solto com `mekanism:items`, o Configurator do dono), e a
# lista de confiança no disco depois de um nível parado fechar. Cada leitura sai numa linha `hora rótulo: valor`. Escreve PASTA/final.fim ao acabar, falhando ou não.
# Uso: port_mesa_final.sh [ETAPA ...]  (sem argumento, todas: apoio itens solto configurator remocao explosao reabrir disco)
C=$HOME/.cache/rust-mekanism; P=$C/mesa-seguranca-3; D=$HOME/.local/share/jogo-isolado/horizon/data/horizon; cd $P
rm -f final.fim; trap 'echo $? > final.fim' EXIT
V=$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/instances.json | head -1)
ctl() { docker exec horizon ~/.local/share/jogo-isolado/horizon/data/horizon-launcher/versions/$V/horizon-ctl --timeout 120 "$@"; }
say() { echo "$(date +%T.%N | cut -c1-12) $*"; }
cmd() { say "cmd $1: $(ctl command "$1" | tr -d '\n' | head -c ${2:-300})"; }
get() { say "$1: $(ctl command "$2" | tr -d '\n' | head -c ${3:-900})"; }
MUNDO=${MUNDO:-mesa3f}; BASE=${BASE:-spike}
ETAPAS=${*:-apoio itens solto configurator remocao explosao reabrir disco}
tem() { case " $ETAPAS " in *" $1 "*) return 0;; esac; return 1; }
abrir_jogo() {
  docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
  PASTA=$P JOGAR_EXTRA="--name Dev" H_NAME=horizon ./abrir_horizon.sh "$MUNDO"
  for i in $(seq 60); do sleep 3; ctl state 2>/dev/null | grep -q '"gamemode"' && break; done
  docker update --cpus 4 --memory 6g --memory-swap 6g horizon >/dev/null 2>&1
  for i in 1 2 3 4 5 6; do $C/audio_nulo.sh ligar horizon >/dev/null; $C/audio_nulo.sh verificar horizon && break; sleep 4; done
  say "audio $($C/audio_nulo.sh seguro horizon)"
  ctl state | grep -q '"gamemode"' || { say "o jogo não abriu"; exit 1; }
}
X=3; Y=-60; Z=3; L=232; T=65
por_mesa() { # a mesa em (X, Y, Z), posta pelo jogador de (X, Y, 0) olhando para o sul: fica voltada para o norte
  cmd "clear Dev" 80; cmd "tp Dev $X.5 $Y.0 0.5 0 0" 80; cmd "give Dev mekanism:security_desk 1" 80; sleep 0.5
  for i in 1 2 3 4; do out=$(ctl place $X $Y $Z --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "por: $(echo $out | head -c 160)"
  cmd "clear Dev" 80; sleep 0.5
}
bloco() { say "$1: $(ctl block $2 $3 $4 | tr -d '\n ' | head -c 300)"; }
abrir() { for i in 1 2 3 4; do ctl interact $X ${1:-$Y} $Z ${2:-north} >/dev/null; sleep 0.8; ctl screen | tr -d ' ' | grep -q '"screen":"container"' && { say "tela aberta por $X ${1:-$Y} $Z"; return 0; }; done; say "a tela não abriu por $X ${1:-$Y} $Z: $(ctl screen | tr -d '\n ' | head -c 200)"; return 1; }
itens() { say "$1: $(ctl command "data get block $X $Y $Z" | tr -d '\n' | grep -o 'Items: \[.*' | head -c 900)"; }
inv() { say "$1: $(ctl inventory | tr -d '\n ' | head -c ${2:-700})"; }
slot_de() { ctl inventory | python3 -I -c "import json,sys; d=json.load(sys.stdin); print(next((s['slot'] for k in ('hotbar','inventory','main') for s in d.get(k,[]) if s.get('item')=='$1'),''))" 2>/dev/null; }
soltos() { say "$1: $(ctl command 'data get entity @e[type=item,limit=1,sort=nearest,distance=..12]' | tr -d '\n' | head -c 1200)"; }

if tem inicio || [ ! -d "$D/saves/$MUNDO" ]; then
  rm -rf "$D/saves/$MUNDO"; cp -a --reflink=auto "$D/saves/$BASE" "$D/saves/$MUNDO"; rm -rf "$D/saves/$MUNDO/data/mekanism/frequency"
fi
abrir_jogo
for c in "time set noon" "weather clear" "gamerule advance_time false" "gamerule advance_weather false" "difficulty peaceful" \
  "fill -3 -61 -6 9 -50 10 minecraft:air" "fill -3 -61 -6 9 -50 10 minecraft:black_concrete hollow" "gamemode creative Dev" "kill @e[type=item]"; do cmd "$c" 80; sleep 0.4; done
ctl options-set guiScale 3 >/dev/null

if tem apoio; then
  por_mesa
  get "nbt mesa" "data get block $X $Y $Z"
  get "nbt apoio" "data get block $X $((Y+1)) $Z"
  bloco "bloco apoio" $X $((Y+1)) $Z
  # A mira: os olhos em (3.5, -58.38, 0.5); o monitor da mesa voltada para o norte fica em z de 3.6875 a 3.8125 e y de -59.125 a -58.5.
  ctl look-at $X.5 -58.7 3.7 >/dev/null; sleep 0.5; say "mira em cima: $(ctl looking-at | tr -d '\n ' | head -c 300)"
  ctl look-at $X.5 -59.6 3.0 >/dev/null; sleep 0.5; say "mira embaixo: $(ctl looking-at | tr -d '\n ' | head -c 300)"
  # O clique no apoio abre a tela do principal.
  abrir $((Y+1)) && { ctl screenshot $D/../shots/final_port_tela_pelo_apoio.png >/dev/null 2>&1; ctl close; }; sleep 0.5
  abrir && ctl close; sleep 0.5
  # O pick no apoio (a tecla do pick trocada para P: o horizon-ctl não manda o clique do meio).
  ctl options-set key_key.pickItem key.keyboard.p >/dev/null; ctl look-at $X.5 -58.7 3.7 >/dev/null; sleep 0.4
  ctl key p tap >/dev/null; sleep 0.6; inv "inventário depois do pick no apoio" 400
  ctl options-set key_key.pickItem key.mouse.middle >/dev/null; cmd "clear Dev" 80
  # A colisão: um item solto sobre o monitor (repouso em y = -58.5) e um sobre a frente da mesa (y = -59.1875).
  cmd "summon item $X.5 -57.5 3.75 {Item:{id:\"minecraft:stone\",count:1},PickupDelay:32767}" 80; sleep 3
  get "item sobre o monitor" "data get entity @e[type=item,limit=1,sort=nearest,distance=..12] Pos" 200; cmd "kill @e[type=item]" 80
  cmd "summon item $X.5 -57.5 3.2 {Item:{id:\"minecraft:stone\",count:1},PickupDelay:32767}" 80; sleep 3
  get "item sobre a frente" "data get entity @e[type=item,limit=1,sort=nearest,distance=..12] Pos" 200; cmd "kill @e[type=item]" 80
fi

if tem itens; then
  [ "$(ctl block $X $Y $Z | grep -c security_desk)" = 0 ] && por_mesa
  # Barra: 0 Crusher sem dono, 1 pedra, 2 Heat Generator. Os slots da mesa: 300 (destravar) e 301 (travar).
  cmd "clear Dev" 80; cmd "item replace entity Dev hotbar.0 with mekanism:crusher" 80; cmd "item replace entity Dev hotbar.1 with minecraft:stone" 80; cmd "item replace entity Dev hotbar.2 with mekanismgenerators:heat_generator" 80
  abrir; sleep 0.5
  ctl click-slot 0 shift >/dev/null; sleep 1.2; itens "shift no Crusher sem dono"; inv "inventário" 500
  ctl click-slot 1 shift >/dev/null; sleep 0.6; itens "shift na pedra"; inv "inventário" 500
  ctl click-slot 301 shift >/dev/null; sleep 0.6; itens "shift no slot de travar (volta)"; inv "inventário (Crusher com dono)" 900
  # O Crusher voltou com dono: o shift agora o leva ao slot de destravar, que tira o dono.
  S=$(slot_de mekanism:crusher); say "o Crusher voltou no slot ${S:-?}"
  ctl click-slot ${S:-0} shift >/dev/null; sleep 1.2; itens "shift no Crusher do dono"
  ctl click-slot 300 shift >/dev/null; sleep 0.6; inv "inventário (Crusher destravado)" 900
  # O Heat Generator: trava e destrava.
  S=$(slot_de mekanismgenerators:heat_generator)
  ctl click-slot ${S:-2} shift >/dev/null; sleep 1.2; itens "shift no Heat Generator sem dono"
  ctl click-slot 301 shift >/dev/null; sleep 0.6
  S=$(slot_de mekanismgenerators:heat_generator)
  inv "inventário (Heat Generator com dono)" 900
  ctl click-slot ${S:-2} shift >/dev/null; sleep 1.2; itens "shift no Heat Generator do dono"
  ctl click-slot 300 shift >/dev/null; sleep 0.6; inv "inventário (Heat Generator destravado)" 900
  ctl close; sleep 0.4
fi

if tem solto; then
  [ "$(ctl block $X $Y $Z | grep -c security_desk)" = 0 ] && por_mesa
  # Um Crusher sem dono no slot de travar, e a mesa quebrada em sobrevivência: o item solto leva `mekanism:items`.
  cmd "clear Dev" 80; cmd "item replace entity Dev hotbar.0 with mekanism:crusher" 80
  abrir; sleep 0.4; ctl click-slot 0 shift >/dev/null; sleep 1.2; ctl close; itens "antes de quebrar"
  cmd "kill @e[type=item]" 80; cmd "gamemode survival Dev" 80; cmd "tp Dev $X.5 $Y.0 -3.5 0 0" 80; cmd "item replace entity Dev hotbar.0 with minecraft:netherite_pickaxe" 80; sleep 0.5
  cmd "tp Dev $X.5 $Y.0 1.5 0 0" 80; sleep 0.3
  say "quebrar: $(ctl break $X $Y $Z | tr -d '\n' | head -c 200)"; sleep 1.5
  bloco "mesa depois de quebrar" $X $Y $Z; bloco "apoio depois de quebrar" $X $((Y+1)) $Z
  say "itens soltos: $(ctl command 'execute if entity @e[type=item,distance=..12]' | tr -d '\n' | head -c 120)"
  soltos "item solto"
  cmd "tp Dev $X.5 $Y.0 3.5 0 0" 80; sleep 1.5; inv "inventário com a mesa" 1500
  cmd "gamemode creative Dev" 80; cmd "tp Dev $X.5 $Y.0 0.5 0 0" 80; sleep 0.4
  for i in 1 2 3 4; do out=$(ctl place $X $Y $Z --item mekanism:security_desk); echo "$out" | grep -q '"error"' || break; sleep 0.6; done; say "recolocar: $(echo $out | head -c 160)"; sleep 1
  itens "recolocada"; cmd "clear Dev" 80
fi

if tem configurator; then
  [ "$(ctl block $X $Y $Z | grep -c security_desk)" = 0 ] && por_mesa
  conf() { cmd "clear Dev" 60; cmd "give Dev mekanism:configurator[mekanism:configurator_mode=\"$1\"] 1" 60; sleep 0.4; ctl select mekanism:configurator >/dev/null; }
  clique() { [ "$3" = agachado ] && ctl key shift down >/dev/null; ctl interact $X $1 $Z $2 >/dev/null; [ "$3" = agachado ] && ctl key shift up >/dev/null; sleep 0.6; }
  conf rotate
  bloco "facing no começo" $X $Y $Z
  for i in 1 2 3 4; do clique $Y north; bloco "ROTATE $i" $X $Y $Z; done
  clique $((Y+1)) north; bloco "ROTATE pelo apoio" $X $Y $Z; get "nbt apoio depois de girar" "data get block $X $((Y+1)) $Z" 300
  clique $Y west agachado; bloco "ROTATE agachado na face oeste" $X $Y $Z
  clique $Y west agachado; bloco "ROTATE agachado de novo na face oeste" $X $Y $Z
  clique $Y up agachado; bloco "ROTATE agachado na face de cima" $X $Y $Z
  # EMPTY agachado solta os dois slots.
  cmd "clear Dev" 60; cmd "item replace entity Dev hotbar.0 with mekanism:crusher" 80; abrir; sleep 0.4; ctl click-slot 0 shift >/dev/null; sleep 1.2; ctl close; sleep 0.3
  ctl screen | tr -d ' ' | grep -q '"screen":"container"' && ctl close
  cmd "kill @e[type=item]" 60; conf empty; clique $Y north; say "EMPTY sem agachar: tela $(ctl screen | tr -d '\n ' | head -c 80)"; ctl close; sleep 0.3
  clique $Y north agachado; itens "EMPTY agachado"; soltos "solto pelo EMPTY"; cmd "kill @e[type=item]" 60
  # WRENCH agachado desmonta.
  conf wrench; clique $Y north agachado; bloco "mesa depois do WRENCH" $X $Y $Z; bloco "apoio depois do WRENCH" $X $((Y+1)) $Z; soltos "solto pelo WRENCH"; inv "inventário depois do WRENCH" 900
  cmd "kill @e[type=item]" 60; cmd "clear Dev" 60
fi

if tem remocao; then
  [ "$(ctl block $X $Y $Z | grep -c security_desk)" = 0 ] && por_mesa
  cmd "kill @e[type=item]" 60
  cmd "setblock $X $((Y+1)) $Z minecraft:air" 80; sleep 0.6; bloco "mesa com o apoio trocado por ar" $X $Y $Z; say "itens soltos: $(ctl command 'execute if entity @e[type=item,distance=..12]' | tr -d '\n' | head -c 120)"
  cmd "setblock $X $Y $Z minecraft:air" 60; por_mesa
  cmd "setblock $X $((Y+1)) $Z minecraft:stone" 80; sleep 0.6; bloco "mesa com o apoio trocado por pedra" $X $Y $Z; bloco "o lugar do apoio" $X $((Y+1)) $Z; say "itens soltos: $(ctl command 'execute if entity @e[type=item,distance=..12]' | tr -d '\n' | head -c 120)"
  cmd "setblock $X $((Y+1)) $Z minecraft:air" 60; cmd "setblock $X $Y $Z minecraft:air" 60
fi

if tem explosao; then
  cmd "setblock $X $Y $Z minecraft:air" 60; cmd "setblock $X $((Y+1)) $Z minecraft:air" 60; por_mesa; cmd "kill @e[type=item]" 60
  cmd "tp Dev $X.5 $Y.0 -4.5 0 0" 80; get "regra" "gamerule tnt_explodes" 120
  # O controle: uma TNT longe da mesa (o chão sob ela sai). O `fuse` do comando não vale no port: a TNT estoura em 4 s.
  cmd "summon minecraft:tnt 8.5 $Y.0 -5.5 {fuse:10}" 80; sleep 6
  bloco "chão sob a TNT de controle" 8 $((Y-1)) -6; bloco "mesa depois da TNT de controle" $X $Y $Z
  # Uma TNT na altura do apoio, a 2 blocos, do lado leste. Depois, outra mesa com a TNT 3 blocos acima do apoio (só o apoio no caminho dos raios de cima).
  cmd "summon minecraft:tnt $((X+2)).5 $((Y+1)).0 $Z.5 {fuse:10}" 80; sleep 6
  say "tnt viva: $(ctl command 'execute if entity @e[type=tnt]' | tr -d '\n' | head -c 100)"
  bloco "chão sob a TNT" $((X+2)) $((Y-1)) $Z
  bloco "mesa depois da TNT" $X $Y $Z; bloco "apoio depois da TNT" $X $((Y+1)) $Z
  say "itens soltos: $(ctl command 'execute if entity @e[type=item,distance=..14]' | tr -d '\n' | head -c 120)"; get "a mesa solta pela explosão" 'data get entity @e[type=item,limit=1,nbt={Item:{id:"mekanism:security_desk"}}] Item' 700
  say "mesas soltas: $(ctl command 'execute if entity @e[type=item,nbt={Item:{id:"mekanism:security_desk"}}]' | tr -d '\n' | head -c 100)"
  cmd "kill @e[type=item]" 60; cmd "fill -3 -61 -6 9 -50 10 minecraft:air" 80; cmd "fill -3 -61 -6 9 -50 10 minecraft:black_concrete hollow" 80
fi

if tem reabrir; then
  cmd "setblock $X $Y $Z minecraft:air" 60; por_mesa; ctl quit; sleep 8; abrir_jogo; sleep 2
  cmd "tp Dev $X.5 $Y.0 0.5 0 0" 80; sleep 1
  get "nbt apoio (reaberto)" "data get block $X $((Y+1)) $Z" 300
  ctl look-at $X.5 -58.7 3.7 >/dev/null; sleep 0.5; say "mira em cima (reaberto): $(ctl looking-at | tr -d '\n ' | head -c 300)"
  abrir $((Y+1)) && ctl close
fi

if tem disco; then
  # Dois nomes com 25 s entre eles (um nível parado fecha uns 15 s depois de o mundo abrir): os dois têm de estar no arquivo ao fechar.
  nome() { ctl click-at $((L+60)) $((T+73)) >/dev/null; for ch in $(echo "$1" | grep -o .); do ctl type "$ch" >/dev/null; sleep 0.05; done; sleep 0.3; ctl key enter tap >/dev/null; say "nome $1"; }
  F="$D/saves/$MUNDO/data/mekanism/frequency/security/public.dat"
  [ "$(ctl block $X $Y $Z | grep -c security_desk)" = 0 ] && por_mesa
  abrir; sleep 0.5; nome zz_disco_um; sleep 25; say "arquivo no meio: $(stat -c '%y %s' "$F" 2>&1 | head -c 80)"; nome zz_disco_dois; sleep 3; ctl close
fi
cmd "gamerule advance_time true" 60; cmd "gamerule advance_weather true" 60
ctl quit; sleep 8
say "fechado"
if tem disco; then say "freq disco: $(python3 -I $C/nbt_ler.py "$F" 2>&1 | tail -1 | head -c 2000)"; fi
say "log: $(grep -a "panicked\|machinery_basis: \|mekanism: " game.log | tail -12 | cut -c1-300)"
