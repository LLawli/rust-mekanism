#!/usr/bin/env python3
"""A radiação no jogo (frente `radiacao`, 2.ª rodada): o mesmo roteiro no port (`horizon-ctl`) e no original (`testes/CenaDois.java`, `oficial_dois.sh`).

    H_NAME=horizon h2_radiacao.py port PARTE SAIDA.jsonl     # roda a PARTE no port aberto e grava uma linha JSON por passo
    h2_radiacao.py roteiro PARTE SAIDA.txt                   # o roteiro da `CenaDois` para a mesma PARTE (o anfitrião sozinho, sem LAN)

PARTE:
  fontes   (com o servidor parado) duas fontes por comando, o nível lido em vários pontos e em outra dimensão, os erros de argumento, o arquivo gravado;
  decai    o servidor andando 40 s: as fontes decaem (o arquivo antes e depois);
  quebra   um Chemical Tank básico com polônio quebrado pelo nível, e o criativo: a fonte de cada um e o item que cai;
  dose     o jogador em sobrevivência a 1 bloco de uma fonte de 0,1 Sv/h: a dose e a vida a cada segundo, com e sem o traje de hazmat, e longe da fonte.
O mundo de cada jogo é uma cópia (o port: `saves/radiacao`, copiado do `limpo`; o original: o `GameTestWorld` com backup, devolvido ao fim). A plataforma fica em
y = 150 (céu), em volta de (0, 150, 0). Toda fonte sai no fim (`removeAll`) e o jogador volta ao criativo, curado.
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, TOOLS)
os.environ.setdefault("H_NAME", "horizon")

Y = 150
SOURCE = (0, Y, 0)

PREPARO = [
    ("srv", "time set noon"), ("srv", "weather clear"), ("srv", "gamerule advance_time false"), ("srv", "gamerule advance_weather false"),
    ("srv", "difficulty peaceful"), ("srv", "gamemode creative @p"), ("srv", f"tp @p 0.5 {Y} 0.5 0 0"), ("espera", 40),
    # O chunk da plataforma só está carregado depois do `tp` (no mundo do original o jogador começa longe): a plataforma e o `tp` de novo.
    ("srv", f"fill -3 {Y - 1} -3 3 {Y - 1} 3 minecraft:glass"), ("srv", f"fill -3 {Y - 1} 47 3 {Y - 1} 53 minecraft:glass"),
    ("srv", f"tp @p 0.5 {Y} 0.5 0 0"), ("espera", 20), ("msg", "limpa", "mek radiation removeAll"), ("msg", "cura", "mek radiation heal"),
]

FONTES = PREPARO + [
    ("srv", "tick freeze"),
    ("msg", "add_a", f"mek radiation add 2 0 {Y} 0"),
    ("msg", "add_b", f"mek radiation add 0.5 3 {Y} 4"),
    ("msg", "add_c", f"mek radiation add 0.000002 -4 {Y} 7.6"),
    ("msg", "add_d", f"mek radiation add 2500 ~-30 ~ ~"),
] + [
    ("msg", f"get_{dx}_{dz}", f"mek radiation get {dx} {Y} {dz}")
    for dx, dz in [(0, 0), (1, 0), (2, 0), (3, 4), (5, 0), (10, 0), (40, 0), (79, 0), (80, 0), (81, 0), (85, 0), (-30, 0), (-110, 0), (-111, 0)]
] + [
    ("msg", "get_aqui", "mek radiation get"),
    ("msg", "get_nether", f"mek radiation get 0 {Y} 0 minecraft:the_nether"),
    ("msg", "get_local", "mek radiation get ^ ^ ^2"),
    ("msg", "erro_zero", "mek radiation add 0"),
    ("msg", "erro_grande", "mek radiation add 20000"),
    ("msg", "erro_incompleto", f"mek radiation get 0 {Y}"),
    ("msg", "erro_dimensao", f"mek radiation get 0 {Y} 0 minecraft:nada"),
    ("arquivo", "fontes"),
]

DECAI = [("srv", "tick unfreeze"), ("espera", 800), ("srv", "tick freeze"), ("arquivo", "decaido")] + [
    ("msg", f"decai_get_{dx}", f"mek radiation get {dx} {Y} 0") for dx in (0, 2)
] + [("msg", "limpa_fim", "mek radiation removeAll"), ("srv", "tick unfreeze")]

TANK = '{chemical_tanks:[{container:0b,stored:{type:"mekanism:polonium",amount:1000L}}]}'
# Os tanques ficam a 48 e 52 blocos do jogador: no cliente do original, um Chemical Tank não criativo com químico radioativo a até 32 blocos leva o
# `BlockMekanism.animateTick` a pedir `Integer.MAX_VALUE` partículas e o jogo acaba sem memória (medido em 2026-11-22).
# O tubo não entra: nenhum dos dois jogos enche a rede de um tubo pelo NBT (`data merge` não chega ao buffer da rede), e o despejo do tubo segue conferido pelo
# cenário `radioactive` da rede de químico (decisão 130).
QUEBRA = PREPARO + [
    ("srv", f"setblock -2 {Y} 52 mekanism:basic_chemical_tank"),
    ("srv", f"setblock -2 {Y} 48 mekanism:creative_chemical_tank"),
    ("espera", 10),
    ("srv", f"data merge block -2 {Y} 52 {TANK}"),
    ("srv", f"data merge block -2 {Y} 48 {TANK}"),
    ("espera", 10),
    ("msg", "tanque_nbt", f"data get block -2 {Y} 52 chemical_tanks"),
    ("srv", f"setblock -2 {Y} 52 minecraft:air destroy"),
    ("espera", 10),
    ("msg", "tanque", f"mek radiation get -2 {Y} 52"),
    ("msg", "item_basico", f"data get entity @e[type=minecraft:item,limit=1,sort=nearest,x=-2,y={Y},z=52] Item"),
    ("srv", "kill @e[type=minecraft:item]"),
    ("espera", 5),
    ("srv", f"setblock -2 {Y} 48 minecraft:air destroy"),
    ("espera", 10),
    ("msg", "criativo", f"mek radiation get -2 {Y} 48"),
    ("msg", "item_criativo", f"data get entity @e[type=minecraft:item,limit=1,sort=nearest,x=-2,y={Y},z=48] Item"),
    ("arquivo", "quebra"),
    ("srv", "kill @e[type=minecraft:item]"),
    ("msg", "limpa_fim", "mek radiation removeAll"),
]

DOSE_PATH = 'neoforge:attachments."mekanism:radiation".radiation'


def amostras(prefixo, n, passo=20):
    out = []
    for i in range(n):
        out += [("espera", passo), ("teste", f"{prefixo}_dose_{i}", f"data get entity @p {DOSE_PATH} 1000000000"),
                ("teste", f"{prefixo}_vida_{i}", "data get entity @p Health 1000")]
    return out


DOSE = PREPARO + [
    ("msg", "fonte", f"mek radiation add 0.1 1 {Y} 0"),
    ("srv", "gamemode survival @p"),
] + amostras("exposto", 25) + [
    ("srv", "item replace entity @p armor.head with mekanism:hazmat_mask"),
    ("srv", "item replace entity @p armor.chest with mekanism:hazmat_gown"),
    ("srv", "item replace entity @p armor.legs with mekanism:hazmat_pants"),
    ("srv", "item replace entity @p armor.feet with mekanism:hazmat_boots"),
    ("msg", "reduz_antes_traje", "mek radiation reduce 10000"),
] + amostras("traje", 15) + [
    ("srv", "item replace entity @p armor.head with minecraft:air"),
    ("srv", "item replace entity @p armor.chest with minecraft:air"),
    ("srv", "item replace entity @p armor.legs with minecraft:air"),
    ("srv", "item replace entity @p armor.feet with minecraft:air"),
    ("msg", "dose_longe", "mek radiation addEntity 0.05"),
    ("srv", f"fill -1 {Y - 1} 99 1 {Y - 1} 101 minecraft:glass"),
    ("srv", f"tp @p 0.5 {Y} 100.5"),
] + amostras("longe", 15) + [
    ("srv", f"tp @p 0.5 {Y} 0.5"),
    ("srv", "gamemode creative @p"),
    ("msg", "cura_fim", "mek radiation heal"),
    ("msg", "limpa_fim", "mek radiation removeAll"),
    ("srv", "effect give @p minecraft:instant_health 1 10 true"),
]

PARTES = {"fontes": FONTES, "decai": DECAI, "quebra": QUEBRA, "dose": DOSE}
FILE = "~/.local/share/jogo-isolado/horizon/data/horizon/saves/radiacao/dimensions/minecraft/overworld/data/neoforge/data_attachments.dat"


def port(parte, saida):
    import h2_identity as h
    with open(saida, "a") as out:
        def put(row):
            out.write(json.dumps(row, ensure_ascii=False) + "\n")
            out.flush()
        for passo in PARTES[parte]:
            kind = passo[0]
            if kind == "srv":
                h.command(passo[1])
            elif kind == "espera":
                time.sleep(passo[1] / 20 + 0.2)
            elif kind == "msg":
                t0 = time.time()
                chat = h.command(passo[2])
                # O `chat` do `command` traz as chaves; o `styled` de `messages` traz o componente inteiro (do mais novo ao mais velho).
                styled = json.loads(h.ctl("messages", str(max(1, len(chat))))).get("styled", []) if chat else []
                put({"passo": passo[1], "comando": passo[2], "chat": chat, "styled": list(reversed(styled)), "t": round(time.time() - t0, 2)})
            elif kind == "teste":
                chat = h.command(passo[2])
                numero = None
                for line in chat:
                    m = re.search(r"(?:following entity data:| is) (-?\d+)$", line)
                    if m:
                        numero = int(m.group(1))
                put({"passo": passo[1], "numero": numero, "chat": chat})
            elif kind == "arquivo":
                h.ctl("save")
                time.sleep(2)
                raw = subprocess.run(["ssh", "desktop", f"python3 ~/.cache/rust-mekanism/nbt_ler.py {FILE} | head -c 4000"], capture_output=True, text=True).stdout
                put({"passo": "arquivo_" + passo[1], "arquivo": raw.strip()})


def roteiro(parte, saida):
    with open(saida, "w") as out:
        out.write(f"# h2_radiacao.py roteiro {parte}\nespera 80\n")
        for passo in PARTES[parte]:
            kind = passo[0]
            if kind == "srv":
                out.write(f"srv {passo[1]}\n")
            elif kind == "espera":
                out.write(f"espera {passo[1]}\n")
            elif kind == "msg":
                out.write(f"msg {passo[1]} {passo[2]}\nespera 2\n")
            elif kind == "teste":
                out.write(f"teste {passo[1]} {passo[2]}\nespera 1\n")
            elif kind == "arquivo":
                out.write("srv save-all flush\nespera 40\n")
        out.write("fim\n")


def main():
    acao = sys.argv[1]
    if acao == "port":
        port(sys.argv[2], sys.argv[3])
    elif acao == "roteiro":
        roteiro(sys.argv[2], sys.argv[3])
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main()
