#!/usr/bin/env python3
"""Repete no horizon2 o rastro identity_heat_generator.jsonl: o hopper de cima tick a tick (com a lava refeita a cada tick, como o
cenário em Java) e o shift-clique."""
import json, sys, time
import h2_identity as h

POS = h.POS
ABOVE = h.ABOVE
TRACE = f"{h.TRACES}/identity_heat_generator.jsonl"
BLOCK = "mekanismgenerators:heat_generator"
LAVA = '{machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:1000L}]}}'


def refill():
    h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {LAVA}")


def read_all():
    return h.read_items(POS), h.read_items(ABOVE)


def hopper_phases(lines):
    ok = True
    phases = []
    for line in lines:
        if line.get("phase", "").startswith("hopper_"):
            if line["t"] == 0:
                phases.append([])
            phases[-1].append(line)
    for phase in phases:
        first = phase[0]
        h.command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:air")
        slot = first["slots"][0]
        items = f"[{h.item_snbt(0, slot)}]" if slot[3] else "[]"
        h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:{items}}}")
        refill()
        hopper = ",".join(h.item_snbt(i, it) for i, it in enumerate(first["hopper"]) if it[3])
        h.command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:hopper[facing=down]{{Items:[{hopper}]}}")
        same_phase = True
        for t in range(0, 40):
            if t > 0:
                h.command("tick step 1")
                time.sleep(0.12)
            if t in (0, 1, 2, 8, 9, 10, 17, 18, 25, 33, 39) or t % 8 in (1, 2):
                want = phase[t]
                machine, hopper_items = read_all()
                got_m = [x for x in machine if x[0] == 0]
                want_m = [(0, *want["slots"][0])] if want["slots"][0][3] else []
                want_h = h.expected_items(want["hopper"])
                if got_m != want_m or hopper_items != want_h:
                    same_phase = False
                    print(f"DIFF {first['phase']} t={t}: esperado máquina {want_m} hopper {want_h}; obtido {got_m} {hopper_items}")
            refill()
        ok &= same_phase
        print(("OK   " if same_phase else "DIFF ") + "heat_generator", first["phase"])
    return ok


def quick_phases(lines):
    ok = True
    h.command(f"setblock {ABOVE[0]} {ABOVE[1]} {ABOVE[2]} minecraft:air")
    scenes = h.scenes_for("enrichment_chamber")
    # O que só o gerador tem no rastro: o que o combustível e o energia aceitam ou recusam.
    extra = {
        "stick_player": {"input": ("coal", "A", "", 3), "player": [(9, ("stick", "", "", 5))]},
        "stick_player_fuel_empty": {"input": None, "player": [(14, ("stick", "", "", 5))]},
        "coal_player_fuel_empty": {"input": None, "player": [(12, ("coal", "", "", 5))]},
        "lava_bucket_player": {"input": None, "player": [(13, ("lava_bucket", "", "", 1))]},
        # O item que não queima, com o slot de combustível vazio: o slot o recusa e ele fica no inventário.
        "clay_ball_player_fuel_empty": {"input": None, "player": [(15, ("clay_ball", "", "", 5))]},
    }
    h.QUICK.update({"stick_player": ("inv", 9), "stick_player_fuel_empty": ("inv", 14), "coal_player_fuel_empty": ("inv", 12), "lava_bucket_player": ("inv", 13), "clay_ball_player_fuel_empty": ("inv", 15)})
    for line in lines:
        phase = line.get("phase", "")
        if not phase.startswith("quick_"):
            continue
        label = phase[len("quick_"):]
        if label.startswith("tablet"):
            continue  # o energy_tablet não é portado
        if label in extra:
            scene = extra[label]
        else:
            scene = {**scenes[label]}
            scene["input"] = tuple(("coal", *scene["input"][1:])) if scene["input"] else None
            scene["player"] = [(slot, ("coal", *item[1:])) for slot, item in scene["player"]]
        # Com o servidor parado a cena não muda sozinha: o gerador queima o combustível assim que a lava tem lugar.
        h.command("tick freeze")
        h.command("clear @s")
        h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:[]}}")
        refill()
        if scene["input"]:
            h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{Items:[{h.item_snbt(0, scene['input'])}]}}")
        for slot, item in scene["player"]:
            where = f"inventory.{slot - 9}" if slot >= 9 else f"hotbar.{slot}"
            spec = []
            if item[1]:
                spec.append(f"custom_name='\"{item[1]}\"'")
            if item[2]:
                spec.append(f"custom_data={{d:{item[2]}}}")
            suffix = f"[{','.join(spec)}]" if spec else ""
            h.command(f"item replace entity @s {where} with minecraft:{item[0]}{suffix} {item[3]}")
        for _attempt in range(3):
            h.ctl("interact", *map(str, POS))
            time.sleep(0.4)
            if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
                break
        kind, slot = h.QUICK[label]
        h.ctl("click-slot", str(slot if kind == "inv" else 300), "shift")
        time.sleep(0.5)
        got = h.screen_slots()
        h.command("tick unfreeze")
        # O rastro: o slot de combustível é o `InventoryContainerSlot` (menu 2) e a barra começa em 31 (a principal em 4).
        want = {}
        for entry in line["menu"]:
            java, _, item = entry
            if item[0] == "energy_tablet":
                continue
            if java == 2:
                want[300] = tuple(item)
            elif 4 <= java < 31:
                want[9 + java - 4] = tuple(item)
            elif java >= 31:
                want[java - 31] = tuple(item)
        got_cmp = {k: v for k, v in got.items() if k == 300 or k < 36}
        same = got_cmp == want
        ok &= same
        print(("OK   " if same else "DIFF ") + "heat_generator", phase)
        if not same:
            print("  esperado", want)
            print("  obtido   ", got_cmp)
        h.ctl("close")
        for _ in range(15):
            if '"screen":"playing"' in h.ctl("screen").replace(" ", ""):
                break
            time.sleep(0.2)
        time.sleep(0.3)
    return ok


def main():
    lines = [json.loads(l) for l in open(TRACE)]
    h.command("tick freeze")
    h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
    h.ctl("setblock", *map(str, POS), BLOCK)
    time.sleep(1)
    ok = True
    mode = sys.argv[1:] or ["hopper", "quick"]
    if "hopper" in mode:
        ok &= hopper_phases(lines)
    h.command("tick unfreeze")
    time.sleep(3)  # a block entity recém-posta só "nasce" no primeiro tick: antes disso o que se grava nela se perde
    if "quick" in mode:
        ok &= quick_phases(lines)
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")


if __name__ == "__main__":
    main()
