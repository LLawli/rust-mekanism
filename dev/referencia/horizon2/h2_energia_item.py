#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): a energia guardada no item (frente `energia-item`, componente `mekanism:energy`).

    h2_energia_item.py [DIRETORIO_DOS_PRINTS]

Dá os itens com energia (a barra e o tooltip), põe uma Enrichment Chamber e um Heat Generator vindos de itens com energia (o tile guarda o que cabe), quebra a máquina com energia
(o item solto leva `mekanism:energy`) e a repõe (o tile volta com a mesma energia). Cada passo tira um print e a linha do que o jogo leu; a saída é `legendas.json` na pasta.
O jogo aberto num mundo, em criativo, sem som e headless (regra de builds e jogo).
"""
import json, os, re, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("H_NAME", "horizon")
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/energia_item"
OUT = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser("~/.cache/rust-mekanism/wa/energia-item")
CHAMBER = (2, 101, 0)
HEAT = (6, 101, 0)
captions = []
failures = []


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


def expect(label, got, want):
    ok = got == want
    print(("ok   " if ok else "FALHOU"), label, got, "(esperado", want, ")")
    if not ok:
        failures.append(label)


def number(text):
    found = re.findall(r"(-?\d+)[lLbBsS]?\b", text.split("energy")[-1]) if text else []
    return int(found[-1]) if found else None


def block_energy(pos):
    return number(cmd("data get block {} {} {} energy".format(*pos)))


def drop_energy():
    text = cmd("data get entity @e[type=minecraft:item,limit=1,sort=nearest] Item")
    m = re.search(r'"mekanism:energy": (\d+)L', text)
    return int(m.group(1)) if m else 0, text


def menu_slot(item, skip=()):
    """O primeiro slot do menu aberto com o item, fora da grade e do resultado."""
    for slot in json.loads(ctl("menu")).get("slots", []):
        stack = slot.get("stack")
        if stack and stack.get("item") == item and not (100 <= slot["slot"] <= 200) and slot["slot"] not in skip:
            return slot["slot"]
    return None


def mek_data():
    """A mesa: o cubo avançado (`mekanism:energy_cube/advanced`, AEA/IPI/AEA) a partir do cubo básico com 5 000 e dois Energy Tablets com 1 000 e 2 500: o resultado soma 8 500."""
    cmd("gamemode survival")
    cmd("clear @s")
    cmd("setblock 4 101 3 minecraft:crafting_table")
    cmd("give @s mekanism:alloy_infused 4")
    cmd("give @s mekanism:ingot_osmium 2")
    cmd("give @s mekanism:energy_tablet[mekanism:energy=1000L]")
    cmd("give @s mekanism:energy_tablet[mekanism:energy=2500L]")
    cmd("give @s mekanism:basic_energy_cube[mekanism:energy=5000L]")
    ctl("tp", "4", "101", "1")
    time.sleep(0.5)
    ctl("interact", "4", "101", "3")
    time.sleep(0.8)
    # AEA / IPI / AEA: A=0,2,6,8  E=1,7  I=3,5  P=4
    plan = [("mekanism:alloy_infused", [0, 2, 6, 8]), ("mekanism:ingot_osmium", [3, 5]), ("mekanism:energy_tablet", [1]), ("mekanism:energy_tablet", [7]), ("mekanism:basic_energy_cube", [4])]
    for item, cells in plan:
        slot = menu_slot(item)
        ctl("click-slot", str(slot))
        for cell in cells:
            ctl("click-slot", str(100 + cell), "right")
    time.sleep(0.8)
    result = next((s.get("stack") for s in json.loads(ctl("menu"))["slots"] if s["slot"] == 200 and s.get("stack")), None)
    expect("mesa: o cubo avançado sai", result and result.get("item"), "mekanism:advanced_energy_cube")
    shot("mesa_mek_data", "Mesa: cubo básico com 5 000 FE e dois Energy Tablets com 1 000 e 2 500 FE dão o cubo avançado, que a receita mek_data enche com a soma (8 500 FE)")
    ctl("click-slot", "200", "shift")
    ctl("close")
    text = cmd("data get entity @s Inventory")
    m = re.search(r'"mekanism:energy": (\d+)L, count: 1, Slot: \d+b, id: "mekanism:advanced_energy_cube"', text) or re.search(r'advanced_energy_cube', text) and re.search(r'"mekanism:energy": (\d+)L[^}]*}[^{]*advanced', text)
    expect("mesa: a energia do cubo avançado", int(m.group(1)) if m else None, 8500)
    cmd("clear @s")
    cmd("gamemode creative")


def wrench():
    """O Configurator no modo `wrench` com o jogador agachado desmonta a máquina e o item vai para o inventário com a energia do tile (`WorldUtils.dismantleBlock` rola a mesma loot table)."""
    cmd("gamemode creative")
    cmd("clear @s")
    cmd("give @s mekanism:enrichment_chamber[mekanism:energy=321L]")
    ctl("place", *map(str, CHAMBER), "--item", "mekanism:enrichment_chamber")
    cmd("clear @s")
    cmd('give @s mekanism:configurator[mekanism:configurator_mode="wrench"]')
    ctl("select", "mekanism:configurator")
    time.sleep(1.0)
    ctl("look-at", "block", *map(str, CHAMBER))
    ctl("key", "shift", "down")
    ctl("interact", *map(str, CHAMBER))
    ctl("key", "shift", "up")
    time.sleep(1.0)
    text = cmd("data get entity @s Inventory")
    m = re.search(r'"mekanism:energy": (\d+)L[^}]*}, count: 1, Slot: \d+b, id: "mekanism:enrichment_chamber"', text)
    expect("Configurator em wrench: a máquina vai para o inventário com a energia", int(m.group(1)) if m else None, 321)
    shot("wrench", "Configurator em modo wrench (agachado): a Enrichment Chamber desmontada vai ao inventário com os 321 FE do tile")
    cmd("clear @s")


def main():
    os.makedirs(OUT, exist_ok=True)
    subprocess.run(["ssh", "desktop", f"rm -rf {SHOTS}; mkdir -p {SHOTS}"], check=True)
    ctl("wait-ready")
    time.sleep(10)
    cmd("gamemode creative")
    cmd("tp @s 4 102 -4 0 20")
    cmd("fill -4 100 -8 12 100 8 minecraft:stone")
    cmd("fill -4 101 -8 12 106 8 minecraft:air")
    cmd("kill @e[type=minecraft:item]")
    cmd("clear @s")
    for item in [
        "mekanism:energy_tablet[mekanism:energy=500000L]",
        "mekanism:energy_tablet",
        "mekanism:free_runners[mekanism:energy=64000L]",
        "mekanism:meka_tool[mekanism:energy=8000000L]",
        "mekanism:basic_energy_cube[mekanism:energy=2000000L]",
        "mekanism:enrichment_chamber[mekanism:energy=500L]",
        "mekanismgenerators:heat_generator[mekanism:energy=500L]",
        "mekanism:electric_bow[mekanism:energy=1L]",
    ]:
        print(cmd(f"give @s {item}"))
    ctl("select", "1")
    shot("barras", "Hotbar: tablet com 500 kFE (7 de 13), tablet vazio (sem barra), Free Runners cheio, Meka-Tool pela metade, cubo básico pela metade, arco com 1 FE")
    # tooltips na tela do inventário (em sobrevivência: a tela criativa tem outra disposição)
    cmd("gamemode survival")
    ctl("open-inventory")
    time.sleep(1.0)
    for item, nth, name, caption in [
        ("mekanism:energy_tablet", 0, "tooltip_tablet", "Tooltip do Energy Tablet com 500 kFE: Stored energy: 500 kFE/1 MFE"),
        ("mekanism:energy_tablet", 1, "tooltip_tablet_vazio", "Tooltip do Energy Tablet vazio: Stored energy: 0 FE/1 MFE"),
        ("mekanism:meka_tool", 0, "tooltip_meka_tool", "Tooltip do Meka-Tool com 8 MFE: 8 MFE/16 MFE"),
        ("mekanism:basic_energy_cube", 0, "tooltip_cubo", "Tooltip do cubo de energia básico com 2 MFE: 2 MFE/4 MFE"),
        ("mekanism:enrichment_chamber", 0, "tooltip_chamber", "Tooltip da Enrichment Chamber com 500 FE: 500 FE/20 FE (o item guarda quatro ticks de uso, o tile 1 kFE)"),
        ("mekanismgenerators:heat_generator", 0, "tooltip_heat", "Tooltip do Heat Generator com 500 FE: 500 FE/240 FE (o item passa da capacidade)"),
    ]:
        slots = [w for w in widgets().get("widgets", []) if w.get("kind") == "slot" and (w.get("stack") or {}).get("item") == item]
        if len(slots) <= nth:
            failures.append(f"sem slot de {item}")
            continue
        x, y, w, h_ = slots[nth]["bounds"]
        ctl("scroll-at", str(int(x + 8)), str(int(y + 8)), "0")
        shot(name, caption)
    ctl("close")
    cmd("gamemode creative")
    # máquina posta por um item com energia
    cmd("setblock {} {} {} minecraft:air".format(*CHAMBER))
    ctl("select", "6")
    print(ctl("place", *map(str, CHAMBER), "--item", "mekanism:enrichment_chamber"))
    ctl("select", "7")
    print(ctl("place", *map(str, HEAT), "--item", "mekanismgenerators:heat_generator"))
    time.sleep(1.5)
    expect("Enrichment Chamber posta com 500 no item", block_energy(CHAMBER), 500)
    expect("Heat Generator posto com 500 (cabem 240)", block_energy(HEAT), 240)
    cmd("tp @s 4 102 -4 0 20")
    shot("postas", "Enrichment Chamber posta com 500 FE e Heat Generator com 240 FE (o item levava 500: o resto não cabe)")
    # quebrar com energia (sobrevivência, picareta)
    cmd("gamemode survival")
    cmd("give @s minecraft:diamond_pickaxe")
    ctl("select", "minecraft:diamond_pickaxe")
    cmd("data merge block {} {} {} {{energy:777}}".format(*CHAMBER))
    cmd("kill @e[type=minecraft:item]")
    print(ctl("break", *map(str, CHAMBER)))
    time.sleep(1.0)
    energy, text = drop_energy()
    expect("o item solto leva a energia do tile", energy, 777)
    shot("quebrada", "Enrichment Chamber quebrada com 777 FE: o item solto leva mekanism:energy = 777")
    # repor
    cmd("kill @e[type=minecraft:item]")
    cmd("gamemode creative")
    cmd("clear @s mekanism:enrichment_chamber")
    cmd("give @s mekanism:enrichment_chamber[mekanism:energy=777L]")
    print(ctl("place", *map(str, CHAMBER), "--item", "mekanism:enrichment_chamber"))
    time.sleep(1.5)
    expect("reposta com a mesma energia", block_energy(CHAMBER), 777)
    shot("reposta", "Enrichment Chamber reposta: o tile volta com 777 FE")
    # zero energia: a máquina quebrada empilha com a simples
    cmd("data merge block {} {} {} {{energy:0}}".format(*CHAMBER))
    cmd("gamemode survival")
    cmd("give @s minecraft:diamond_pickaxe")
    ctl("select", "minecraft:diamond_pickaxe")
    cmd("kill @e[type=minecraft:item]")
    print(ctl("break", *map(str, CHAMBER)))
    time.sleep(1.0)
    energy, text = drop_energy()
    expect("o item solto sem energia existe", "enrichment_chamber" in text, True)
    expect("sem energia o item solto não leva o componente", "mekanism:energy" in text, False)
    cmd("gamemode creative")
    wrench()
    mek_data()
    json.dump(captions, open(os.path.join(OUT, "legendas.json"), "w"), ensure_ascii=False, indent=1)
    print("falhas:", failures)


main()
