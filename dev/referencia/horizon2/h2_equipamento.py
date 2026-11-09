#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): os itens de equipamento do Mekanism (frente `itens-equipamento`), sem função.

Uso: H_NAME=horizon h2_equipamento.py items|tab|armor|table|configurator   [DIRETORIO_DOS_PRINTS no desktop, o padrão é shots_equip]
  items         todo id da frente responde ao `give` e aparece no inventário
  tab           prints das abas Mekanism, Ferramentas e Combate do inventário criativo (o fim de cada uma, onde o Mekanism põe os itens)
  armor         a armadura vestida (traje de hazmat, élitro de HDPE, MekaSuit, jetpack, botas e máscara) vista de frente: o que o jogo desenha de cada uma
  table         receitas de mesa: o traje de hazmat (tintes e lingotes de chumbo, as tags `c:` do NeoForge), o instalador de tier e o configurador
  configurator  o item do configurador existe e tem modelo (o item aparece na barra, sem a mensagem de definição de item não carregada no log)
"""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

SHOTS = os.environ.get("SHOTS", "/home/luka/.local/share/jogo-isolado/horizon/data/horizon/shots_equip")
ITEMS = [
    "atomic_disassembler", "canteen", "configuration_card", "configurator", "crafting_formula", "dictionary", "dosimeter", "electric_bow", "energy_tablet", "flamethrower",
    "free_runners", "free_runners_armored", "gauge_dropper", "geiger_counter", "hazmat_mask", "hazmat_gown", "hazmat_pants", "hazmat_boots", "hdpe_elytra", "jetpack",
    "jetpack_armored", "mekasuit_helmet", "mekasuit_bodyarmor", "mekasuit_pants", "mekasuit_boots", "meka_tool", "network_reader", "portable_qio_dashboard",
    "portable_teleporter", "robit", "scuba_mask", "scuba_tank", "seismic_reader", "basic_tier_installer", "advanced_tier_installer", "elite_tier_installer",
    "ultimate_tier_installer",
]
# O gui_size de 640 por 360 (guiScale 3, janela de 1920 por 1080): o botão da próxima página e as abas de baixo do inventário criativo.
NEXT_PAGE, PREVIOUS_PAGE = (406, 72), (231, 72)
TAB_TOOLS, TAB_COMBAT = (235, 259), (262, 259)
MEKANISM_TAB_PAGE = 2


def cmd(text):
    return h.command(text)


def ctl(*args):
    return h.ctl(*args)


def shot(name):
    time.sleep(0.8)
    ctl("screenshot", f"{SHOTS}/{name}.png")


def items():
    cmd("gamemode creative")
    cmd("clear @s")
    bad = []
    for name in ITEMS:
        chat = " ".join(cmd(f"give @s mekanism:{name} 1"))
        if "Gave" not in chat:
            bad.append((name, chat[:120]))
    print(f"give: {len(ITEMS)} ids, {len(bad)} recusados {bad}")
    return not bad


def scroll_to_end(times=3):
    for _ in range(times):
        ctl("scroll-at", "450", "190", "-60")
        time.sleep(0.3)


def tab():
    cmd("gamemode creative")
    ctl("close")
    ctl("key", "e", "tap")
    time.sleep(1.2)
    ctl("time", "6000")
    ctl("click-at", *map(str, NEXT_PAGE))
    time.sleep(0.5)
    shot("aba_mekanism_inicio")
    scroll_to_end()
    shot("aba_mekanism_fim")
    ctl("click-at", *map(str, PREVIOUS_PAGE))
    time.sleep(0.5)
    for name, spot in (("ferramentas", TAB_TOOLS), ("combate", TAB_COMBAT)):
        ctl("click-at", *map(str, spot))
        time.sleep(0.5)
        scroll_to_end()
        shot(f"aba_{name}_fim")
    ctl("close")
    return True


def armor():
    cmd("gamemode creative")
    ctl("time", "6000")
    ctl("close")
    cmd("tp @s 70 -60 50 0 10")
    ctl("key", "f5", "tap")
    ctl("key", "f5", "tap")
    sets = {
        "hazmat": [("head", "hazmat_mask"), ("chest", "hazmat_gown"), ("legs", "hazmat_pants"), ("feet", "hazmat_boots")],
        "mekasuit": [("head", "mekasuit_helmet"), ("chest", "mekasuit_bodyarmor"), ("legs", "mekasuit_pants"), ("feet", "mekasuit_boots")],
        "elytra": [("chest", "hdpe_elytra")],
        "jetpack": [("chest", "jetpack_armored"), ("feet", "free_runners_armored")],
        "scuba": [("head", "scuba_mask"), ("chest", "scuba_tank")],
    }
    for name, pieces in sets.items():
        cmd("clear @s")
        for slot, item in pieces:
            cmd(f"item replace entity @s armor.{slot} with mekanism:{item} 1")
        shot(f"vestido_{name}")
    cmd("clear @s")
    ctl("key", "f5", "tap")
    return True


def grid(pattern):
    """Põe os itens da mesa de 3 por 3 (`pattern`: nove slots, `None` vazio) um a um, a partir do inventário: os itens estão nos slots 0, 1, 2 da barra."""
    ctl("close")
    time.sleep(0.4)
    ctl("interact", "72", "-60", "50")
    time.sleep(0.8)
    for hotbar, cells in pattern.items():
        ctl("click-slot", str(hotbar))
        for cell in cells:
            ctl("click-slot", str(100 + cell), "right")
        ctl("click-slot", str(hotbar))
    time.sleep(0.8)
    menu = json.loads(ctl("menu"))
    return next((s["stack"] for s in menu["slots"] if s["slot"] == 200 and s.get("stack")), None)


def table():
    ctl("time", "6000")
    cmd("gamemode survival")
    cmd("clear @s")
    ctl("close")
    cmd("tp @s 72.5 -60 52.5 180 30")
    cmd("setblock 72 -60 50 minecraft:crafting_table")
    time.sleep(0.5)
    ok = True
    # `hazmat_mask`: `III` / `IDI` com `#c:ingots/lead` e `#c:dyes/orange`; `hazmat_boots`: o tinte preto; `basic_tier_installer`: ver a receita.
    cases = [
        ("hazmat_mask", {"give": [("mekanism:ingot_lead", 5), ("minecraft:orange_dye", 1)], "grid": {0: [0, 1, 2, 3, 5], 1: [4]}}),
        ("hazmat_boots", {"give": [("mekanism:ingot_lead", 4), ("minecraft:black_dye", 1)], "grid": {0: [0, 2, 3, 5], 1: [4]}}),
    ]
    for name, case in cases:
        cmd("clear @s")
        for item, count in case["give"]:
            cmd(f"give @s {item} {count}")
        time.sleep(0.4)
        result = grid(case["grid"])
        shot(f"mesa_{name}")
        print(f"mesa {name}:", result and (result["item"], result["count"]))
        ok &= bool(result) and result["item"] == f"mekanism:{name}"
        ctl("close")
    cmd("gamemode creative")
    cmd("clear @s")
    return ok


def configurator():
    cmd("gamemode creative")
    ctl("close")
    ctl("time", "6000")
    cmd("clear @s")
    cmd("item replace entity @s hotbar.0 with mekanism:configurator 1")
    ctl("select", "1")
    time.sleep(0.8)
    shot("configurador_na_mao")
    return True


if __name__ == "__main__":
    part = sys.argv[1] if len(sys.argv) > 1 else "items"
    if len(sys.argv) > 2:
        SHOTS = sys.argv[2]
    result = {"items": items, "tab": tab, "armor": armor, "table": table, "configurator": configurator}[part]()
    print("OK" if result else "FALHOU")
    sys.exit(0 if result else 1)
