#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): os blocos de máquina e os blocos avulsos com tile do Mekanism, sem função.

Uso: H_NAME=horizon h2_blocos.py give|estados|cena [PASTA_DOS_PRINTS]
- give: todo id responde ao `give`;
- estados: cada bloco, em até cinco estados do original (o primeiro, o último, `active=true`, cada `facing`), é posto por `setblock` e lido de volta por `block`;
- cena: o roteiro `cenas/blocos.json`, o mesmo que `testes/CenaBlocos.java` roda no original, com os prints em PASTA_DOS_PRINTS (padrão `SHOTS`, na pasta de dados do jogo; `scp` os traz).
Supõe o jogo aberto num mundo plano (o chão em y -61) com o jogador criativo, e a janela de 1920 por 1080.
"""
import json, os, subprocess, sys, time

import h2_identity as h

ROOT = os.path.abspath(os.path.join(h.TOOLS, "..", "..", ".."))
RASTRO = os.path.join(ROOT, "dev/referencia/rastros/blocos.jsonl")
# O `horizon-ctl screenshot` escreve dentro do container, onde só a pasta de dados do jogo é a mesma do desktop.
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/blocos"
SCOPE = os.path.join(ROOT, "mods/mekanism/content/data/mekanism/horizon/block")


# O que o port tinha antes dos blocos de máquina (e os blocos de recurso de outra frente): fora desta conferência.
EARLIER = {"enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser", "steel_casing", "teleporter_frame"} | {
    f"{t}_universal_cable" for t in ("basic", "advanced", "elite", "ultimate")}


def scope():
    ported = {f[:-5] for f in os.listdir(SCOPE)}
    rows = {}
    for line in open(RASTRO):
        row = json.loads(line)
        name = row["id"].split(":")[1]
        if name in ported and name not in EARLIER and not name.startswith("block_") and not name.endswith("_ore"):
            rows[name] = row
    return rows


def give():
    bad = []
    rows = scope()
    for name in rows:
        chat = h.command(f"give @s mekanism:{name} 1")
        text = " ".join(chat) if isinstance(chat, list) else str(chat)
        if "Unknown" in text or "No item" in text or "error" in text.lower():
            bad.append((name, text[:120]))
    print(f"give: {len(rows)} ids, {len(bad)} recusados {bad}")
    return not bad


def block_state(pos):
    return json.loads(h.ctl("block", *map(str, pos)))["state"]


def state_text(props):
    return "[" + ",".join(f"{k}={v}" for k, v in sorted(props.items())) + "]" if props else ""


def estados():
    """Põe e lê de volta cinco estados de cada bloco. Os blocos vão para y -52, no ar, em colunas separadas."""
    h.command("tick unfreeze")
    bad, total = [], 0
    for i, (name, row) in enumerate(sorted(scope().items())):
        picks = [row["states"][0]["props"], row["states"][-1]["props"]]
        picks += [s["props"] for s in row["states"] if s["props"].get("active") == "true"][:1]
        picks += [s["props"] for s in row["states"] if s["props"].get("facing") in ("east", "west")][:2]
        pos = (56 + (i % 20) * 3, -52 - (i // 20) * 3, 70)
        for props in picks:
            text = state_text(props)
            h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} mekanism:{name}{text}")
            got = block_state(pos)
            want = f"mekanism:{name}{text}"
            total += 1
            if got != want:
                bad.append((want, got))
    print(f"estados: {total} lidos, {len(bad)} diferentes", bad[:5])
    return not bad


def tag_items(tag):
    """Os itens de uma tag de item do conteúdo, com as tags aninhadas resolvidas."""
    namespace, path = tag.split(":")
    file = os.path.join(ROOT, f"mods/mekanism/content/data/{namespace}/tags/item/{path}.json")
    out = []
    for value in json.load(open(file))["values"]:
        out += tag_items(value[1:]) if value.startswith("#") else [value]
    return out


def mesa(name):
    """Uma receita de mesa de trabalho do conteúdo (`data/mekanism/recipe/<name>.json`): põe cada ingrediente na grade como um jogador (pega a pilha da barra, clica com o direito
    em cada célula) e confere o resultado."""
    recipe = json.load(open(os.path.join(ROOT, f"mods/mekanism/content/data/mekanism/recipe/{name}.json")))
    key = {k: (tag_items(v[1:])[0] if v.startswith("#") else v) for k, v in recipe["key"].items()}
    pattern = recipe["pattern"]
    h.command("gamemode survival")
    h.command("clear @s")
    needs = {}
    for row in pattern:
        for c in row:
            if c != " ":
                needs[key[c]] = needs.get(key[c], 0) + 1
    order = list(needs)
    for item in order:
        h.command(f"give @s {item} {needs[item]}")
    h.command("setblock 72 -60 60 minecraft:crafting_table")
    h.ctl("tp", "72", "-60", "58")
    time.sleep(0.6)
    h.ctl("interact", "72", "-60", "60")
    time.sleep(0.8)
    # O menu da mesa: a grade 3x3 são os slots 100 a 108 em ordem de linha.
    for index, item in enumerate(order):
        h.ctl("click-slot", str(index))
        for r, row in enumerate(pattern):
            for c, char in enumerate(row):
                if char != " " and key[char] == item:
                    h.ctl("click-slot", str(100 + r * 3 + c), "right")
        h.ctl("click-slot", str(index))
    time.sleep(0.8)
    menu = json.loads(h.ctl("menu"))
    result = next((s["stack"] for s in menu["slots"] if s["slot"] == 200 and s.get("stack")), None)
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS}"], check=True)
    h.ctl("screenshot", f"{SHOTS}/porte_mesa_{name}.png")
    h.ctl("close")
    h.command("gamemode creative")
    h.command("clear @s")
    want = recipe["result"]["id"]
    print(f"mesa {name}:", result and (result["item"], result["count"]))
    return bool(result) and result["item"] == want and result["count"] == recipe["result"].get("count", 1)


def player_yaw():
    return json.loads(h.ctl("state"))["player"].get("yaw") or json.loads(h.ctl("status"))["yaw"]


def expected_facing(rule, yaw, pitch, face):
    """`AttributeStateFacing.getStateForPlacement` em Python (a conta do original, à mão)."""
    import math
    values = rule["values"]
    if rule["mode"] == "selected_face":
        return face if face in values else "north"
    chosen = "south"
    if "up" in values and "down" in values:
        height = math.floor(pitch + 0.5)
        if height >= 65:
            chosen = "up"
        elif height <= -65:
            chosen = "down"
    if chosen not in ("up", "down"):
        chosen = ["north", "east", "south", "west"][math.floor(yaw * 4.0 / 360.0 + 0.5) & 3]
    return chosen


def status():
    return json.loads(h.ctl("status"))


def colocar():
    """Cada bloco posto como jogador (clique direito com o item na mão, `interact` numa face da pedra), de posições diferentes: o `facing` vem da guinada e da inclinação que o jogo
    registrou no clique, ou da face clicada, e o `fluid_logged` do fluido do lugar. O esperado é a conta do original (`AttributeStateFacing`), feita à mão."""
    ok, total = True, 0
    # Onde o jogador fica e a face da pedra (em 80, -60, 80) que ele clica; a de cima, de quatro lados.
    stands = [("east", (82.5, -60, 80.5)), ("west", (77.5, -60, 80.5)), ("south", (80.5, -60, 82.5)), ("north", (80.5, -60, 77.5)),
              ("up", (82.5, -60, 82.5)), ("up", (77.5, -60, 82.5)), ("up", (77.5, -60, 77.5)), ("up", (82.5, -60, 77.5))]
    side = {"east": (81, -60, 80), "west": (79, -60, 80), "south": (80, -60, 81), "north": (80, -60, 79), "up": (80, -59, 80)}
    for name in ["combiner", "personal_barrel", "laser", "logistical_sorter", "chemical_infuser", "quantum_entangloporter", "chargepad", "qio_importer"]:
        rule = json.load(open(os.path.join(ROOT, f"mods/mekanism/content/data/mekanism/machinery_basis/placement/{name}.json")))
        h.command("clear @s")
        h.command(f"give @s mekanism:{name} 1")
        h.ctl("select", "1")
        for face, stand in stands:
            h.command("fill 70 -60 70 90 -52 90 minecraft:air")
            h.command("setblock 80 -60 80 minecraft:stone")
            h.ctl("tp", *map(str, stand))
            h.ctl("look-at", "80.5", "-59.5", "80.5")
            h.ctl("interact", "80", "-60", "80", face)
            time.sleep(0.3)
            pos = side[face]
            state = block_state(pos)
            now = status()
            want = expected_facing(rule["facing"], now["yaw"], now["pitch"], face)
            got = state.split("facing=")[1].split(",")[0].split("]")[0] if "facing=" in state else None
            total += 1
            if got != want:
                ok = False
            print(f"{name} em {face} de {stand[0]},{stand[2]}: yaw {now['yaw']:.1f} pitch {now['pitch']:.1f} -> {got} (esperado {want})")
            h.command("clear @s")
            h.command(f"give @s mekanism:{name} 1")
            h.ctl("select", "1")
    # O fluido do lugar: a água (fonte) enche o bloco, a lava também, o ar não.
    for fluid, want in (("water", "water"), ("lava", "lava"), ("air", "empty")):
        h.command("fill 70 -60 70 90 -52 90 minecraft:air")
        h.command("setblock 80 -60 80 minecraft:stone")
        h.command(f"setblock 81 -60 80 minecraft:{fluid}")
        h.command("clear @s")
        h.command("give @s mekanism:chemical_infuser 1")
        h.ctl("select", "1")
        h.ctl("tp", "82.5", "-60", "80.5")
        h.ctl("look-at", "80.5", "-59.5", "80.5")
        h.ctl("interact", "80", "-60", "80", "east")
        time.sleep(0.3)
        state = block_state((81, -60, 80))
        got = state.split("fluid_logged=")[1].rstrip("]") if "fluid_logged=" in state else state
        total += 1
        ok &= got == want
        print(f"chemical_infuser com {fluid} no lugar: {state} (esperado fluid_logged={want})")
    h.command("fill 70 -60 70 90 -52 90 minecraft:air")
    print(f"colocar: {total} casos")
    return ok


def virados():
    """Quatro Combiners postos como jogador de quatro lados (o `facing` vem da guinada de cada um), vistos de frente, de longe: cada um vira a frente para quem o pôs (norte, leste, sul e oeste)."""
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS}"], check=True)
    h.command("fill 70 -60 70 100 -52 100 minecraft:air")
    h.command("gamemode creative")
    h.command("clear @s")
    h.command("give @s mekanism:combiner 1")
    h.ctl("select", "1")
    for i, (dx, dz) in enumerate([(0, -3), (3, 0), (0, 3), (-3, 0)]):
        stone = (76 + 4 * i, -60, 80)
        h.command(f"setblock {stone[0]} {stone[1]} {stone[2]} minecraft:stone")
        h.ctl("tp", f"{stone[0] + 0.5 + dx}", "-60", f"{stone[2] + 0.5 + dz}")
        h.ctl("look-at", f"{stone[0] + 0.5}", "-59.5", f"{stone[2] + 0.5}")
        h.ctl("interact", *map(str, stone), "up")
        time.sleep(0.4)
        print(block_state((stone[0], stone[1] + 1, stone[2])))
        h.command("give @s mekanism:combiner 1")
        h.ctl("select", "1")
    h.ctl("key", "f1", "tap")
    h.command("tp @s 82.5 -60 74.5 0 20")
    time.sleep(2.5)
    h.ctl("screenshot", f"{SHOTS}/porte_virados.png")
    h.ctl("key", "f1", "tap")
    return True


def luz():
    """A luz de cada estado (`ctl light` na célula do bloco), contra o original (`rastros/blocos.jsonl`), num mundo limpo (superplano novo, sem teste anterior): as luzes se medem em duas
    levas. Os estados sem luz (nenhuma fonte no mundo) ficam todos postos de uma vez, uma célula cada, e se leem depois; os com luz ficam a 16 blocos um do outro (a luz de 15 chega a 15
    blocos), postos de uma vez e lidos depois. Nada é apagado: não há resto de luz para ler. Estados com água ou lava dentro ficam de fora (a lava dá 15 em volta)."""
    rows = scope()
    dark, lit = [], []
    for name, row in sorted(rows.items()):
        seen = set()
        for state in row["states"]:
            # Um estado por (`active`, luz): a luz só depende do `active` e do fluido (a tabela de linhas é por bloco).
            key = (state["props"].get("active"), state["light"])
            if state["props"].get("fluid_logged") in ("water", "lava") or key in seen:
                continue
            seen.add(key)
            (lit if state["light"] > 0 else dark).append((name, state["props"], state["light"]))
    h.command("tick unfreeze")
    bad = []

    def medir(casos, posicoes):
        for (name, props, _), pos in zip(casos, posicoes):
            h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} mekanism:{name}{state_text(props)}")
        time.sleep(3.0)
        for (name, props, want), pos in zip(casos, posicoes):
            got = json.loads(h.ctl("light", *map(str, pos), *map(str, pos)))["block"]
            if int(got, 16) != want:
                bad.append((name, props, want, got))

    # A leva sem luz: uma grade de 4 em 4 blocos, 30 por fileira; a com luz, longe dela, de 16 em 16, 8 por fileira.
    medir(dark, [(40 + (i % 30) * 4, -60, 40 + (i // 30) * 4) for i in range(len(dark))])
    medir(lit, [(40 + (i % 8) * 16, -60, 200 + (i // 8) * 16) for i in range(len(lit))])
    print(f"luz: {len(dark)} estados sem luz e {len(lit)} com luz, {len(bad)} diferentes", bad)
    return not bad


def cena(out):
    spec = json.load(open(os.path.join(ROOT, "dev/referencia/cenas/blocos.json")))
    subprocess.run(["ssh", "desktop", f"mkdir -p {out}"], check=True)
    h.command("tick unfreeze")
    for cmd in spec["setup"]:
        h.command(cmd)
    time.sleep(1.0)
    h.ctl("key", "f1", "tap")
    for shot in spec["shots"]:
        h.command(spec["clear"])
        for cmd in shot["cmds"]:
            h.command(cmd)
        h.command("tp @s " + shot["tp"])
        time.sleep(2.5)
        h.ctl("screenshot", f"{out}/porte_{shot['name']}.png")
        print("print", shot["name"])
    h.ctl("key", "f1", "tap")


def main():
    mode = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else SHOTS
    ok = {"give": give, "estados": estados, "colocar": colocar, "luz": luz, "virados": virados, "mesa": lambda: mesa(sys.argv[2])}.get(mode, lambda: cena(out))()
    sys.exit(0 if ok in (True, None) else 1)


if __name__ == "__main__":
    main()
