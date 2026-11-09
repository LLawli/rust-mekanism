#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): os blocos e itens do Mekanism Generators, sem função (frente `generators-conteudo`).

Uso: H_NAME=horizon h2_generators.py give|estados|luz|colocar|tags|mesa NOME|tab|cena [PASTA_DOS_PRINTS]
- give: todo bloco, item e balde da frente responde ao `give`;
- estados: cada bloco, em até cinco estados do original (o primeiro, o último, `active=true`, cada `facing`, cada `mode`), é posto por `setblock` e lido de volta por `block`;
- luz: a luz de cada estado (`ctl light`), contra o original (`rastros/blocos_generators.jsonl`);
- colocar: cada gerador posto como jogador (guinada, inclinação, face e fluido do lugar) e o `facing` e o `fluid_logged` lidos de volta;
- tags: as tags do vanilla que a datagen do Generators amplia valem em jogo (`execute if block ... #tag`; o comando não conhece as tags de mod);
- mesa NOME: a receita de mesa `data/mekanismgenerators/recipe/NOME.json`, posta na grade como um jogador;
- tab: prints da aba do Generators e do fim das abas do vanilla em que ele acrescenta itens;
- cena: o roteiro `cenas/generators.json`, o mesmo que `testes/CenaBlocos.java` roda no original, com os prints em PASTA_DOS_PRINTS (padrão `SHOTS`).
Supõe o jogo aberto num mundo plano (o chão em y -61) com o jogador criativo, e a janela de 1920 por 1080.
"""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_blocos as hb

ROOT = hb.ROOT
RASTRO = os.path.join(ROOT, "dev/referencia/rastros/blocos_generators.jsonl")
ITENS = os.path.join(ROOT, "dev/referencia/rastros/blocos_generators_itens.jsonl")
CONTENT = os.path.join(ROOT, "mods/mekanismgenerators/content")
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/generators"
NS = "mekanismgenerators"
# O heat_generator tem o gabarito do forno (`lit`, Fase 5): fora da conferência de estados.
EARLIER = {"heat_generator"}


def rows():
    out = {}
    for line in open(RASTRO):
        row = json.loads(line)
        out[row["id"].split(":")[1]] = row
    return out


def scope():
    return {name: row for name, row in rows().items() if name not in EARLIER}


def give():
    bad = []
    ids = list(rows()) + ["solar_panel", "hohlraum", "turbine_blade"] + [f"{f}_bucket" for f in ("bioethanol", "deuterium", "fusion_fuel", "tritium")]
    h.command("gamemode creative")
    for name in ids:
        chat = h.command(f"give @s {NS}:{name} 1")
        text = " ".join(chat) if isinstance(chat, list) else str(chat)
        if "Gave" not in text:
            bad.append((name, text[:120]))
    print(f"give: {len(ids)} ids, {len(bad)} recusados {bad}")
    return not bad


def estados():
    h.command("tick unfreeze")
    bad, total = [], 0
    for i, (name, row) in enumerate(sorted(scope().items())):
        picks = [row["states"][0]["props"], row["states"][-1]["props"]]
        picks += [s["props"] for s in row["states"] if s["props"].get("active") == "true"][:1]
        picks += [s["props"] for s in row["states"] if s["props"].get("facing") in ("east", "west")][:2]
        picks += [s["props"] for s in row["states"] if "mode" in s["props"]]
        pos = (56 + (i % 20) * 3, -52 - (i // 20) * 3, 70)
        for props in picks:
            text = hb.state_text(props)
            h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} {NS}:{name}{text}")
            got = hb.block_state(pos)
            want = f"{NS}:{name}{text}"
            total += 1
            if got != want:
                bad.append((want, got))
    print(f"estados: {total} lidos, {len(bad)} diferentes", bad[:5])
    return not bad


def luz():
    """A luz de cada estado (`ctl light` na célula do bloco), num mundo limpo: os sem luz de uma vez, os com luz a 16 blocos um do outro."""
    dark, lit = [], []
    for name, row in sorted(scope().items()):
        seen = set()
        for state in row["states"]:
            key = (state["props"].get("active"), state["light"])
            if state["props"].get("fluid_logged") in ("water", "lava") or key in seen:
                continue
            seen.add(key)
            (lit if state["light"] > 0 else dark).append((name, state["props"], state["light"]))
    h.command("tick unfreeze")
    bad = []

    def medir(casos, posicoes):
        """Em levas de uma fileira da grade: o jogador vai para o meio dela (fora da distância de carga o `setblock` não pega e a luz lê `.`), os blocos se põem, e a luz se lê depois."""
        fileiras = {}
        for item in zip(casos, posicoes):
            fileiras.setdefault(item[1][2], []).append(item)
        for z, fila in sorted(fileiras.items()):
            meio = sum(pos[0] for _, pos in fila) // len(fila)
            h.ctl("tp", str(meio), "-58", str(z))
            h.ctl("wait-ready")
            time.sleep(1.5)
            for (name, props, _), pos in fila:
                h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} {NS}:{name}{hb.state_text(props)}")
            time.sleep(3.0)
            for (name, props, want), pos in fila:
                got = json.loads(h.ctl("light", *map(str, pos), *map(str, pos)))["block"]
                if got == "." or int(got, 16) != want:
                    bad.append((name, props, want, got))

    medir(dark, [(40 + (i % 30) * 4, -60, 40 + (i // 30) * 4) for i in range(len(dark))])
    medir(lit, [(40 + (i % 8) * 16, -60, 200 + (i // 8) * 16) for i in range(len(lit))])
    print(f"luz: {len(dark)} estados sem luz e {len(lit)} com luz, {len(bad)} diferentes", bad)
    return not bad


def colocar():
    ok, total = True, 0
    stands = [("east", (82.5, -60, 80.5)), ("west", (77.5, -60, 80.5)), ("south", (80.5, -60, 82.5)), ("north", (80.5, -60, 77.5)),
              ("up", (82.5, -60, 82.5)), ("up", (77.5, -60, 82.5)), ("up", (77.5, -60, 77.5)), ("up", (82.5, -60, 77.5))]
    side = {"east": (81, -60, 80), "west": (79, -60, 80), "south": (80, -60, 81), "north": (80, -60, 79), "up": (80, -59, 80)}
    for name in ["solar_generator", "gas_burning_generator", "bio_generator", "advanced_solar_generator", "wind_generator"]:
        rule = json.load(open(os.path.join(CONTENT, f"data/{NS}/machinery_basis/placement/{name}.json")))
        h.command("clear @s")
        h.command(f"give @s {NS}:{name} 1")
        h.ctl("select", "1")
        for face, stand in stands:
            h.command("fill 70 -60 70 90 -52 90 minecraft:air")
            h.command("setblock 80 -60 80 minecraft:stone")
            h.ctl("tp", *map(str, stand))
            h.ctl("look-at", "80.5", "-59.5", "80.5")
            h.ctl("interact", "80", "-60", "80", face)
            time.sleep(0.3)
            pos = side[face]
            state = hb.block_state(pos)
            now = hb.status()
            want = hb.expected_facing(rule["facing"], now["yaw"], now["pitch"], face)
            got = state.split("facing=")[1].split(",")[0].split("]")[0] if "facing=" in state else None
            total += 1
            ok &= got == want
            print(f"{name} em {face} de {stand[0]},{stand[2]}: yaw {now['yaw']:.1f} pitch {now['pitch']:.1f} -> {got} (esperado {want})")
            h.command("clear @s")
            h.command(f"give @s {NS}:{name} 1")
            h.ctl("select", "1")
    for fluid, want in (("water", "water"), ("lava", "lava"), ("air", "empty")):
        h.command("fill 70 -60 70 90 -52 90 minecraft:air")
        h.command("setblock 80 -60 80 minecraft:stone")
        h.command(f"setblock 81 -60 80 minecraft:{fluid}")
        h.command("clear @s")
        h.command(f"give @s {NS}:bio_generator 1")
        h.ctl("select", "1")
        h.ctl("tp", "82.5", "-60", "80.5")
        h.ctl("look-at", "80.5", "-59.5", "80.5")
        h.ctl("interact", "80", "-60", "80", "east")
        time.sleep(0.3)
        state = hb.block_state((81, -60, 80))
        got = state.split("fluid_logged=")[1].split(",")[0].rstrip("]") if "fluid_logged=" in state else state
        total += 1
        ok &= got == want
        print(f"bio_generator com {fluid} no lugar: {state} (esperado fluid_logged={want})")
    h.command("fill 70 -60 70 90 -52 90 minecraft:air")
    print(f"colocar: {total} casos")
    return ok


def tags():
    """`execute if block` com cada tag de bloco da datagen: o bloco posto está nela. Lê o resultado do chat do comando."""
    datagen = json.load(open(os.path.join(ROOT, "dev/referencia/rastros/generators_datagen.json")))
    # O comando `execute if block #tag` só conhece as tags do vanilla (medido: `#mekanism:factories`, `#c:storage_blocks/osmium` e `#mekanismgenerators:structures/turbine` dão
    # "Unknown block tag" mesmo nas que o Mekanism já tinha); as tags do mod ficam para `tests/generators_content.rs`.
    cases = [("mekanismgenerators:reactor_glass", "minecraft:impermeable"), ("mekanismgenerators:wind_generator", "minecraft:mineable/pickaxe"),
             ("mekanismgenerators:laser_focus_matrix", "minecraft:dangerous_for_teleportation"), ("mekanismgenerators:control_rod_assembly", "minecraft:dangerous_for_teleportation"),
             ("mekanismgenerators:solar_generator", "minecraft:blocks_motion_no_leaves"), ("mekanismgenerators:bioethanol", "minecraft:replaceable")]
    negative = [("mekanismgenerators:turbine_rotor", "minecraft:impermeable"), ("mekanismgenerators:turbine_casing", "minecraft:dangerous_for_teleportation"), ("minecraft:stone", "minecraft:impermeable")]
    bad = []
    for block, tag in cases + negative:
        h.command(f"setblock 70 -60 70 {block}" if "bioethanol" not in block else "setblock 70 -60 70 mekanismgenerators:bioethanol")
        time.sleep(0.2)
        chat = " ".join(h.command(f"execute if block 70 -60 70 #{tag}"))
        yes = "passed" in chat.lower() or "success" in chat.lower() or "test passed" in chat.lower()
        want = (block, tag) in cases
        print(f"#{tag} <- {block}: {chat[:60]!r} -> {yes} (esperado {want})")
        if yes != want:
            bad.append((block, tag, chat))
    h.command("setblock 70 -60 70 minecraft:air")
    print(f"tags: {len(cases) + len(negative)} casos, {len(bad)} diferentes {bad}; datagen com {len(datagen['tags'])} tags")
    return not bad


def tag_items(tag):
    namespace, path = tag.split(":")
    for root in ("mods/mekanismgenerators/content", "mods/mekanism/content"):
        file = os.path.join(ROOT, root, f"data/{namespace}/tags/item/{path}.json")
        if os.path.isfile(file):
            out = []
            for value in json.load(open(file))["values"]:
                out += tag_items(value[1:]) if value.startswith("#") else [value]
            return out
    return []


def mesa(name):
    """Uma receita de mesa do Generators: põe cada ingrediente na grade como um jogador e confere o resultado."""
    recipe = json.load(open(os.path.join(CONTENT, f"data/{NS}/recipe/{name}.json")))
    key = {}
    for k, v in recipe["key"].items():
        if isinstance(v, list):
            v = v[0]
        if v.startswith("#"):
            vanilla = {"minecraft:planks": "minecraft:oak_planks", "minecraft:bars": "minecraft:iron_bars"}
            items = tag_items(v[1:])
            v = items[0] if items else vanilla.get(v[1:], "minecraft:iron_bars")
        key[k] = v
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
    h.ctl("screenshot", f"{SHOTS}/porte_mesa_{name.replace('/', '_')}.png")
    h.ctl("close")
    h.command("gamemode creative")
    h.command("clear @s")
    want = recipe["result"]["id"]
    print(f"mesa {name}:", result and (result["item"], result["count"]))
    return bool(result) and result["item"] == want and result["count"] == recipe["result"].get("count", 1)


# O gui_size de 640 por 360 (guiScale 3, janela de 1920 por 1080): o botão da próxima página e as abas do inventário criativo.
NEXT_PAGE, PREVIOUS_PAGE = (406, 72), (231, 72)


def shot(name):
    time.sleep(0.8)
    h.ctl("screenshot", f"{SHOTS}/{name}.png")


def scroll_to_end(times=4):
    for _ in range(times):
        h.ctl("scroll-at", "450", "190", "-60")
        time.sleep(0.3)


def tab(page, x, y):
    """A aba do Generators fica numa página das abas do criativo: `page` cliques na seta e o clique em (x, y); prints do começo e do fim da lista."""
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS}"], check=True)
    h.command("gamemode creative")
    h.ctl("close")
    h.ctl("key", "e", "tap")
    time.sleep(1.2)
    h.ctl("time", "6000")
    for _ in range(int(page)):
        h.ctl("click-at", *map(str, NEXT_PAGE))
        time.sleep(0.5)
    shot("porte_aba_pagina")
    h.ctl("click-at", x, y)
    time.sleep(0.6)
    shot("porte_aba_geradores_inicio")
    scroll_to_end(1)
    shot("porte_aba_geradores_meio")
    scroll_to_end(3)
    shot("porte_aba_geradores_fim")
    h.ctl("close")
    return True


def cena(out):
    spec = json.load(open(os.path.join(ROOT, "dev/referencia/cenas/generators.json")))
    subprocess.run(["ssh", "desktop", f"mkdir -p {out}"], check=True)
    h.command("tick unfreeze")
    # Só do port: o mundo novo nasce com mobs (queimam à luz do dia e entram no print); o original não tem nenhum.
    for cmd in ("gamerule spawn_mobs false", "gamerule advance_time false", "gamerule advance_weather false", "kill @e[type=!minecraft:player]"):
        h.command(cmd)
    for cmd in spec["setup"]:
        h.command(cmd)
    h.command("kill @e[type=!minecraft:player]")
    time.sleep(1.0)
    h.ctl("key", "f1", "tap")
    for s in spec["shots"]:
        h.command(spec["clear"])
        for cmd in s["cmds"]:
            h.command(cmd)
        h.command("tp @s " + s["tp"])
        time.sleep(2.5)
        h.ctl("screenshot", f"{out}/porte_{s['name']}.png")
        print("print", s["name"])
    h.ctl("key", "f1", "tap")


def main():
    mode = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else SHOTS
    simple = {"give": give, "estados": estados, "luz": luz, "colocar": colocar, "tags": tags}
    if mode == "mesa":
        ok = mesa(sys.argv[2])
    elif mode == "tab":
        ok = tab(*sys.argv[2:5])
    elif mode in simple:
        ok = simple[mode]()
    else:
        ok = cena(out)
    sys.exit(0 if ok in (True, None) else 1)


if __name__ == "__main__":
    main()
