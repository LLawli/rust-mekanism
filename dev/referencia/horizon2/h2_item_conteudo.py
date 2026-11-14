#!/usr/bin/env python3
"""Em jogo (container `horizon2`, H_NAME=horizon2): o conteúdo guardado no item da máquina (frente `item-conteudo`, Fase 5).

    h2_item_conteudo.py [DIRETORIO_DOS_PRINTS]

Para cada máquina (a Enrichment Chamber, o Crusher, o Energized Smelter, o Metallurgic Infuser e o Heat Generator): dá o item com os componentes que o original solta
(`mekanism:upgrades`, `items`, `chemicals` ou `fluids` e `heat`, `redstone_control`, `energy`), põe o bloco, confere o que o tile guardou (`component_upgrade`, `items`, o tanque, o
modo de redstone), quebra com a picareta e confere o item solto (os componentes e nenhum item solto a mais no chão), e põe o item solto de volta. Cada máquina tira um print do
tooltip do item na tela do inventário. A saída é `legendas.json` na pasta. O jogo aberto num mundo, em criativo, sem som e headless (regra de builds e jogo).
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("H_NAME", "horizon2")
NAME = os.environ["H_NAME"]
SHOTS_HOST = f"/home/luka/.local/share/jogo-isolado/{NAME}/data/shots/item_conteudo"
# Dentro do container o caminho é sempre o do `horizon`.
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/item_conteudo"
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/.cache/rust-mekanism/wa/item-conteudo")
captions = []
failures = []

UP = ('mekanism:upgrades={upgrades:{"mekanism:speed":3,"mekanism:energy":2},'
      'input:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:speed"}},amount:5L},'
      'output:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:energy"}},amount:7L}}')
UP_MUFFLING = ('mekanism:upgrades={upgrades:{"mekanism:muffling":1},'
               'input:{type:{id:"mekanism:upgrade",components:{"mekanism:upgrade_type":"mekanism:muffling"}},amount:3L},output:{}}')
COBBLE = '{type:{id:"minecraft:cobblestone"},amount:17L}'
IRON = '{type:{id:"minecraft:iron_ingot"},amount:9L}'
REDSTONE = '{type:{id:"minecraft:redstone"},amount:5L}'
COAL = '{type:{id:"minecraft:coal"},amount:17L}'
COMMON = 'mekanism:redstone_control="high",mekanism:energy=200L'
MACHINES = [
    ("mekanism:enrichment_chamber", (2, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:crusher", (6, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:energized_smelter", (10, 101, 0), f"{UP},mekanism:items={{containers:[{COBBLE},{IRON},{REDSTONE}]}},{COMMON}", "Speed", 3),
    ("mekanism:metallurgic_infuser", (2, 101, 4),
     f'{UP},mekanism:items={{containers:[{{}},{COBBLE},{IRON},{REDSTONE}]}},mekanism:chemicals={{containers:[{{type:"mekanism:bio",amount:150L}}]}},{COMMON}', "Speed", 3),
    ("mekanismgenerators:heat_generator", (6, 101, 4),
     f'{UP_MUFFLING},mekanism:items={{containers:[{COAL},{REDSTONE}]}},mekanism:fluids={{containers:[{{type:{{id:"minecraft:lava"}},amount:1500L}}]}},'
     f'mekanism:heat={{heat:4000.0d,heat_capacity:10.0d}},mekanism:redstone_control="high",mekanism:energy=200L', "Muffling", 1),
]


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    try:
        return " ".join(json.loads(ctl("command", text))["chat"])
    except (json.JSONDecodeError, KeyError):
        return ""


def widgets():
    return json.loads(ctl("ui"))


def shot(name, caption):
    time.sleep(0.8)
    ctl("screenshot", f"{SHOTS}/{len(captions):02d}_{name}.png")
    captions.append({"file": f"{len(captions):02d}_{name}.png", "caption": caption})
    print(caption)


def expect(label, ok, detail=""):
    print(("ok   " if ok else "FALHOU"), label, detail if not ok else "")
    if not ok:
        failures.append(label)


def main():
    os.makedirs(OUT, exist_ok=True)
    subprocess.run(["ssh", "desktop", f"rm -rf {SHOTS_HOST}; mkdir -p {SHOTS_HOST}"], check=True)
    ctl("wait-ready")
    time.sleep(5)
    cmd("gamemode creative")
    cmd("tp @s 4 102 -4 0 20")
    cmd("fill -4 100 -8 16 100 10 minecraft:stone")
    cmd("fill -4 101 -8 16 106 10 minecraft:air")
    cmd("kill @e[type=minecraft:item]")
    cmd("clear @s")
    for item, pos, components, upgrade_label, upgrade_total in MACHINES:
        short = item.split(":")[1]
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        cmd("clear @s")
        cmd(f"give @s {item}[{components}]")
        ctl("select", "1")
        # o tooltip do item com o conteúdo, na tela do inventário
        cmd("gamemode survival")
        ctl("open-inventory")
        time.sleep(1.0)
        slots = [w for w in widgets().get("widgets", []) if w.get("kind") == "slot" and (w.get("stack") or {}).get("item") == item]
        if slots:
            x, y, w, h = slots[0]["bounds"]
            ctl("scroll-at", str(int(x + 8)), str(int(y + 8)), "0")
            shot(f"tooltip_{short}", f"Tooltip do item {short} com upgrades, itens, tanque e modo de redstone")
        else:
            failures.append(f"sem slot de {item}")
        ctl("close")
        cmd("gamemode creative")
        ctl("select", "1")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        ctl("select", "1")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        print(ctl("place", *map(str, pos), "--item", item))
        time.sleep(1.5)
        tile = cmd("data get block {} {} {}".format(*pos))
        expect(f"{short}: o tile guarda os upgrades do item", f'"mekanism:speed": {upgrade_total}' in tile or f'mekanism:speed: {upgrade_total}' in tile or f"{upgrade_label.lower()}" in tile.lower(), tile[:400])
        expect(f"{short}: o tile guarda o modo de redstone", "high" in tile, tile[:400])
        expect(f"{short}: o tile guarda os itens", ("cobblestone" in tile) or ("coal" in tile), tile[:400])
        cmd("tp @s 4 102 -4 0 20")
        shot(f"posta_{short}", f"{short} posta pelo item: o tile traz o conteúdo")
        # quebrar em sobrevivência com a picareta
        cmd("gamemode survival")
        cmd("give @s minecraft:diamond_pickaxe")
        ctl("select", "minecraft:diamond_pickaxe")
        cmd("kill @e[type=minecraft:item]")
        ctl("tp", str(pos[0]), "101", str(pos[2] + 3))
        print(ctl("break", *map(str, pos)))
        time.sleep(1.2)
        item_nbt = cmd("data get entity @e[type=minecraft:item,limit=1,sort=nearest] Item")
        expect(f"{short}: o item solto leva os upgrades", "mekanism:upgrades" in item_nbt, item_nbt[:400])
        expect(f"{short}: o item solto leva os itens", "mekanism:items" in item_nbt, item_nbt[:400])
        expect(f"{short}: o item solto leva o modo de redstone", "mekanism:redstone_control" in item_nbt, item_nbt[:400])
        if "infuser" in short:
            expect(f"{short}: o item solto leva o tanque", "mekanism:chemicals" in item_nbt, item_nbt[:400])
        if "heat" in short:
            expect(f"{short}: o item solto leva a lava e o calor", "mekanism:fluids" in item_nbt and "mekanism:heat" in item_nbt, item_nbt[:400])
        shot(f"quebrada_{short}", f"{short} quebrada: o item solto leva o conteúdo e nada cai no chão")
        # nenhum item solto a mais: o `kill` conta o que está no chão (a máquina e mais nada), e a pedra, o ferro, o carvão e a redstone ficaram no item
        killed = cmd("kill @e[type=minecraft:item]")
        expect(f"{short}: só o item da máquina está no chão", killed.startswith("Killed ") and "entities" not in killed, killed)
        # repor o item solto
        cmd("gamemode creative")
        cmd("clear @s mekanism:" + short.replace("mekanismgenerators:", ""))
        cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        cmd("kill @e[type=minecraft:item]")
    json.dump(captions, open(os.path.join(OUT, "legendas.json"), "w"), ensure_ascii=False, indent=1)
    print("falhas:", failures)


main()
