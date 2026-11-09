#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros de fluxo dos fluidos do original (`FluidsTraceTest`, `rastros/fluids_flow_*.jsonl`) e os
casos do balde (`rastros/fluids_bucket.jsonl`): o fluido de mod corre como bloco de líquido, e o balde põe e pega a fonte.

Servidor parado (`tick freeze`), um `tick step 1` por tick do rastro. O rastro grava as células que mudaram a cada 5 ticks; o jogo só diz quantas
células de um bloco há numa caixa (`horizon-ctl blocks`), então a comparação é pelo número de células de fluido em cada marca do rastro, e pelo
nível das células da linha do meio.

Uso: H_NAME=horizon h2_fluidos.py flat|waterfall|bucket|galeria
"""
import json, os, re, subprocess, sys, time

import h2_identity as h

TRACES = h.TRACES
ORIGIN = (100, -60, 60)  # o canto do volume do rastro; o chão de pedra fica em y = ORIGIN[1]
FLUID = "mekanism:brine"


def cmd(text):
    return h.command(text)


def step(n=1):
    cmd(f"tick step {n}")
    time.sleep(0.35 + 0.05 * n)


def at(x, y, z):
    return f"{ORIGIN[0] + x} {ORIGIN[1] + y} {ORIGIN[2] + z}"


def rows(name):
    return [json.loads(line) for line in open(f"{TRACES}/fluids_flow_{name}.jsonl")]


def count_in_box(size, block):
    a = at(0, 0, 0)
    b = at(size[0] - 1, size[1] - 1, size[2] - 1)
    out = json.loads(h.ctl("blocks", *a.split(), *b.split()))
    return out["counts"].get(block, 0)


def level_at(x, y, z):
    text = h.ctl("block", *at(x, y, z).split())
    found = re.search(r"level=(\d+)", text)
    return int(found.group(1)) if found and FLUID in text else None


def prepare(size, setup):
    cmd("tick unfreeze")
    # Limpa até 12 de altura: o que um cenário anterior (a queda da `waterfall`) deixou no alto continuaria correndo.
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, max(size[1], 12) - 1, size[2] - 1)} minecraft:air")
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, 0, size[2] - 1)} minecraft:stone")
    time.sleep(1)
    cmd("tick freeze")
    time.sleep(0.3)
    setup()


def run(name, size, setup):
    trace = rows(name)
    prepare(size, setup)
    cells = {}
    ok = True
    t = 0
    for row in trace:
        target = row["tick"]
        if row.get("full"):
            for c in row["cells"]:
                cells[tuple(c[:3])] = (c[3], c[4])
            continue
        if target > t:
            step(target - t)
            t = target
        for c in row["cells"]:
            cells[tuple(c[:3])] = (c[3], c[4])
        want = sum(1 for v in cells.values() if v[0] == FLUID)
        got = count_in_box(size, FLUID)
        same = want == got
        ok &= same
        print(("OK   " if same else "DIFF ") + f"{name} t={t}: células de fluido {got} (rastro {want})")
        if not same:
            # onde as duas listas diferem, na linha do meio
            for (x, y, z), (block, level) in sorted(cells.items()):
                if block == FLUID and level_at(x, y, z) != level:
                    print(f"      {x},{y},{z}: rastro {level}, jogo {level_at(x, y, z)}")
                    break
    return ok


def flat():
    return run("flat", (31, 4, 31), lambda: cmd(f"setblock {at(15, 1, 15)} {FLUID}[level=0,up=false]"))


def waterfall():
    def setup():
        cmd(f"fill {at(10, 1, 10)} {at(14, 6, 14)} minecraft:stone")
        cmd(f"setblock {at(10, 7, 10)} {FLUID}[level=0,up=false]")

    return run("waterfall", (25, 12, 25), setup)


def block(x, y, z):
    return json.loads(h.ctl("block", *at(x, y, z).split()))["state"]


def held():
    inv = json.loads(h.ctl("inventory"))
    return (inv.get("held") or {}).get("item")


def counts():
    return json.loads(h.ctl("inventory")).get("counts", {})


def delta(before, after):
    return {k: after.get(k, 0) - before.get(k, 0) for k in set(before) | set(after) if after.get(k, 0) != before.get(k, 0)}


def use_on(x, y, z, face, item, mode="survival"):
    """Põe `item` na mão, mira a face de um bloco e usa. Devolve (o que mudou no inventário)."""
    cmd(f"gamemode {mode}")
    cmd("clear @s")
    cmd(f"give @s {item}")
    h.ctl("select", "1")
    side = {"west": (-2.5, 0), "east": (2.5, 0)}.get(face, (0, -2.5))
    h.ctl("tp", str(ORIGIN[0] + x + 0.5 + side[0]), str(ORIGIN[1] + 1), str(ORIGIN[2] + z + 0.5 + side[1]))
    time.sleep(0.5)
    h.ctl("look-at", "block", *at(x, y, z).split(), face)
    time.sleep(0.6)
    before = counts()
    h.ctl("use-item")
    time.sleep(0.8)
    return delta(before, counts())


def clean(size=(12, 6, 12)):
    cmd("tick unfreeze")
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, size[1] - 1, size[2] - 1)} minecraft:air")
    cmd(f"fill {at(0, 0, 0)} {at(size[0] - 1, 0, size[2] - 1)} minecraft:stone")
    cmd("tick freeze")
    time.sleep(0.4)


def check(name, got, want):
    same = got == want
    print(("OK   " if same else "DIFF ") + f"{name}: {got} (esperado {want})")
    return same


def bucket():
    """Os casos `full_*` e `empty_*` de `fluids_bucket.jsonl`, repetidos em jogo, em sobrevivência: a célula e o inventário depois do uso."""
    ok = True
    bucket_item = "minecraft:bucket"
    full = f"{FLUID}_bucket"
    ours = lambda lvl: f"{FLUID}[level={lvl},up=false]"
    # full_floor: o balde cheio no piso põe a fonte na célula de cima e vira balde vazio.
    clean()
    change = use_on(5, 0, 5, "up", full)
    ok &= check("full_floor célula", block(5, 1, 5), ours(0))
    ok &= check("full_floor inventário", change, {full: -1, bucket_item: 1})
    # full_side_of_stone: a face do lado leva à célula ao lado.
    clean()
    cmd(f"setblock {at(5, 1, 5)} minecraft:stone")
    use_on(5, 1, 5, "west", full)
    ok &= check("full_side_of_stone célula", block(4, 1, 5), ours(0))
    # full_short_grass e full_leaves: o raio do balde para no contorno da planta, e o fluido vai para a célula de cima, com a planta no lugar.
    for plant in ("minecraft:short_grass", "minecraft:oak_leaves[persistent=true]"):
        clean()
        cmd(f"setblock {at(5, 0, 5)} minecraft:grass_block")
        cmd(f"setblock {at(5, 1, 5)} {plant}")
        use_on(5, 1, 5, "up", full)
        ok &= check(f"full_{plant.split(':')[1].split('[')[0]} célula de cima", block(5, 2, 5), ours(0))
        ok &= check(f"full_{plant.split(':')[1].split('[')[0]} a planta fica", block(5, 1, 5).split("[")[0], plant.split("[")[0])
    # full_water_source: a fonte de água é trocada.
    clean()
    cmd(f"setblock {at(5, 1, 5)} minecraft:water")
    use_on(5, 0, 5, "up", full)
    ok &= check("full_water_source célula", block(5, 1, 5), ours(0))
    # full_own_source: a fonte do próprio fluido não muda e o balde se gasta.
    clean()
    cmd(f"setblock {at(5, 1, 5)} {ours(0)}")
    change = use_on(5, 0, 5, "up", full)
    ok &= check("full_own_source célula", block(5, 1, 5), ours(0))
    ok &= check("full_own_source inventário", change, {full: -1, bucket_item: 1})
    # full_own_flowing_3: o corrente vira fonte.
    clean()
    cmd(f"setblock {at(5, 1, 5)} {FLUID}[level=3,up=false]")
    use_on(5, 0, 5, "up", full)
    ok &= check("full_own_flowing_3 célula", block(5, 1, 5), ours(0))
    # empty_source: o balde vazio pega a fonte.
    clean()
    cmd(f"setblock {at(5, 1, 5)} {ours(0)}")
    h.ctl("tp", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1), str(ORIGIN[2] + 2.5))
    cmd("gamemode survival"); cmd("clear @s"); cmd("give @s minecraft:bucket"); h.ctl("select", "1")
    h.ctl("look-at", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1.4), str(ORIGIN[2] + 5.5)); time.sleep(0.6)
    before = counts(); h.ctl("use-item"); time.sleep(0.8); change = delta(before, counts())
    ok &= check("empty_source inventário", change, {bucket_item: -1, full: 1})
    ok &= check("empty_source célula", block(5, 1, 5), "minecraft:air")
    # empty_level_3 e empty_level_8: o balde vazio não pega o que corre.
    for lvl in (3, 8):
        clean()
        cmd(f"setblock {at(5, 1, 5)} {FLUID}[level={lvl},up=false]")
        h.ctl("tp", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1), str(ORIGIN[2] + 2.5))
        cmd("clear @s"); cmd("give @s minecraft:bucket"); h.ctl("select", "1")
        h.ctl("look-at", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1.2), str(ORIGIN[2] + 5.5)); time.sleep(0.6)
        before = counts(); h.ctl("use-item"); time.sleep(0.8); change = delta(before, counts())
        ok &= check(f"empty_level_{lvl} inventário", change, {})
    # a coleta em criativo: o vazio fica e o cheio entra.
    clean()
    cmd(f"setblock {at(5, 1, 5)} {ours(0)}")
    h.ctl("tp", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1), str(ORIGIN[2] + 2.5))
    cmd("gamemode creative"); cmd("clear @s"); cmd("give @s minecraft:bucket"); h.ctl("select", "1")
    h.ctl("look-at", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1.4), str(ORIGIN[2] + 5.5)); time.sleep(0.6)
    before = counts(); h.ctl("use-item"); time.sleep(0.8); change = delta(before, counts())
    ok &= check("coleta em criativo inventário", change, {full: 1})
    cmd("gamemode survival")
    return ok


def dispenser():
    """O dispensador com o balde cheio: esvazia na célula da frente (`BUCKET_DISPENSE_BEHAVIOR`) e o slot fica com o balde vazio; ocupado, solta o item."""
    ok = True
    full = f"{FLUID}_bucket"
    for name, front, want_cell, want_slot in (
        ("livre", "minecraft:air", f"{FLUID}[level=0,up=false]", "minecraft:bucket"),
        ("ocupado", "minecraft:stone", "minecraft:stone", None),
    ):
        clean()
        cmd(f"setblock {at(5, 1, 5)} minecraft:dispenser[facing=east]")
        cmd(f"setblock {at(6, 1, 5)} {front}")
        time.sleep(0.5)
        cmd(f"item replace block {at(5, 1, 5)} container.0 with {full} 1")
        cmd("tick unfreeze")
        cmd(f"setblock {at(5, 1, 6)} minecraft:redstone_block")
        time.sleep(2.5)
        cmd("tick freeze")
        ok &= check(f"dispensador {name} célula", block(6, 1, 5), want_cell)
        data = " ".join(cmd(f"data get block {at(5, 1, 5)} Items"))
        slot = re.search(r'id: "([^"]+)"', data)
        ok &= check(f"dispensador {name} slot", slot.group(1) if slot else None, want_slot)
    return ok


def paste():
    """O balde de pasta nutritiva: com fome é bebido (a fome sobe e o balde vira o vazio), sem fome esvazia."""
    ok = True
    cmd("gamemode survival"); cmd("clear @s"); cmd("difficulty normal")
    cmd("give @s mekanism:nutritional_paste_bucket"); h.ctl("select", "1")
    cmd("tick unfreeze")
    cmd("effect give @s minecraft:hunger 14 255")
    time.sleep(16)
    cmd("effect clear @s minecraft:hunger")
    h.ctl("tp", str(ORIGIN[0] + 5.5), str(ORIGIN[1] + 1), str(ORIGIN[2] + 2.5)); h.ctl("look", "0", "-20")
    time.sleep(1)
    before = (" ".join(cmd("data get entity @s foodLevel")), " ".join(cmd("data get entity @s foodSaturationLevel")))
    h.ctl("use-item", "--hold", "3")
    time.sleep(1.2)
    after = (" ".join(cmd("data get entity @s foodLevel")), " ".join(cmd("data get entity @s foodSaturationLevel")))
    print(f"fome e saturação antes {before} depois {after}; inventário {counts()}")
    ok &= check("pasta bebida: o balde vira o vazio", counts().get("minecraft:bucket", 0), 1)
    return ok


def meets(kind):
    """O fluido de mod ao lado da água (`kind` `water`) ou da lava (`lava`) do vanilla, como em `meets_water`: as células de cada fluido na região do par, a cada marca do
    rastro. O balde de água por cima (as outras duas fontes do rastro) não entra: a região é só o par."""
    trace = rows("meets_water")
    z0 = 0 if kind == "water" else 24
    vanilla = f"minecraft:{kind}"
    prepare((48, 6, 48), lambda: (
        cmd(f"setblock {at(10, 1, z0 + 10)} {FLUID}[level=0,up=false]"),
        cmd(f"setblock {at(11, 1, z0 + 10)} {vanilla}"),
    ))
    cells, t, ok = {}, 0, True
    box = (at(0, 0, z0), at(23, 5, z0 + 23))
    for row in trace:
        target = row["tick"]
        if row.get("full"):
            for c in row["cells"]:
                cells[tuple(c[:3])] = (c[3], c[4])
            continue
        if target > t:
            step(target - t)
            t = target
        for c in row["cells"]:
            cells[tuple(c[:3])] = (c[3], c[4])
        if t > 70:
            break
        want = {}
        for (x, y, z), (block, _) in cells.items():
            if x < 24 and z0 <= z < z0 + 24 and block in (FLUID, "minecraft:water", "minecraft:lava"):
                want[block] = want.get(block, 0) + 1
        got = json.loads(h.ctl("blocks", *box[0].split(), *box[1].split()))["counts"]
        got = {k: v for k, v in got.items() if k in (FLUID, "minecraft:water", "minecraft:lava")}
        ok &= check(f"meets_{kind} t={t}", got, want)
    return ok


def fire_and_air():
    """O jogador em chamas dentro do fluido de mod e dentro da água do vanilla, e o ar a cada tick nos dois (coluna de 4 células, jogador no meio)."""
    out = {}
    for name, block in (("brine", f"{FLUID}[level=0,up=true]"), ("water", "minecraft:water")):
        clean((12, 8, 12))
        cmd("tick unfreeze")
        cmd(f"fill {at(4, 1, 4)} {at(6, 6, 6)} minecraft:stone")
        cmd(f"fill {at(5, 1, 5)} {at(5, 4, 5)} {block}")
        cmd("gamemode survival"); cmd("effect clear @s")
        cmd(f"tp @s {ORIGIN[0] + 5.5} {ORIGIN[1] + 3.0} {ORIGIN[2] + 5.5}")
        cmd("tick freeze"); time.sleep(0.5)
        cmd("data merge entity @s {Fire:200s}")
        air = []
        for t in range(6):
            step(1)
            p = json.loads(h.ctl("state"))["player"]
            air.append(p["air"])
        fire = " ".join(cmd("data get entity @s Fire"))
        out[name] = (air, fire)
        print(name, "ar a cada tick", air, "| fogo", fire)
    cmd("tick unfreeze")
    return out


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "flat"
    ok = {"flat": flat, "waterfall": waterfall, "bucket": bucket, "dispenser": dispenser, "paste": paste, "meets_water": lambda: meets("water"), "meets_lava": lambda: meets("lava"), "fire_air": fire_and_air}[what]()
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
