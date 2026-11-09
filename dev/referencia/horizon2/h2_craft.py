#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): os itens e blocos de craft e de recurso do Mekanism.

Uso: H_NAME=horizon h2_craft.py items|mining|recipes|worldgen   (cada parte repete uma coisa: `give` de cada item e bloco; os minérios minerados com fortuna e a experiência da fluorita;
uma receita de mesa (o bloco de estanho), uma de forno (o minério de estanho) e uma de máquina (o minério de estanho no Enrichment Chamber); o worldgen dos minérios novos e do sal)
"""
import json, os, re, sys, time

import h2_identity as h

BASE = (70, -60, 60)


def cmd(text):
    return h.command(text)


def ctl_json(*args):
    out = h.ctl(*args)
    return json.loads(out) if out.strip().startswith(("{", "[")) else out


def items():
    """Todo item e todo bloco do conteúdo responde ao `give`: o jogo conhece o id."""
    names = sorted(f[:-5] for kind in ("item", "block") for f in os.listdir(f"{h.TOOLS}/../../../mods/mekanism/content/data/mekanism/horizon/{kind}"))
    bad = []
    for name in names:
        chat = cmd(f"give @s mekanism:{name} 1")
        text = " ".join(chat) if isinstance(chat, list) else str(chat)
        if "Unknown" in text or "No item" in text or "error" in text.lower():
            bad.append((name, text[:120]))
    print(f"give: {len(names)} ids, {len(bad)} recusados {bad}")
    return not bad


def points():
    chat = cmd("experience query @s points")[0]
    return int(re.search(r"has (\d+) experience points", chat).group(1))


def mining():
    """Cada minério quebrado seis vezes com uma picareta de diamante com fortuna III, em sobrevivência: o que cai (`drops` do `horizon-ctl break`) e a experiência que o jogador
    ganha (`experience query`). O original: o metal solta o bruto com fortuna e nenhuma experiência; a fluorita solta de 2 a 4 gemas antes da fortuna e de 1 a 4 de experiência."""
    cmd("tick unfreeze")
    cmd("gamemode survival")
    cmd("difficulty peaceful")
    h.ctl("tp", "70", "-60", "57")
    cmd("clear @s")
    cmd('give @s minecraft:diamond_pickaxe[enchantments={"minecraft:fortune":3}] 1')
    h.ctl("select", "1")
    ok = True
    for block, drop in [("tin_ore", "raw_tin"), ("lead_ore", "raw_lead"), ("uranium_ore", "raw_uranium"), ("osmium_ore", "raw_osmium"), ("deepslate_tin_ore", "raw_tin"),
                        ("fluorite_ore", "fluorite_gem"), ("deepslate_fluorite_ore", "fluorite_gem")]:
        counts, xps = [], []
        for _ in range(8):
            cmd("kill @e[type=minecraft:item]")
            cmd("experience set @s 0 points")
            cmd("setblock 70 -60 60 minecraft:air")
            cmd(f"setblock 70 -60 60 mekanism:{block}")
            out = json.loads(h.ctl("break", "70", "-60", "60"))
            counts.append(out["drops"].get(f"mekanism:{drop}", 0))
            time.sleep(1.5)
            xps.append(points())
        gem = block.endswith("fluorite_ore")
        # a experiência cai como orbes que o jogador recolhe: 1 a 4 em cada quebra da fluorita, 0 nos metais
        fine = all(1 <= x <= 4 for x in xps) if gem else all(x == 0 for x in xps)
        fine = fine and all(c >= 1 for c in counts)
        ok &= fine
        print(("OK   " if fine else "FALHOU ") + f"{block}: {drop} por quebra {counts}; experiência {xps}")
    cmd("gamemode creative")
    return ok


def shot(name):
    time.sleep(0.8)
    h.ctl("screenshot", f"/home/luka/.local/share/jogo-isolado/horizon/data/horizon/shots_itens/{name}.png")


def table_recipe():
    """Mesa de trabalho: nove `ingot_tin` dão um `block_tin` (`storage_blocks/tin`), e o bloco volta a nove lingotes (`processing/tin/ingot/from_block`)."""
    cmd("gamemode survival")
    cmd("clear @s")
    cmd("give @s mekanism:ingot_tin 9")
    cmd("setblock 72 -60 60 minecraft:crafting_table")
    h.ctl("tp", "72", "-60", "58")
    time.sleep(0.5)
    h.ctl("interact", "72", "-60", "60")
    time.sleep(0.6)
    h.ctl("click-slot", "0")
    for slot in range(100, 109):
        h.ctl("click-slot", str(slot), "right")
    time.sleep(0.8)
    menu = json.loads(h.ctl("menu"))
    result = next((s["stack"] for s in menu["slots"] if s["slot"] == 200 and s.get("stack")), None)
    shot("mesa_bloco_de_estanho")
    print("mesa 9 lingotes:", result and (result["item"], result["count"]))
    ok = bool(result) and result["item"] == "mekanism:block_tin" and result["count"] == 1
    h.ctl("click-slot", "200", "shift")
    h.ctl("close")
    cmd("clear @s")
    cmd("give @s mekanism:block_tin 1")
    h.ctl("interact", "72", "-60", "60")
    time.sleep(0.6)
    h.ctl("click-slot", "0")
    h.ctl("click-slot", "100", "right")
    time.sleep(0.8)
    menu = json.loads(h.ctl("menu"))
    result = next((s["stack"] for s in menu["slots"] if s["slot"] == 200 and s.get("stack")), None)
    print("mesa 1 bloco:", result and (result["item"], result["count"]))
    ok &= bool(result) and result["item"] == "mekanism:ingot_tin" and result["count"] == 9
    h.ctl("close")
    cmd("gamemode creative")
    return ok


def furnace_recipe():
    """Forno: `raw_tin` vira `ingot_tin` (`processing/tin/ingot/from_raw_smelting`) e `raw_osmium` vira `ingot_osmium` (a do original, no lugar da escrita à mão)."""
    ok = True
    cmd("tick unfreeze")
    for i, (raw, ingot) in enumerate([("raw_tin", "ingot_tin"), ("raw_osmium", "ingot_osmium"), ("raw_uranium", "ingot_uranium"), ("raw_lead", "ingot_lead")]):
        x = 74 + 2 * i
        cmd(f"setblock {x} -60 60 minecraft:air")
        cmd(f"setblock {x} -60 60 minecraft:furnace")
        time.sleep(0.4)
        cmd(f"item replace block {x} -60 60 container.0 with mekanism:{raw} 4")
        cmd(f"item replace block {x} -60 60 container.1 with minecraft:coal 4")
    time.sleep(25)
    for i, (raw, ingot) in enumerate([("raw_tin", "ingot_tin"), ("raw_osmium", "ingot_osmium"), ("raw_uranium", "ingot_uranium"), ("raw_lead", "ingot_lead")]):
        x = 74 + 2 * i
        chat = cmd(f"data get block {x} -60 60 Items")[0]
        made = re.search(r'count: (\d+), Slot: 2b, id: "mekanism:%s"' % ingot, chat)
        print(("OK   " if made else "FALHOU ") + f"forno {raw}: {chat[-160:]}")
        ok &= bool(made)
    return ok


def machine_recipe():
    """Enrichment Chamber (minério de estanho em dois pós) e Crusher (lingote de estanho em pó), com um Heat Generator encostado."""
    ok = True
    cmd("tick unfreeze")
    for i, (name, item, out) in enumerate([("enrichment_chamber", "tin_ore", "dust_tin"), ("crusher", "ingot_tin", "dust_tin")]):
        x, y, z = 84 + 4 * i, -60, 60
        for pos in [(x, y, z), (x, y, z + 1)]:
            cmd(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        time.sleep(0.3)
        h.ctl("setblock", str(x), str(y), str(z), h.BLOCK[name])
        h.ctl("setblock", str(x), str(y), str(z + 1), "mekanismgenerators:heat_generator")
        time.sleep(2)
        cmd(f"data merge block {x} {y} {z} {{Items:[{{Slot:0b,id:\"mekanism:{item}\",count:8}}]}}")
        cmd(f"data merge block {x} {y} {z+1} {{Items:[{{Slot:0b,id:\"minecraft:coal\",count:64}}]}}")
    # o Heat Generator queima os 64 de carvão em 200 a 400 ticks: repõe até as duas máquinas terem feito uma operação
    for _ in range(6):
        for i in range(2):
            x, y, z = 84 + 4 * i, -60, 60
            cmd(f"data merge block {x} {y} {z + 1} {{Items:[{{Slot:0b,id:\"minecraft:coal\",count:64}}]}}")
        time.sleep(14)
        if all(any(it[1] == "dust_tin" for it in h.read_items((84 + 4 * i, -60, 60))) for i in range(2)):
            break
    for i, (name, item, out) in enumerate([("enrichment_chamber", "tin_ore", "dust_tin"), ("crusher", "ingot_tin", "dust_tin")]):
        x, y, z = 84 + 4 * i, -60, 60
        items = h.read_items((x, y, z))
        made = [it for it in items if it[1] == out]
        print(("OK   " if made else "FALHOU ") + f"{name} {item}: {items}")
        ok &= bool(made)
    return ok


def worldgen():
    """Num mundo novo (terreno normal, sem `spike`), os cinco minérios e o sal existem nos chunks gerados em volta do jogador (`find-block` de 64 de raio)."""
    ok = True
    for block in ["tin_ore", "deepslate_tin_ore", "osmium_ore", "deepslate_osmium_ore", "uranium_ore", "fluorite_ore", "deepslate_fluorite_ore", "lead_ore", "deepslate_lead_ore", "block_salt"]:
        found = json.loads(h.ctl("find-block", f"mekanism:{block}", "64"))
        print(("OK   " if found["count"] else "FALHOU ") + f"{block}: {found['count']} em 64 de raio")
        ok &= found["count"] > 0
    return ok


def main():
    part = sys.argv[1] if len(sys.argv) > 1 else "items"
    ok = {"items": items, "mining": mining, "table": table_recipe, "furnace": furnace_recipe, "machine": machine_recipe, "worldgen": worldgen}[part]()
    print("OK" if ok else "FALHOU")


if __name__ == "__main__":
    main()
