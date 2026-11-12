#!/usr/bin/env python3
"""Em jogo (container `horizon`, H_NAME=horizon): os blocos de máquina e os blocos avulsos com tile do Mekanism, sem função.

Uso: H_NAME=horizon h2_blocos.py give|estados|cena [PASTA_DOS_PRINTS]
- give: todo id responde ao `give`;
- estados: (colunas em y -52) cada bloco, em até cinco estados do original (o primeiro, o último, `active=true`, cada `facing`), é posto por `setblock` e lido de volta por `block`;
- conexoes: os transmissores novos, postos lado a lado, ligam-se a quem é da família (a tag `connects_to`) e não a quem é de outra;
- cena [PASTA_DOS_PRINTS [TIRO ...]]: o roteiro `cenas/blocos.json`, o mesmo que `testes/CenaBlocos.java` roda no original, com os prints em PASTA_DOS_PRINTS (padrão `SHOTS`, na pasta de dados do jogo; `scp` os traz); com nomes de tiro, só esses.
Supõe o jogo aberto num mundo plano (o chão em y -61) com o jogador criativo, e a janela de 1920 por 1080.
"""
import json, os, re, subprocess, sys, time

import h2_identity as h

ROOT = os.path.abspath(os.path.join(h.TOOLS, "..", "..", ".."))
RASTRO = os.path.join(ROOT, "dev/referencia/rastros/blocos.jsonl")
# O `horizon-ctl screenshot` escreve dentro do container, onde só a pasta de dados do jogo é a mesma do desktop.
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots/blocos"
SCOPE = os.path.join(ROOT, "mods/mekanism/content/data/mekanism/horizon/block")


# O que o port tinha antes dos blocos de máquina (e os blocos de recurso de outra frente): fora desta conferência.
EARLIER = {"enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser", "steel_casing", "teleporter_frame"} | {
    f"{t}_universal_cable" for t in ("basic", "advanced", "elite", "ultimate")}


def connects(name):
    """A tag que o bloco declara em `connects_to` (os transmissores), ou None."""
    return json.load(open(os.path.join(SCOPE, f"{name}.json"))).get("connects_to")


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
        refused = "Unknown" in text or "No item" in text or "error" in text.lower()
        # O bloco de apoio não tem item: o `give` o recusa, como o original.
        if refused != (name == "bounding_block"):
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
    # As colunas vão de 56 a 113 em x e de 70 a 91 em z, em y -52: o jogador vai ao meio e espera os chunks.
    h.ctl("tp", "84", "-50", "80")
    h.ctl("wait-ready")
    bad, total = [], 0
    for i, (name, row) in enumerate(sorted(scope().items())):
        picks = [row["states"][0]["props"], row["states"][-1]["props"]]
        picks += [s["props"] for s in row["states"] if s["props"].get("active") == "true"][:1]
        picks += [s["props"] for s in row["states"] if s["props"].get("facing") in ("east", "west")][:2]
        pos = (56 + (i % 20) * 3, -52, 70 + (i // 20) * 3)
        for props in picks:
            text = state_text(props)
            h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} mekanism:{name}{text}")
            got = block_state(pos)
            want = f"mekanism:{name}{text}"
            total += 1
            # Um transmissor volta com os seis lados (`north` a `down`, a conexão do chorus plant do Horizon) além das propriedades do original (decisão 27): sem eles é o que se pôs.
            if connects(name) and "[" in got:
                kept = sorted(p for p in got[got.index("[") + 1 : -1].split(",") if p.split("=")[0] not in ("north", "east", "south", "west", "up", "down"))
                got = got[: got.index("[")] + ("[" + ",".join(kept) + "]" if kept else "")
            if got != want:
                bad.append((want, got))
    print(f"estados: {total} lidos, {len(bad)} diferentes", bad[:5])
    return not bad


def conexoes():
    """Os transmissores novos, postos por `setblock` lado a lado: dois da mesma família ligam-se (`east=true` no da esquerda e `west=true` no da direita), um de outra família não."""
    h.command("tick unfreeze")
    h.ctl("tp", "64", "-58", "98")
    h.ctl("wait-ready")
    ok = True
    pairs = [("basic_mechanical_pipe", "ultimate_mechanical_pipe", True), ("basic_pressurized_tube", "elite_pressurized_tube", True),
             ("advanced_thermodynamic_conductor", "basic_thermodynamic_conductor", True), ("basic_logistical_transporter", "restrictive_transporter", True),
             ("diversion_transporter", "ultimate_logistical_transporter", True), ("basic_mechanical_pipe", "basic_pressurized_tube", False),
             ("basic_universal_cable", "basic_mechanical_pipe", False), ("basic_thermodynamic_conductor", "basic_logistical_transporter", False)]
    for i, (left, right, joined) in enumerate(pairs):
        pos = (60 + 4 * i, -58, 100)
        h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} minecraft:air")
        h.command(f"setblock {pos[0] + 1} {pos[1]} {pos[2]} minecraft:air")
        h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} mekanism:{left}")
        h.command(f"setblock {pos[0] + 1} {pos[1]} {pos[2]} mekanism:{right}")
        a, b = block_state(pos), block_state((pos[0] + 1, pos[1], pos[2]))
        got = ("east=true" in a) and ("west=true" in b)
        print(f"{left} + {right}: {a} | {b} (esperado ligado={joined})")
        ok &= got == joined
    return ok


def tag_items(tag):
    """Os itens de uma tag de item do conteúdo, com as tags aninhadas resolvidas."""
    namespace, path = tag.split(":")
    file = os.path.join(ROOT, f"mods/mekanism/content/data/{namespace}/tags/item/{path}.json")
    out = []
    if not os.path.exists(file):
        # Uma tag aninhada do vanilla (`#minecraft:bars`): o conteúdo do mod não a tem, o jogo sim.
        return out
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
    # O chunk da mesa tem de estar carregado (depois de outro modo o jogador pode estar longe): vai para perto e espera.
    h.ctl("tp", "72", "-58", "58")
    h.ctl("wait-ready")
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
    for name in ["combiner", "personal_barrel", "laser", "logistical_sorter", "chemical_infuser", "quantum_entangloporter", "chargepad", "qio_importer",
                 "basic_bin", "ultimate_energy_cube", "creative_chemical_tank", "supercharged_coil", "thermal_evaporation_controller",
                 "basic_mechanical_pipe", "ultimate_universal_cable"]:
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
            if "facing" not in rule:
                # Um transmissor não tem `facing`: só se confere que o bloco foi posto.
                total += 1
                ok &= state.startswith(f"mekanism:{name}")
                h.command("clear @s")
                h.command(f"give @s mekanism:{name} 1")
                h.ctl("select", "1")
                continue
            want = expected_facing(rule["facing"], now["yaw"], now["pitch"], face)
            got = state.split("facing=")[1].split(",")[0].split("]")[0] if "facing=" in state else None
            total += 1
            if got != want:
                ok = False
            print(f"{name} em {face} de {stand[0]},{stand[2]}: yaw {now['yaw']:.1f} pitch {now['pitch']:.1f} -> {got} (esperado {want})")
            h.command("clear @s")
            h.command(f"give @s mekanism:{name} 1")
            h.ctl("select", "1")
    # O fluido do lugar: a água (fonte) enche o bloco, a lava também, o ar não. Numa máquina e num cano.
    for name in ("chemical_infuser", "basic_mechanical_pipe", "ultimate_universal_cable"):
        for fluid, want in (("water", "water"), ("lava", "lava"), ("air", "empty")):
            h.command("fill 70 -60 70 90 -52 90 minecraft:air")
            h.command("setblock 80 -60 80 minecraft:stone")
            h.command(f"setblock 81 -60 80 minecraft:{fluid}")
            h.command("clear @s")
            h.command(f"give @s mekanism:{name} 1")
            h.ctl("select", "1")
            h.ctl("tp", "82.5", "-60", "80.5")
            h.ctl("look-at", "80.5", "-59.5", "80.5")
            h.ctl("interact", "80", "-60", "80", "east")
            time.sleep(0.3)
            state = block_state((81, -60, 80))
            got = state.split("fluid_logged=")[1].split(",")[0].rstrip("]") if "fluid_logged=" in state else state
            total += 1
            ok &= got == want
            print(f"{name} com {fluid} no lugar: {state} (esperado fluid_logged={want})")
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
        """Em levas de uma fileira da grade: o jogador vai para o meio dela (fora da distância de carga o `setblock` não pega e a luz lê `.`), os blocos se põem, e a luz se lê depois."""
        itens = list(zip(casos, posicoes))
        fileiras = {}
        for item in itens:
            fileiras.setdefault(item[1][2], []).append(item)
        for z, fila in sorted(fileiras.items()):
            meio = sum(pos[0] for _, pos in fila) // len(fila)
            h.ctl("tp", str(meio), "-58", str(z))
            h.ctl("wait-ready")
            time.sleep(1.5)
            for (name, props, _), pos in fila:
                h.command(f"setblock {pos[0]} {pos[1]} {pos[2]} mekanism:{name}{state_text(props)}")
            time.sleep(3.0)
            for (name, props, want), pos in fila:
                got = json.loads(h.ctl("light", *map(str, pos), *map(str, pos)))["block"]
                if got == "." or int(got, 16) != want:
                    bad.append((name, props, want, got))

    # A leva sem luz: uma grade de 4 em 4 blocos, 30 por fileira; a com luz, longe dela, de 16 em 16, 8 por fileira.
    medir(dark, [(40 + (i % 30) * 4, -60, 40 + (i // 30) * 4) for i in range(len(dark))])
    medir(lit, [(40 + (i % 8) * 16, -60, 200 + (i // 8) * 16) for i in range(len(lit))])
    print(f"luz: {len(dark)} estados sem luz e {len(lit)} com luz, {len(bad)} diferentes", bad)
    return not bad


CAPACIDADE_DO_CUBO = {"basic": 4_000_000, "advanced": 16_000_000, "elite": 64_000_000, "ultimate": 256_000_000}
LADOS_DO_CUBO = ("front", "left", "right", "back", "top", "bottom")
TIPO_DO_LADO = {"none": 0, "input": 1, "output": 4}


def comando_do_cubo(texto, cmds):
    """`#cubo X Y Z FRACAO lado=tipo...` (`testes/CenaBlocos.java`) no port: `data merge block` com a energia e o `component_config` de energia (tipo 0, os seis lados na
    ordem de `RelativeSide`). O tier sai do `setblock` do mesmo tiro; o criativo guarda o valor exato em `energy_amount` (decisão 67)."""
    _, x, y, z, fracao, *lados = texto.split()
    tier = next(c.split("mekanism:")[1].split("_energy_cube")[0] for c in cmds if c.startswith(f"setblock {x} {y} {z} "))
    nbt = []
    if tier == "creative":
        nbt.append("energy_amount:%dL" % int(float(fracao) * 9223372036854775807))
    else:
        nbt.append("energy:%d" % int(float(fracao) * CAPACIDADE_DO_CUBO[tier]))
    if lados:
        tipos = dict(l.split("=") for l in lados)
        nbt.append("component_config:{config0:[I;%s]}" % ",".join(str(TIPO_DO_LADO[tipos[l]]) for l in LADOS_DO_CUBO))
    return f"data merge block {x} {y} {z} {{{','.join(nbt)}}}"


PERIODO_DO_NUCLEO = 360  # o núcleo do cubo gira 4 graus por tick e sobe e desce com 3 graus por tick: o mínimo múltiplo de 90 e 120 ticks


def alinha_o_tempo(nome):
    """O núcleo e o item do cubo desenham pelo tempo do jogo (`gameTime`), e o do original no print de `nome` vem do log (`GAMETIME_ORIGINAL`, um JSON `{tiro: tempo}`):
    com o servidor parado, `tick step` leva o tempo do port à mesma fase do período do núcleo."""
    arquivo = os.environ.get("GAMETIME_ORIGINAL")
    if not arquivo:
        return
    alvo = json.load(open(arquivo)).get(nome)
    if alvo is None:
        return
    atual = int(re.search(r"\d+", " ".join(h.command("time query gametime"))).group())
    passos = (alvo - atual) % PERIODO_DO_NUCLEO
    if passos:
        h.command(f"tick step {passos}")
        time.sleep(passos / 20.0 + 1.0)
    print("tempo", nome, "alvo", alvo, "port", atual, "passos", passos)


def cena(out, nomes=(), roteiro="blocos.json"):
    if os.environ.get("IGUALAR_OPCOES"):
        # As opções do cliente do original (`options.txt` da run do `mekanism-ref`): o campo de visão, o brilho, a oclusão ambiente. Sem elas o mundo sai com outra vista.
        sys.path.insert(0, os.path.join(h.TOOLS, ".."))
        import cena_telas
        cena_telas.iguala_as_opcoes()
    spec = json.load(open(os.path.join(ROOT, "dev/referencia/cenas", roteiro)))
    if nomes:
        spec["shots"] = [shot for shot in spec["shots"] if shot["name"] in nomes]
    subprocess.run(["ssh", "desktop", f"mkdir -p {out}"], check=True)
    h.command("tick unfreeze")
    # Só do port: o mundo novo nasce com mobs (queimam à luz do dia e entram no print); o original não tem nenhum.
    for cmd in ("gamerule spawn_mobs false", "gamerule advance_time false", "gamerule advance_weather false", "kill @e[type=!minecraft:player]"):
        h.command(cmd)
    for cmd in spec["setup"]:
        h.command(cmd)
    h.command("kill @e[type=!minecraft:player]")
    time.sleep(1.0)
    # `"hud": true` no roteiro deixa a tela ligada (a mão e a barra de atalhos), como o `CenaBlocos`.
    if not spec.get("hud"):
        h.ctl("key", "f1", "tap")
    for shot in spec["shots"]:
        h.command(spec["clear"])
        for cmd in shot["cmds"]:
            if cmd.startswith("#cubo "):
                # O bloco precisa ter nascido para receber o NBT.
                time.sleep(1.5)
                cmd = comando_do_cubo(cmd, shot["cmds"])
            if cmd.startswith("#esperar "):
                time.sleep(int(cmd.split()[1]) / 20.0)
                continue
            h.command(cmd)
        alinha_o_tempo(shot["name"])
        h.command("tp @s " + shot["tp"])
        time.sleep(2.5)
        h.ctl("screenshot", f"{out}/porte_{shot['name']}.png")
        print("print", shot["name"])
    if not spec.get("hud"):
        h.ctl("key", "f1", "tap")
    h.command("tick unfreeze")


def main():
    mode = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else SHOTS
    ok = {"give": give, "estados": estados, "colocar": colocar, "luz": luz, "virados": virados, "conexoes": conexoes, "mesa": lambda: mesa(sys.argv[2])}.get(mode, lambda: cena(out, sys.argv[3:], os.environ.get("ROTEIRO", "blocos.json")))()
    sys.exit(0 if ok in (True, None) else 1)


if __name__ == "__main__":
    main()
