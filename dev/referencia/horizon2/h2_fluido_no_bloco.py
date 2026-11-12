#!/usr/bin/env python3
"""A água e a lava dentro de um bloco `fluid_logged` (decisão 21), em jogo (`H_NAME=horizon`), contra os rastros do original (`FluidoNoBlocoTraceTest`,
`rastros/fluido_no_bloco_{flow,entity,bucket}.jsonl`).

    h2_fluido_no_bloco.py flow      os oito blocos com água e com lava postos como o `placeLiquid` põe (bloco e vizinho mexido, que agenda o tick), servidor parado, um
                                    `tick step` até cada marca do rastro: o número de blocos de água e de lava no volume e o `fluid_logged` do bloco, contra o original
    h2_fluido_no_bloco.py entity    o jogador numa célula de cabo com água, com lava e sem fluido: o ar, a vida e o fogo, contra o rastro de jogador do original; o
                                    porco e o item (que o Horizon lê pela tabela de fatos do estado) só se medem, sem comparar
    h2_fluido_no_bloco.py mob       o porco e o item solto na célula de um cabo com água, com lava e sem fluido, tick a tick: o ar, a vida, o fogo, se vive e a altura, contra o rastro de
                                    porco e de item do original (decisão 88: eles leem o fluido pela tabela de fatos do estado, que `logged_fluid_facts` completa)
    h2_fluido_no_bloco.py parado      o item solto parado na célula do Heat Generator (e do cabo) com lava, com água e sem fluido, tick a tick, contra `rastros/fluido_item_parado.jsonl` (decisão 100:
                                    o item com `noPhysics` não é afetado pelos blocos: a lava não o acende, a água não o apaga, o cacto não o fere)
    h2_fluido_no_bloco.py parado_video   o vídeo com som: o item no gerador com lava (vive) e no cabo com lava (queima), lado a lado (`NOME=<gravar-jogo start>`, `SEGUNDOS`, `ARQUIVO`)
    h2_fluido_no_bloco.py mob_video   o vídeo com som: o porco num cabo com água, o porco num cabo com lava (queima e morre) e o item de pedra que a lava destrói, em poços de vidro
                                    (`NOME=<gravar-jogo start>`, `SEGUNDOS`, `ARQUIVO`)
    h2_fluido_no_bloco.py bucket    os casos do `fluido_no_bloco_bucket.jsonl` (cabo, fábrica e cubo de energia; vazio, com água e com lava; balde de água, de lava e vazio;
                                    em pé, agachado e no criativo): o estado do bloco e o inventário depois do clique
    h2_fluido_no_bloco.py cena PASTA   o print lado a lado de cada bloco com água e com lava
Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim."""
import json
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

TRACES = os.path.join(h.TOOLS, "..", "rastros")
BLOCKS = ["mekanism:basic_universal_cable", "mekanism:basic_mechanical_pipe", "mekanism:basic_fluid_tank", "mekanism:basic_energy_cube",
          "mekanism:industrial_alarm", "mekanism:chargepad", "mekanism:basic_smelting_factory", "mekanism:personal_chest"]
CELL = 18
X0, Z0, Y = 20, 100, 150  # o canto do volume de testes: um andar de pedra em y = 149 e as células em y = 150
MARKS = (10, 30, 60, 100, 200)


def cmd(text):
    return h.command(text)


def tp(x, y, z):
    """`/tp`: o `tp` do `horizon-ctl` deixa de valer depois de um teleporte com o jogador no ar."""
    cmd(f"tp @s {x} {y} {z}")


def ctl(*args):
    return h.ctl(*args)


def sky():
    ctl("close")
    for c in ("gamemode creative", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false",
              "difficulty peaceful", "kill @e[type=!minecraft:player]"):
        cmd(c)


def restore():
    # O jogador sai do meio do que os testes deixaram correndo (a lava mata o jogador de sobrevivência e a vida zerada trava o resto do teste).
    cmd("effect clear @s")
    cmd("gamemode creative")
    tp(f"{X0 + 60}", f"{Y + 30}", f"{Z0 + 20}")
    cmd("tick unfreeze")
    cmd("gamerule advance_time true")
    cmd("gamerule advance_weather true")
    cmd("gamerule spawn_mobs true")


def rows(name):
    return [json.loads(line) for line in open(f"{TRACES}/fluido_no_bloco_{name}.jsonl")]


def cell(index, row):
    return (X0 + CELL * index + CELL // 2, Y, Z0 + CELL * row + CELL // 2)


def check(name, got, want):
    same = got == want
    if same:
        print(f"OK   {name}")
    elif isinstance(got, dict):
        print(f"DIFF {name}: " + ", ".join(f"{k}={got[k]} (esperado {want[k]})" for k in got if got[k] != want[k]))
    else:
        print(f"DIFF {name}: {got} (esperado {want})")
    return same


def want_flow():
    """Do rastro: por cenário (bloco, fluido) e marca, quantas células com bloco de água ou de lava (de `y` a `y + 3`, o que o jogo conta) e o `fluid_logged` do bloco."""
    state = {}
    last = {}
    for row in rows("flow"):
        if "tick" not in row:
            continue
        key = (row["block"], row["fluid"])
        cells = state.setdefault(key, {})
        for x, y, z, block, fluid, amount in row["cells"]:
            cells[(x, y, z)] = (block, fluid)
        last[(key, row["tick"])] = None
    out = {}
    # Recalcula por marca, reaplicando as linhas em ordem.
    for mark in MARKS:
        snap = {}
        for row in rows("flow"):
            if "tick" not in row or row["tick"] > mark:
                continue
            cells = snap.setdefault((row["block"], row["fluid"]), {})
            for x, y, z, block, fluid, amount in row["cells"]:
                cells[(x, y, z)] = (block, fluid)
        for key, cells in snap.items():
            water = sum(1 for (x, y, z), (b, f) in cells.items() if y >= 0 and b.startswith("minecraft:water"))
            lava = sum(1 for (x, y, z), (b, f) in cells.items() if y >= 0 and b.startswith("minecraft:lava"))
            logged = [b for (x, y, z), (b, f) in cells.items() if (x, y, z) == (0, 0, 0)]
            out[(key, mark)] = (water, lava, re.search(r"fluid_logged=(\w+)", logged[0]).group(1) if logged and "fluid_logged" in logged[0] else None)
    return out


def box(index, row):
    cx, cy, cz = cell(index, row)
    return (cx - 8, cy, cz - 8), (cx + 8, cy + 3, cz + 8)


def counts(index, row):
    a, b = box(index, row)
    got = json.loads(ctl("blocks", *map(str, a), *map(str, b)))["counts"]
    return got.get("minecraft:water", 0), got.get("minecraft:lava", 0)


def flow():
    want = want_flow()
    sky()
    cmd("tick unfreeze")
    for z in range(Z0, Z0 + CELL * 2 + 1, 12):
        cmd(f"fill {X0} {Y} {z} {X0 + CELL * len(BLOCKS)} {Y + 6} {min(z + 11, Z0 + CELL * 2)} minecraft:air")
    cmd(f"fill {X0} {Y - 1} {Z0} {X0 + CELL * len(BLOCKS)} {Y - 1} {Z0 + CELL * 2} minecraft:stone")
    tp(str(X0 + CELL * len(BLOCKS) // 2), str(Y + 4), str(Z0 + CELL))
    time.sleep(3)
    cmd("tick freeze")
    time.sleep(0.5)
    for row, fluid in enumerate(("water", "lava")):
        for index, block in enumerate(BLOCKS):
            x, y, z = cell(index, row)
            ctl("setblock", str(x), str(y), str(z), f"{block}[fluid_logged={fluid}]")
            # O vizinho mexido atualiza a forma do bloco, que agenda o tick do fluido (`updateFluids`), como o `placeLiquid` agenda o dele.
            ctl("setblock", str(x + 1), str(y), str(z), "minecraft:stone")
            ctl("setblock", str(x + 1), str(y), str(z), "minecraft:air")
    done = 0
    ok = True
    for mark in MARKS:
        cmd(f"tick step {mark - done}")
        time.sleep(0.6 + 0.01 * (mark - done))
        done = mark
        for row, fluid in enumerate(("water", "lava")):
            for index, block in enumerate(BLOCKS):
                x, y, z = cell(index, row)
                state = json.loads(ctl("block", str(x), str(y), str(z)))["state"]
                logged = re.search(r"fluid_logged=(\w+)", state)
                got = counts(index, row) + (logged.group(1) if logged else None,)
                ok &= check(f"{block.split(':')[1]} {fluid} @{mark}", got, want.get(((block, fluid), mark)))
    restore()
    return ok


BUCKET_BLOCKS = ["mekanism:basic_universal_cable", "mekanism:basic_smelting_factory", "mekanism:basic_energy_cube"]


def inventory():
    inv = json.loads(ctl("inventory"))
    held = inv.get("held") or {}
    item = held.get("item")
    return (f"{item}x{held.get('count', 1)}" if item and item != "minecraft:air" else "empty"), inv.get("counts", {})


def bucket_case(block, start, item, mode, x, z):
    """Um caso do rastro: o bloco no estado `start`, o balde `item` na mão, o clique na face de cima, em pé, agachado ou no criativo."""
    cmd(f"gamemode {'creative' if mode == 'creative' else 'survival'}")
    cmd("clear @s")
    cmd(f"fill {x - 2} {Y} {z - 2} {x + 2} {Y + 4} {z + 3} minecraft:air")
    cmd(f"setblock {x} {Y} {z} {block}[fluid_logged={start}]")
    cmd(f"give @s minecraft:{item}")
    ctl("close")
    ctl("select", "1")
    # De pé no chão, a dois blocos ao sul, mirando a face sul: o cabo é fino demais para o jogador ficar em cima dele. "Em cima" do rastro é o bloco ao sul aqui.
    if mode == "sneak":
        ctl("key", "shift", "down")
        time.sleep(0.3)
    tp(f"{x + 0.5}", f"{Y}", f"{z + 2.5}")
    time.sleep(0.6)
    ctl("look-at", "block", str(x), str(Y), str(z), "south")
    time.sleep(0.4)
    ctl("use-item")
    time.sleep(0.3)
    cmd("tick step 2")
    time.sleep(0.6)
    if mode == "sneak":
        ctl("key", "shift", "up")
    ctl("close")
    state = json.loads(ctl("block", str(x), str(Y), str(z)))["state"]
    above = json.loads(ctl("block", str(x), str(Y), str(z + 1)))["state"]
    hand, counts = inventory()
    logged = re.search(r"fluid_logged=(\w+)", state)
    return {"state": logged.group(1) if logged else state, "above": above.replace("minecraft:air", "minecraft:air"), "hand": hand,
            "water_buckets": counts.get("minecraft:water_bucket", 0), "lava_buckets": counts.get("minecraft:lava_bucket", 0), "buckets": counts.get("minecraft:bucket", 0)}


def bucket():
    only = sys.argv[2] if len(sys.argv) > 2 else None
    sky()
    cmd("tick unfreeze")
    cmd(f"fill {X0} {Y} {Z0} {X0 + 160} {Y + 8} {Z0 + 40} minecraft:air")
    cmd(f"fill {X0} {Y - 1} {Z0} {X0 + 160} {Y - 1} {Z0 + 40} minecraft:stone")
    time.sleep(2)
    tp(f"{X0 + 12}", f"{Y + 1}", f"{Z0 + 6}")
    time.sleep(5)
    cmd("tick freeze")
    ok = True
    index = 0
    for row in rows("bucket"):
        if "start" not in row or (only and only not in row["block"]):
            continue
        x, z = X0 + 3 * (index % 30) + 2, Z0 + 3 * (index // 30) + 2
        index += 1
        got = bucket_case(row["block"], row["start"], row["item"], row["mode"], x, z)
        want = {"state": row["state"], "above": row["above"], "hand": row["hand"], "water_buckets": row["water_buckets"], "lava_buckets": row["lava_buckets"], "buckets": row["buckets"]}
        # O estado de cima no original é `minecraft:air` (a descrição do bloco é `minecraft:air`); no jogo vem sem propriedades também.
        ok &= check(f"{row['block'].split(':')[1]} {row['start']} {row['item']} {row['mode']}", got, want)
    restore()
    return ok


def nbt(path):
    text = cmd(f"data get entity @s {path}")[0]
    found = re.search(r"data: (-?[\d.]+)", text)
    return float(found.group(1)) if found else None


def entity():
    """O jogador de sobrevivência numa célula de cabo (poço de pedra de um bloco, com teto): com água o fogo se apaga, com lava ele queima e perde vida, sem fluido nada acontece.
    O rastro do original (`fluido_no_bloco_entity.jsonl`, jogador) dá o mesmo: água `in_water` e fogo 0, lava `in_lava` com fogo 300 e a vida caindo de 20 a 16 no primeiro tick."""
    sky()
    cmd("tick unfreeze")
    cmd(f"fill {X0} {Y} {Z0} {X0 + 40} {Y + 8} {Z0 + 40} minecraft:air")
    cmd(f"fill {X0} {Y - 1} {Z0} {X0 + 40} {Y - 1} {Z0 + 40} minecraft:stone")
    time.sleep(3)
    ok = True
    for index, fluid in enumerate(("none", "water", "lava")):
        x, z = X0 + 6 + 8 * index, Z0 + 8
        cmd("gamemode survival")
        for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            cmd(f"setblock {x + dx} {Y} {z + dz} minecraft:stone")
            cmd(f"setblock {x + dx} {Y + 1} {z + dz} minecraft:stone")
        cmd(f"setblock {x} {Y + 2} {z} minecraft:stone")
        # A fonte guardada no cabo agenda o tick e correria para fora se houvesse saída: o poço a segura.
        cmd(f"setblock {x} {Y} {z} mekanism:basic_universal_cable[fluid_logged={'empty' if fluid == 'none' else fluid}]")
        tp(f"{x + 0.5}", f"{Y}", f"{z + 0.5}")
        time.sleep(2.0)
        health = nbt("Health")
        status = json.loads(ctl("status"))["player"]
        vy = status["velocity"][1]
        # O rastro do original (jogador): água `in_water`, movimento vertical -0,005 e vida 20; lava `in_lava`, movimento -0,02 e a vida caindo (20 a 16 no primeiro tick,
        # 9,5 no tick 20); sem fluido -0,0784 e vida 20.
        if fluid == "water":
            good = health == 20 and abs(vy + 0.005) < 0.011
        elif fluid == "lava":
            good = health is not None and health < 20 and abs(vy + 0.02) < 0.011
        else:
            good = health == 20 and vy < -0.05
        ok &= check(f"jogador na célula do cabo com {fluid}", good, True)
        print(f"     vida={health} movimento_y={vy}")
        tp(f"{X0 + 20}", f"{Y + 2}", f"{Z0 + 20}")
        cmd("gamemode creative")
        time.sleep(0.5)
    restore()
    return ok


MOB_TICKS = 59


def read_number(selector, path):
    """O primeiro número da resposta de `data get entity` (a lista vazia se a entidade não existe mais)."""
    text = " ".join(cmd(f"data get entity {selector} {path}"))
    text = text.split("data:")[-1]
    found = re.findall(r"-?\d+\.?\d*(?:[eE]-?\d+)?", text)
    return float(found[-1]) if found and "No entity" not in text and "found" not in text.lower() else None


def read_pos(selector):
    text = " ".join(cmd(f"data get entity {selector} Pos"))
    if "No entity" in text or "found" in text.lower():
        return None
    found = re.findall(r"-?\d+\.?\d*(?:[eE]-?\d+)?", text.split("data:")[-1])
    return [float(n) for n in found[:3]] if len(found) >= 3 else None


def mob_want():
    """Do rastro: por (fluido, tipo) e tick, o que o original deixou."""
    out = {}
    for row in rows("entity"):
        if row["kind"] in ("pig", "item"):
            out[(row["fluid"], row["kind"], row["tick"])] = row
    return out


def mob():
    """Um poço de pedra de 1 por 1 com a célula do cabo embaixo (o rastro do original), um porco e um item de pedra em cada: com água, com lava e sem fluido. O servidor fica parado e anda
    um tick por vez; a cada tick o porco dá o ar, a vida, o fogo e a altura, e o item dá se existe e a altura. Contra o original: a água tira 1 de ar por tick do porco e o item sobe até 1,0625;
    a lava queima o porco (a vida cai e ele morre no tick 21) e destrói o item no tick 2."""
    want = mob_want()
    sky()
    cmd("tick unfreeze")
    cmd(f"fill {X0} {Y} {Z0} {X0 + 40} {Y + 8} {Z0 + 40} minecraft:air")
    cmd(f"fill {X0} {Y - 1} {Z0} {X0 + 40} {Y - 1} {Z0 + 40} minecraft:stone")
    time.sleep(3)
    cmd("tick freeze")
    time.sleep(0.5)
    wells = []
    for index, (fluid, kind) in enumerate([(f, k) for f in ("water", "lava", "none") for k in ("pig", "item")]):
        x, z = X0 + 4 + 4 * index, Z0 + 10
        for dy in (0, 1):
            for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                cmd(f"setblock {x + dx} {Y + dy} {z + dz} minecraft:stone")
        cmd(f"setblock {x} {Y + 2} {z} minecraft:stone")
        cmd(f"setblock {x} {Y} {z} mekanism:basic_universal_cable[fluid_logged={'empty' if fluid == 'none' else fluid}]")
        wells.append((fluid, kind, x, z))
    time.sleep(0.5)
    for fluid, kind, x, z in wells:
        if kind == "pig":
            cmd(f"summon minecraft:pig {x + 0.5} {Y + 0.05} {z + 0.5} {{PersistenceRequired:1b,Silent:1b,NoAI:1b,Health:10f}}")
        else:
            cmd(f"summon minecraft:item {x + 0.5} {Y + 0.05} {z + 0.5} {{Item:{{id:\"minecraft:cobblestone\",count:1}},Age:-32768s,PickupDelay:32767s}}")
    time.sleep(0.5)

    def selector(kind, x, z):
        return f"@e[type=minecraft:{'pig' if kind == 'pig' else 'item'},limit=1,x={x + 0.5},y={Y},z={z + 0.5},distance=..2]"

    ok = True
    log = {}
    for tick in range(1, MOB_TICKS + 1):
        cmd("tick step 1")
        time.sleep(0.4)
        for fluid, kind, x, z in wells:
            sel = selector(kind, x, z)
            pos = read_pos(sel)
            row = {"alive": pos is not None, "y": round(pos[1], 4) if pos else None}
            if kind == "pig" and pos:
                row["air"] = read_number(sel, "Air")
                row["health"] = read_number(sel, "Health")
                row["fire"] = read_number(sel, "Fire")
            log[(fluid, kind, tick)] = row
    cmd("tick unfreeze")
    for fluid, kind, x, z in wells:
        # Os pontos que o original dá, de ponta a ponta: a vida e o ar do porco, se o porco e o item vivem, e a altura do item no fim.
        for tick in (1, 2, 5, 10, 21, 30, 59):
            got = log[(fluid, kind, tick)]
            ref = want[(fluid, kind, tick)]
            expected = {"alive": ref["alive"]}
            if kind == "pig":
                expected["air"] = float(ref["air"])
                expected["health"] = float(ref["health"])
            observed = {"alive": got["alive"]}
            if kind == "pig" and got["alive"]:
                observed["air"] = got.get("air")
                observed["health"] = got.get("health")
            elif kind == "pig":
                # Morto no jogo: o `data get` não acha mais a entidade; o original deixa a vida em 0 e o ar onde estava.
                expected = {"alive": ref["alive"]}
            ok &= check(f"{kind} {fluid} @{tick}", observed, expected)
        if kind == "item" and log[(fluid, kind, MOB_TICKS)]["alive"]:
            ok &= check(f"item {fluid} altura final", log[(fluid, kind, MOB_TICKS)]["y"] - Y, round(want[(fluid, kind, MOB_TICKS)]["pos"][1] - 1, 4))
    json.dump({f"{a}/{b}/{c}": v for (a, b, c), v in log.items()}, open(os.environ.get("MOB_LOG", "/tmp/fluido_no_bloco_mob.json"), "w"))
    cmd("kill @e[type=!minecraft:player]")
    restore()
    return ok


GEN = "mekanismgenerators:heat_generator"
CABLE = "mekanism:basic_universal_cable"
PARADO_TICKS = 12
REPEATS = 6
# A colisão de cada alvo (caixas `[x0, y0, z0, x1, y1, z1]` da célula): a laje de baixo do Heat Generator (as outras caixas só aumentam o `noPhysics`, e a laje basta para saber que o item
# está preso), o núcleo do cabo e o cacto.
FLOOR = [[-8.0, -1.0, -8.0, 9.0, 0.0, 9.0]]  # o andar de pedra em y = 149 (a célula fica em y = 150): um item preso que cai pelo vão sem fluido entra nele
COLLISION = {GEN: [[0.0, 0.0, 0.0, 1.0, 0.375, 1.0]], CABLE: [[0.3125, 0.3125, 0.3125, 0.6875, 0.6875, 0.6875]], "minecraft:cactus": [[0.0625, 0.0, 0.0625, 0.9375, 0.9375, 0.9375]]}


def parado_cases():
    """(nome, bloco, fluido, item, fogo): os casos do rastro `fluido_item_parado.jsonl` (o gerador com a pedra e o carvão repetidos, e os de item em chamas)."""
    out = []
    for repeat in range(REPEATS):
        out.append((f"heat_generator_lava_stone#{repeat}", GEN, "lava", "stone", 0))
        out.append((f"heat_generator_lava_coal#{repeat}", GEN, "lava", "coal", 0))
    out.append(("cable_lava_stone", CABLE, "lava", "stone", 0))
    for target, label in ((GEN, "heat_generator"), (CABLE, "cable")):
        for fluid in ("lava", "water", "empty"):
            out.append((f"{label}_{fluid}_burning", target, fluid, "stone", 100))
    out.append(("cactus_stone", "minecraft:cactus", "empty", "stone", 0))
    # O controle de saída: com uma fonte de lava de verdade no vão ao norte do gerador (para onde o item preso é empurrado), o item que sai da colisão cai na lava e pega fogo.
    for repeat in range(REPEATS):
        out.append((f"heat_generator_pool_stone#{repeat}", GEN, "empty", "stone", 0))
    only = [name for name in os.environ.get("PARADO_SO", "").split(",") if name]  # `PARADO_SO=a,b`: só os casos cujo nome começa por um deles (a mutação em jogo)
    return [case for case in out if not only or any(case[0].startswith(name) for name in only)]


def stuck(block, rel, half=0.125, height=0.25):
    """Se a caixa do item (pé em `rel`, relativo à célula) está dentro de alguma colisão do bloco: o `noPhysics` do tick que começa ali (`noCollision(box.deflate(1.0E-7))`)."""
    e = 1.0e-7
    lo, hi = (rel[0] - half + e, rel[1] + e, rel[2] - half + e), (rel[0] + half - e, rel[1] + height - e, rel[2] + half - e)
    return any(lo[0] < b[3] and hi[0] > b[0] and lo[1] < b[4] and hi[1] > b[1] and lo[2] < b[5] and hi[2] > b[2] for b in COLLISION[block] + FLOOR)


def run_parado(spots, log):
    """A montagem e os ticks de `parado`: um tick por vez com o servidor parado, a posição, o fogo e a vida de cada item a cada tick, em `log`."""
    sky()
    cmd("tick unfreeze")
    for z in (Z0 + 8, Z0 + 16, Z0 + 24):
        cmd(f"fill {X0} {Y} {z} {X0 + 60} {Y + 8} {z + 6} minecraft:air")
    cmd(f"fill {X0} {Y - 1} {Z0 + 8} {X0 + 60} {Y - 1} {Z0 + 32} minecraft:stone")
    time.sleep(3)
    cmd("tick freeze")
    time.sleep(0.5)
    for name, block, fluid, item, fire, x, z in spots:
        if block == "minecraft:cactus":
            cmd(f"setblock {x} {Y - 1} {z} minecraft:sand")
            cmd(f"setblock {x} {Y} {z} minecraft:cactus")
        else:
            cmd(f"setblock {x} {Y} {z} {block}[fluid_logged={fluid}]")
        if "_pool_" in name:
            cmd(f"setblock {x} {Y} {z - 1} minecraft:lava")
    time.sleep(0.5)
    for name, block, fluid, item, fire, x, z in spots:
        nbt = f'Item:{{id:"minecraft:{item}",count:1}},Age:-32768s,PickupDelay:32767s,Motion:[0.0,0.0,0.0]' + (f",Fire:{fire}s" if fire else "")
        cmd(f"summon minecraft:item {x + 0.5} {Y + 0.05} {z + 0.5} {{{nbt}}}")
    time.sleep(0.5)
    for tick in range(1, PARADO_TICKS + 1):
        cmd("tick step 1")
        time.sleep(0.4)
        for name, block, fluid, item, fire, x, z in spots:
            sel = f"@e[type=minecraft:item,limit=1,x={x + 0.5},y={Y},z={z + 0.5},distance=..3]"
            pos = read_pos(sel)
            log[(name, tick)] = {"alive": pos is not None, "pos": pos, "fire": read_number(sel, "Fire") if pos else None, "health": read_number(sel, "Health") if pos else None}
    cmd("tick unfreeze")


def parado():
    """Cada caso do rastro numa célula sua (o bloco com o fluido e um item de pedra ou carvão parado a 0,05 de altura no meio, com ou sem fogo), servidor parado, um tick por vez. O
    `noPhysics` de cada tick se calcula da posição em que o tick começa e da colisão do bloco. Contra o original (decisão 100): enquanto o item está preso ele não pega fogo na lava,
    a água não apaga o que já queimava, o cacto não o fere, e o fogo só desce 1 por tick; no tick em que a caixa sai da colisão a lava o acende (fogo 300), e o do cabo, que nunca fica
    preso, queima no primeiro tick e morre no segundo."""
    cases = parado_cases()
    reuse = os.environ.get("PARADO_REUSE")  # um log salvo de uma rodada: confere de novo sem abrir o jogo
    spots = [(name, block, fluid, item, fire, X0 + 4 + 5 * (index % 11), Z0 + 10 + 8 * (index // 11)) for index, (name, block, fluid, item, fire) in enumerate(cases)]
    log = {}
    if reuse:
        log = {(key.rsplit("/", 1)[0], int(key.rsplit("/", 1)[1])): value for key, value in json.load(open(reuse)).items()}
    else:
        run_parado(spots, log)
    cmd("tick unfreeze")
    ok = True
    summary = []
    exits = 0
    trace = {}
    for line in open(f"{TRACES}/fluido_item_parado.jsonl"):
        row = json.loads(line)
        trace.setdefault(row["case"], []).append(row)
    for name, block, fluid, item, fire, x, z in spots:
        start = (0.5, 0.05, 0.5)
        pos_before = start
        held = True  # o item esteve preso em todos os ticks até aqui (o fogo do original nesses ticks é o do `baseTick`)
        expected_fire = fire
        first_left = None
        for tick in range(1, PARADO_TICKS + 1):
            row = log[(name, tick)]
            np = stuck(block, pos_before)
            free = not np and pos_before[1] < 0.12  # sem colisão por certo: abaixo da laje de 0,375 só ela conta
            after = row["pos"]
            if row["pos"]:
                pos_before = (row["pos"][0] - x, row["pos"][1] - Y, row["pos"][2] - z)
            if "_pool_" in name and free and after and (after[2] - z) < 0.0 and (after[2] - z) >= -1.0:
                # Saiu da colisão e a caixa está na célula da lava: acende neste tick (ou já morreu).
                burned = (row["fire"] or 0) >= 299
                ok &= check(f"{name} t={tick} saiu para a lava", burned, True)
                exits += 1
            if not held:
                continue
            if not np:
                held = False
                first_left = tick
                continue
            # Preso neste tick: sem efeito de bloco. O fogo é o do `baseTick` (desce 1 por tick); a água e a lava não mexem nele; o item vive.
            expected_fire = max(expected_fire - 1, 0)
            got = row["fire"]
            # A vida: o `baseTick` fere 1 no primeiro tick de quem chega em chamas (fogo 100, `% 20 == 0`, e `isInLava` é falso no primeiro tick), e mais nada nestes ticks.
            ok &= check(f"{name} t={tick} preso", {"alive": row["alive"], "fire": max(int(got), 0) if got is not None else 0, "health": row["health"]},
                        {"alive": True, "fire": expected_fire, "health": 4.0 if fire else 5.0})
        summary.append(f"{name}: preso até o tick {(first_left - 1) if first_left else PARADO_TICKS}")
        if name.startswith("cable"):
            # O vão do cabo nunca prende o item (sem `noPhysics`): sem rastro de acaso, o original e o port andam igual em todos os ticks: se vive, o fogo e a vida.
            for row in trace[name]:
                got = log[(name, row["t"])]
                want = {"alive": not row["removed"]}
                seen = {"alive": got["alive"]}
                if got["alive"] and not row["removed"]:
                    want.update({"fire": max(row["fire"], 0), "health": float(row["health"])})
                    seen.update({"fire": max(int(got["fire"]), 0), "health": got["health"]})
                ok &= check(f"{name} t={row['t']} no vão (rastro)", seen, want)
    print("\n".join(summary))
    # O caminho de antes volta a valer quando o item sai: ao menos um dos do vão com lava saiu da colisão para ela (e então pegou fogo, conferido acima).
    print(f"itens que saíram da colisão para a lava do vão: {exits}")
    ok &= check("saídas para a lava conferidas", exits >= 1, True)
    json.dump({f"{a}/{b}": v for (a, b), v in log.items()}, open(os.environ.get("MOB_LOG", "/tmp/fluido_item_parado.json"), "w"))
    if not reuse:
        cmd("kill @e[type=!minecraft:player]")
        restore()
    return ok


# A cena do print e do vídeo: uma fileira em cima do andar de pedra de y = 149 (a plataforma de `h2_nota_instrumento.py`, com céu), de frente para o norte.
SCENE_Z = 80
SCENE = (("mekanism:basic_universal_cable", 66, "empty"), ("mekanism:basic_fluid_tank", 70, "water"), ("mekanism:basic_energy_cube", 74, "lava"))
CAMERA = (68.5, 151, 83.5)


def scene_clear():
    sky()
    cmd("tick unfreeze")
    cmd(f"fill 55 {Y} 74 90 {Y + 10} 96 minecraft:air")
    cmd(f"fill 55 {Y - 1} 74 90 {Y - 1} 96 minecraft:stone")
    time.sleep(3)
    cmd("gamemode survival")
    cmd("effect give @s minecraft:fire_resistance 999 255 true")
    cmd("clear @s")


def scene_build(still=False):
    """Os três blocos postos; `still`: com o fluido dentro (a água no tanque e a lava no cubo) e sem fluxo ainda, que só começa quando se mexe num vizinho."""
    for block, x, fluid in SCENE:
        cmd(f"setblock {x} {Y} {SCENE_Z} {block}[fluid_logged={fluid if still else 'empty'}]")


def aim(x):
    ctl("look-at", "block", str(x), str(Y), str(SCENE_Z), "south")


def scene(folder):
    """Os três blocos com o fluido dentro, parados (o fluido só corre depois que se mexe num vizinho): o print."""
    scene_clear()
    cmd("tick freeze")
    scene_build(still=True)
    cmd(f"setblock 66 {Y} {SCENE_Z} mekanism:basic_universal_cable[fluid_logged=water]")
    ctl("options-set", "chatVisibility", "2")
    tp(70.5, 152.0, 89.5)
    time.sleep(1.0)
    ctl("look", "180", "17")
    time.sleep(1.5)
    ctl("screenshot", f"{folder}/cena.png")
    print("print cena")
    restore()


def video():
    """O roteiro do vídeo: o balde de água enche o cabo (a água aparece dentro dele), o balde vazio a tira, de novo a água, e depois o tanque com água e o cubo com lava postos
    correm para fora (o vizinho mexido agenda o tick). Com `REC=1` o roteiro abre sozinho o `gravar-jogo rec` (34 s, com o som do jogo) quando começa."""
    import subprocess
    scene_clear()
    cmd(f"setblock 66 {Y} {SCENE_Z} mekanism:basic_universal_cable[fluid_logged=empty]")
    # Um pilar de pedra levanta o jogador acima da água que vai correr.
    cmd(f"setblock 68 {Y} 83 minecraft:stone")
    ctl("options-set", "chatVisibility", "2")
    cmd("give @s minecraft:water_bucket")
    ctl("select", "1")
    tp(*CAMERA)
    time.sleep(1.0)
    aim(66)
    time.sleep(1.5)
    rec = None
    if os.environ.get("REC"):
        rec = subprocess.Popen(["ssh", "desktop", "~/.cache/rust-mekanism/gravar-jogo rec fluido-no-bloco fnb.mp4 34 horizon"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(3.0)
    for _ in range(3):
        aim(66)
        time.sleep(0.5)
        ctl("use-item")
        time.sleep(2.5)
    ctl("look", "180", "24")
    # O tanque e o cubo entram com o fluido dentro e começam a correr com o vizinho mexido.
    for block, x, fluid in SCENE[1:]:
        cmd(f"setblock {x} {Y} {SCENE_Z} {block}[fluid_logged={fluid}]")
        cmd(f"setblock {x + 1} {Y} {SCENE_Z} minecraft:stone")
        cmd(f"setblock {x + 1} {Y} {SCENE_Z} minecraft:air")
    time.sleep(12.0)
    if os.environ.get("FOTO"):
        ctl("screenshot", os.environ["FOTO"])
    if rec:
        rec.wait(timeout=60)
    restore()


def mob_video():
    """O roteiro do vídeo com som: três poços de vidro de 1 por 1 com a célula do cabo embaixo; o porco do primeiro recebe água, o do segundo lava (queima e morre em um segundo) e, no
    terceiro, também com lava, cai um item de pedra, que a lava destrói. A resposta de `data get entity` mostra o ar do porco na água caindo. O som só sobe depois de o stream do
    jogo estar num sink de teste, e uma thread confere a cada meio segundo (`seguro`) e manda o stream novo para o sink da gravação (o stream nasce no `som_nulo`)."""
    import subprocess
    import threading
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import h2_nota_instrumento as n

    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "34"), os.environ.get("ARQUIVO", "mob_no_cabo.mp4")
    scene_clear()
    cmd("gamerule send_command_feedback true")
    cmd("effect give @s minecraft:fire_resistance 999 255 true")
    wells = [(63, "water"), (67, "lava"), (71, "lava")]
    z = SCENE_Z
    for x, fluid in wells:
        for dy in (0, 1):
            for dx, dz in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                cmd(f"setblock {x + dx} {Y + dy} {z + dz} minecraft:glass")
        cmd(f"setblock {x} {Y + 2} {z} minecraft:glass")
        cmd(f"setblock {x} {Y} {z} mekanism:basic_universal_cable[fluid_logged=empty]")
    time.sleep(1.0)
    pig = "{PersistenceRequired:1b,Silent:0b,NoAI:1b,Health:10f}"
    cmd(f"summon minecraft:pig 63.5 {Y + 0.05} {z + 0.5} {pig}")
    cmd(f"summon minecraft:pig 67.5 {Y + 0.05} {z + 0.5} {pig}")
    cmd(f"tp @s 67.0 {Y + 1.4} {z + 5.5} 180 10")
    ctl("options-set", "chatVisibility", "0")
    n.mute()
    for categoria in n.SOUND_OFF:
        if categoria != "neutral":
            ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "neutral", "player"):
        ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()
    grav = f"som_grav_{nome}"
    ate = [None]

    def vigia():
        while not parar.is_set():
            if ate[0] is not None and time.time() < ate[0]:
                subprocess.run(["ssh", "desktop", f"AUDIO_SINK={grav} {n.AUDIO_NULO} ligar {n.CONTAINER}"], capture_output=True, text=True)
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    try:
        ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 10
        # A água entra no primeiro cabo e a lava no segundo e no terceiro.
        for x, fluid in wells:
            cmd(f"setblock {x} {Y} {z} mekanism:basic_universal_cable[fluid_logged={fluid}]")
        time.sleep(3.0)
        # O ar do porco na água: 1 por tick, 20 por segundo.
        for _ in range(4):
            cmd("data get entity @e[type=minecraft:pig,limit=1,x=63.5,y=150,z=80.5,distance=..2] Air")
            time.sleep(2.0)
        cmd(f"summon minecraft:item 71.5 {Y + 0.05} {z + 0.5} {{Item:{{id:\"minecraft:cobblestone\",count:1}},Age:-32768s,PickupDelay:32767s}}")
        time.sleep(6.0)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        cmd("kill @e[type=!minecraft:player]")
        cmd("effect clear @s")
        ctl("options-set", "chatVisibility", "0")
        restore()


def parado_video():
    """O roteiro do vídeo com som: o Heat Generator com lava (à esquerda) e o cabo com lava (à direita), lado a lado, e em cada um o mesmo item de pedra ou de carvão parado na célula. O
    servidor anda em câmera lenta (`tick step 1` um atrás do outro, cerca de dois ticks por segundo): o item do cabo pega fogo no primeiro tick e morre no segundo, com o som da queima; o do
    gerador, preso na colisão, fica na lava sem pegar fogo e sem som até a colisão o pôr para fora. Cinco rodadas, a lava reposta a cada uma. O som só sobe depois de o stream do jogo
    estar num sink de teste, e uma thread confere a cada meio segundo (`seguro`) e manda o stream novo para o sink da gravação."""
    import subprocess
    import threading
    sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
    import h2_nota_instrumento as n

    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "46"), os.environ.get("ARQUIVO", "item_parado.mp4")
    scene_clear()
    cmd("gamerule send_command_feedback false")
    gen_x, cable_x, z = 66, 69, SCENE_Z
    ctl("options-set", "chatVisibility", "2")

    def reset(item):
        for x in (gen_x, cable_x):
            cmd(f"setblock {x} {Y} {z} minecraft:air")
        cmd("kill @e[type=minecraft:item]")
        cmd(f"setblock {gen_x} {Y} {z} {GEN}[facing=south,fluid_logged=lava]")
        cmd(f"setblock {cable_x} {Y} {z} {CABLE}[fluid_logged=lava]")
        for x in (gen_x, cable_x):
            cmd(f"summon minecraft:item {x + 0.5} {Y + 0.05} {z + 0.5} {{Item:{{id:\"minecraft:{item}\",count:1}},Age:-32768s,PickupDelay:32767s,Motion:[0.0,0.0,0.0]}}")

    cmd("tick freeze")
    reset("cobblestone")
    cmd(f"tp @s 67.5 {Y + 1.7} {z + 3.2} 180 28")
    n.mute()
    for categoria in n.SOUND_OFF:
        if categoria != "neutral":
            ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "neutral", "player", "hostile"):
        ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()
    grav = f"som_grav_{nome}"
    ate = [None]

    def vigia():
        while not parar.is_set():
            if ate[0] is not None and time.time() < ate[0]:
                subprocess.run(["ssh", "desktop", f"AUDIO_SINK={grav} {n.AUDIO_NULO} ligar {n.CONTAINER}"], capture_output=True, text=True)
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    try:
        ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 10
        inicio = time.time()
        for rodada, item in enumerate(("cobblestone", "coal", "cobblestone", "coal", "cobblestone", "coal", "cobblestone")):
            if time.time() - inicio > int(segundos) - 12:
                break
            if rodada:
                reset(item)
            time.sleep(1.5)
            for _ in range(12):
                cmd("tick step 1")
            time.sleep(1.0)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        cmd("kill @e[type=!minecraft:player]")
        cmd("setblock 66 150 80 minecraft:air")
        cmd("setblock 69 150 80 minecraft:air")
        ctl("options-set", "chatVisibility", "0")
        restore()


def main():
    what = sys.argv[1] if len(sys.argv) > 1 else "flow"
    if what == "flow":
        sys.exit(0 if flow() else 1)
    if what == "cena":
        scene(sys.argv[2])
        return
    if what == "video":
        video()
        return
    if what == "entity":
        sys.exit(0 if entity() else 1)
    if what == "bucket":
        sys.exit(0 if bucket() else 1)
    if what == "mob_video":
        mob_video()
        return
    if what == "mob":
        sys.exit(0 if mob() else 1)
    if what == "parado":
        sys.exit(0 if parado() else 1)
    if what == "parado_video":
        parado_video()
        return
    print("uso:", __doc__)


if __name__ == "__main__":
    main()
