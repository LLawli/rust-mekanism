#!/usr/bin/env python3
"""Os vizinhos que empurram, em jogo (frente `tanques`, 9.ª rodada): os cenários dos rastros `fluid_tank_*` e `chemical_tank_*` repetidos por comandos, no port e no
cliente do original, com o NBT de cada ator lido a cada tick, mais o item do tanque destruído pelo nível e por uma explosão, a cena da lava com o Fluid Tank no lugar
do caldeirão e a cena do vídeo (um tanque enchendo outro por canos).

    h2_tanques9.py roteiro > ../cenas/tanques9.json      o roteiro do `CenaBlocos` do original (os mesmos comandos, com o NBT do original)
    H_NAME=horizon h2_tanques9.py port PARTE...          no notebook: `setup`, `video`, `rastros`, um cenário (`fluid:tank_push_pipe`), `drops`, `lava`, `limpar`
    h2_tanques9.py original LOG                          confere o log do cliente do original (`oficial.log`) contra os rastros

Cada tick: as marcas `pre` do rastro viram comandos, `tick step 1`, e um `data get block` por ator. O estado ativo entra por `active_state` no NBT (o caminho de
`loadAdditional`, igual nos dois jogos: o tanque empurra com o vidro apagado), os lados de um cano pelo Configurator (`#conexao`), e as marcas de modo no lado de cima
de um cano com um tanque em cima não têm clique possível: o cenário para ali. O que se compara é o conteúdo de cada ator a cada tick.
"""
import json, os, re, sys, time

AQUI = os.path.dirname(os.path.abspath(__file__))
RASTROS = os.path.join(AQUI, "..", "rastros")
# A origem da linha em cada jogo: o chão do original é plano em y -60; o mundo `telas` do port ganha uma plataforma em y 140.
BASE = {"original": (58, -58, 82), "port": (58, 142, 82)}
CENARIOS = [
    ("fluid", "tank_push_pipe"), ("fluid", "tank_push_alone"), ("fluid", "tank_push_generator"), ("fluid", "tank_valve_top"), ("fluid", "tank_incompatible"),
    ("chemical", "tank_eject_tube"), ("chemical", "tank_eject_alone"), ("chemical", "tank_eject_machine"), ("chemical", "tank_incompatible"),
]
CAPACITY = {"fluid": {"basic": 32000, "advanced": 64000, "elite": 128000, "ultimate": 256000}, "chemical": {"basic": 64000, "advanced": 256000, "elite": 1024000, "ultimate": 8192000}}
MAX = 9223372036854775807
DUMP = ["idle", "dumping_excess", "dumping"]
CLEAR = "fill {} {} {} {} {} {} minecraft:air"


def rows(kind, scenario):
    return [json.loads(line) for line in open(os.path.join(RASTROS, f"{kind}_{scenario}.jsonl"))]


def contents(game, kind, resource, amount):
    """O NBT do conteúdo de um tanque."""
    if kind == "chemical":
        return "" if amount == 0 else 'chemical_tanks:[{container:0b,stored:{type:"%s",amount:%dL}}]' % (resource, amount)
    if game == "port":
        return "machinery_basis:{v:1,tanks:[{amount:0L}]}" if amount == 0 else 'machinery_basis:{v:1,tanks:[{resource:"%s",amount:%dL}]}' % (resource, amount)
    return "" if amount == 0 else 'fluid_tanks:[{container:0,stored:{amount:%dL,type:{id:"%s"}}}]' % (amount, resource)


def ops(game, kind, scenario):
    """As operações do cenário: `("cmd", texto)`, `("conexao", "X Y Z", lado, tipo)`, `("wait", ticks)`, `("step",)`, `("mark", tick)` e `("read", nome, "X Y Z", tipo)`.
    Devolve também o último tick comparado."""
    bx, by, bz = BASE[game]
    at = lambda x, y, z: "%d %d %d" % (bx + int(x), by + int(y), bz + int(z))
    pipe = "mekanism:%s_mechanical_pipe" if kind == "fluid" else "mekanism:%s_pressurized_tube"
    tank = "mekanism:%s_fluid_tank" if kind == "fluid" else "mekanism:%s_chemical_tank"
    # O Chemical Tank do rastro: os seis lados `INPUT_OUTPUT`, sem ejetar. Vai no `setblock`, para a capability existir desde que o bloco nasce.
    sides = "component_config:{config2:[I;7,7,7,7,7,7],eject2:0b}" if kind == "chemical" else ""
    marks = {}
    for row in rows(kind, scenario):
        if row["e"] == "mark":
            marks.setdefault(row["t"], []).append(row["m"])
    last = max(row["t"] for row in rows(kind, scenario) if row["e"] == "estate")
    out, actors, dumping, stop, placed = [], {}, {}, None, {}
    # Uma fonte que o rastro põe no mesmo tick em que o cano passa a puxar dela: o braço do cano só existe depois de um tick do cano, e o Configurator do port
    # mira o braço. Nos dois jogos a fonte nasce vazia na montagem, o lado vai a `PULL` ali, e no tick do rastro ela ganha o conteúdo.
    early = {}
    for t, group in marks.items():
        sources = [m.removeprefix("pre ").split(" ") for m in group if m.startswith("pre source ")]
        pulls = [m for m in group if m.startswith("pre pull ")]
        if t > 0 and sources and pulls:
            early[t] = (sources, pulls)

    def place_tank(words, creative=False):
        if creative:
            name, x, y, z, tier, resource, amount = words[1], words[2], words[3], words[4], "creative", words[5], MAX
        else:
            name, x, y, z, tier, resource, amount = words[1], words[2], words[3], words[4], words[5], words[6], int(words[7])
            amount = 0 if resource == "empty" else amount
        nbt = ",".join(part for part in (contents(game, kind, resource, amount) if amount else "", sides) if part)
        out.append(("cmd", "setblock %s %s%s" % (at(x, y, z), tank % tier, "{%s}" % nbt if nbt else "")))
        placed[name] = "setblock %s %s%s" % (at(x, y, z), tank % tier, "{%s}" % sides if sides else "")
        actors[name] = (at(x, y, z), "tank", tier)

    def apply(mark):
        nonlocal stop
        words = mark.removeprefix("pre ").split(" ")
        head = words[0]
        if head == "place":
            out.append(("cmd", "setblock %s %s" % (at(words[1], 0, words[2]), pipe % words[3])))
        elif head == "break":
            out.append(("cmd", "setblock %s minecraft:air" % at(words[1], 0, words[2])))
        elif head == "rtank":
            place_tank(words)
        elif head == "source":
            place_tank(words, creative=True)
        elif head == "generator":
            out.append(("cmd", "setblock %s mekanismgenerators:heat_generator[facing=north]" % at(words[2], words[3], words[4])))
            actors[words[1]] = (at(words[2], words[3], words[4]), "generator", "")
        elif head == "machine":
            # O Metallurgic Infuser com o químico em `INPUT` nos seis lados.
            placed[words[1]] = "setblock %s mekanism:metallurgic_infuser[facing=north]{component_config:{config2:[I;1,1,1,1,1,1]}}" % at(words[2], words[3], words[4])
            out.append(("cmd", placed[words[1]]))
            actors[words[1]] = (at(words[2], words[3], words[4]), "machine", "")
        elif head == "active":
            out.append(("cmd", "data merge block %s {active_state:%db}" % (actors[words[1]][0], words[2] == "true")))
        elif head == "eject":
            out.append(("cmd", "data merge block %s {component_config:{eject2:%db}}" % (actors[words[1]][0], words[2] == "true")))
        elif head == "dump":
            dumping[words[1]] = (dumping.get(words[1], 0) + 1) % 3
            out.append(("cmd", 'data merge block %s {dumping:"%s"}' % (actors[words[1]][0], DUMP[dumping[words[1]]])))
        elif head in ("setfluid", "setchem"):
            pos, what, tier = actors[words[1]]
            if words[2] == "empty" or words[3] == "0":
                # Esvaziar: nenhum comando de dados esvazia o tanque do original (o `data remove` da chave deixa o contêiner como estava, medido), então o bloco
                # é trocado por um novo, nos dois jogos.
                out.append(("cmd", "setblock %s minecraft:air" % pos))
                out.append(("cmd", placed[words[1]]))
            else:
                amount = CAPACITY[kind][tier] if words[3] == "max" else int(words[3])
                out.append(("cmd", "data merge block %s {%s}" % (pos, contents(game, kind, words[2], amount))))
        elif head in ("pull", "mode"):
            side, mode = words[3], ("pull" if head == "pull" else words[4])
            if side in ("up", "down"):
                stop = True
            else:
                out.append(("conexao", at(words[1], 0, words[2]), side, mode))
        elif head != "finish":
            raise SystemExit("marca sem comando: " + mark)

    for mark in marks.get(0, []):
        if not mark.startswith("pre "):
            apply(mark)
    for t, (sources, pulls) in early.items():
        for words in sources:
            out.append(("cmd", "setblock %s %s" % (at(words[2], words[3], words[4]), tank % "creative")))
            actors[words[1]] = (at(words[2], words[3], words[4]), "tank", "creative")
        out.append(("wait", 30))
        for mark in pulls:
            apply(mark)
    # A cena assenta com o servidor andando (os canos formam a rede) e só então para.
    out += [("wait", 40), ("cmd", "tick freeze"), ("wait", 10)]
    end = 0
    for t in range(0, last + 1):
        for mark in marks.get(t, []):
            if t in early and mark.startswith("pre pull "):
                continue
            if t in early and mark.startswith("pre source "):
                words = mark.removeprefix("pre ").split(" ")
                out.append(("cmd", "data merge block %s {%s}" % (actors[words[1]][0], contents(game, kind, words[5], MAX))))
                continue
            if mark.startswith("pre "):
                apply(mark)
        if stop:
            break
        out.append(("step",))
        out.append(("mark", t))
        for name, (pos, what, _) in actors.items():
            out.append(("read", name, pos, what))
        end = t
    out.append(("cmd", "tick unfreeze"))
    return out, end, actors


def absorver_ops(game, kind):
    """`absorver`: sem rastro, o port contra o cliente do original. Um tanque vazio ao sul de um cano (ou tubo) em `PULL`, com outro tanque do outro lado do cano;
    com o servidor parado, dois ticks, o conteúdo do primeiro tanque posto por `data merge`, e seis ticks: no original o cano já puxa do conteúdo novo no primeiro tick
    depois do comando (`loadAdditional` troca o contêiner na hora)."""
    bx, by, bz = BASE[game]
    at = lambda x, y, z: "%d %d %d" % (bx + x, by + y, bz + z)
    pipe = "mekanism:basic_mechanical_pipe" if kind == "fluid" else "mekanism:basic_pressurized_tube"
    tank = "mekanism:basic_fluid_tank" if kind == "fluid" else "mekanism:basic_chemical_tank{component_config:{config2:[I;7,7,7,7,7,7],eject2:0b}}"
    resource = "minecraft:water" if kind == "fluid" else "mekanism:hydrogen"
    actors = {"S": (at(0, 0, 1), "tank", "basic"), "D": (at(1, 0, 0), "tank", "basic")}
    out = [("cmd", "setblock %s %s" % (at(0, 0, 1), tank)), ("cmd", "setblock %s %s" % (at(0, 0, 0), pipe)), ("cmd", "setblock %s %s" % (at(1, 0, 0), tank)),
           ("wait", 30), ("conexao", at(0, 0, 0), "south", "pull"), ("wait", 20), ("cmd", "tick freeze"), ("wait", 10)]
    for t in range(8):
        if t == 2:
            out.append(("cmd", "data merge block %s {%s}" % (at(0, 0, 1), contents(game, kind, resource, 5000))))
        out += [("step",), ("mark", t)] + [("read", name, pos, what) for name, (pos, what, _) in actors.items()]
    out.append(("cmd", "tick unfreeze"))
    return out, 7, actors


def scenario_ops(game, kind, scenario):
    return absorver_ops(game, kind) if scenario == "absorver" else ops(game, kind, scenario)


# Os cenários sem rastro (só o port contra o cliente do original).
SEM_RASTRO = [("fluid", "absorver"), ("chemical", "absorver")]


def save(game, readings):
    """As leituras de um jogo, para `comparar`: `{"tipo:cenário": {tick: {ator: [recurso, quantidade]}}}`."""
    path = os.environ.get("LEITURAS", "leituras_%s.json" % game)
    old = json.load(open(path)) if os.path.exists(path) else {}
    old.update({key: {str(t): {name: list(got) for name, got in tick.items()} for t, tick in value.items()} for key, value in readings.items()})
    json.dump(old, open(path, "w"), indent=0, sort_keys=True)


def comparar(a, b):
    """As leituras de dois jogos (o port e o cliente do original, os mesmos comandos), tick a tick."""
    one, two = json.load(open(a)), json.load(open(b))
    total = 0
    for key in sorted(set(one) | set(two)):
        if key not in one or key not in two:
            print(f"{key}: só num dos dois")
            total += 1
            continue
        diffs, states = [], 0
        for t in sorted(set(one[key]) | set(two[key]), key=int):
            for name in sorted(set(one[key].get(t, {})) | set(two[key].get(t, {}))):
                states += 1
                x, y = one[key].get(t, {}).get(name), two[key].get(t, {}).get(name)
                if x != y:
                    diffs.append((t, name, x, y))
        print(f"{key}: {states} leituras, {len(diffs)} diferenças entre os dois jogos")
        for diff in diffs[:8]:
            print("   t={} {}: {} contra {}".format(*diff))
        total += len(diffs)
    return total


def expected(kind, scenario):
    """`{tick: {ator: (recurso, quantidade)}}` do rastro."""
    key = "fluid" if kind == "fluid" else "chem"
    out = {}
    for row in rows(kind, scenario):
        if row["e"] == "estate":
            out[row["t"]] = {actor["name"]: (actor[key] if actor["amount"] else "", actor["amount"]) for actor in row["actors"]}
    return out


def stored(data, kind, what, game):
    """O recurso e a quantidade de um ator no texto do `data get block`."""
    if kind == "fluid" and game == "port":
        found = re.search(r"tanks: \[\{([^}]*)\}", data)
        if not found:
            return ("", 0)
        amount = re.search(r"amount: (\d+)L", found.group(1))
        resource = re.search(r'resource: "([^"]+)"', found.group(1))
        amount = int(amount.group(1)) if amount else 0
        return (resource.group(1) if resource and amount else "", amount)
    found = re.search(r"%s: \[\{(.*?)\}\]" % ("fluid_tanks" if kind == "fluid" else "chemical_tanks"), data)
    if not found:
        return ("", 0)
    amount = int(re.search(r"amount: (\d+)L", found.group(1)).group(1))
    resource = re.search(r'id: "([^"]+)"|type: "([^"]+)"', found.group(1))
    return ((resource.group(1) or resource.group(2)) if amount else "", amount)


def compare(label, kind, scenario, readings, end):
    want = expected(kind, scenario)
    diffs, states = [], 0
    for t in range(0, end + 1):
        for name, got in readings.get(t, {}).items():
            if name not in want[t]:
                # Um ator que o rastro só põe mais tarde (a fonte que aqui nasce vazia na montagem).
                continue
            states += 1
            if want[t].get(name) != got:
                diffs.append((t, name, got, want[t].get(name)))
    print(f"{label} {kind}:{scenario}: ticks 0 a {end}, {states} leituras, {len(diffs)} diferenças", flush=True)
    for diff in diffs[:10]:
        print("   t={} {}: lido {} rastro {}".format(*diff), flush=True)
    return len(diffs)


# ---------------------------------------------------------------------------------------------------------------------------------------
# A cena do vídeo, os itens soltos e a lava: os mesmos comandos nos dois jogos.


def video_cmds(game):
    """Um Fluid Tank básico cheio de água sobre o primeiro de três canos, com um tanque básico vazio no fim. Com o servidor parado e um `tick step` de cada vez:
    10 ticks parado, o clique agachado do Configurator no tanque de cima (liga: o som do clique, o vidro aceso, a água passa a 500 mB por tick), 30 ticks, outro
    clique (desliga: a água para), 15 ticks, outro clique e a água até o fim."""
    bx, by, bz = BASE[game]
    at = lambda x, y=0, z=0: "%d %d %d" % (bx + x, by + y, bz + z)
    antes = [
        "tick unfreeze",
        CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2),
        "kill @e[type=item]",
        "setblock %s mekanism:basic_mechanical_pipe" % at(0),
        "setblock %s mekanism:basic_mechanical_pipe" % at(1),
        "setblock %s mekanism:basic_mechanical_pipe" % at(2),
        "setblock %s mekanism:basic_fluid_tank" % at(3),
        "setblock %s mekanism:basic_fluid_tank{%s}" % (at(0, 1), contents(game, "fluid", "minecraft:water", 32000)),
        "#esperar 40",
        "tick freeze",
        "#esperar 10",
    ]
    passos = lambda n: ["tick step 1", "#esperar 4"] * n
    clique = ["#agachado %s" % at(0, 1), "#esperar 10"]
    fluxo = passos(10) + clique + passos(30) + clique + passos(15) + clique + passos(34) + ["#esperar 20"]
    return [("t9_antes", antes, VIDEO_TP[game]), ("t9_depois", fluxo, VIDEO_TP[game])]


# A câmera do vídeo e dos prints: de frente para a linha, perto o bastante para o clique do jogador alcançar o tanque de cima.
VIDEO_TP = {game: "%.1f %.1f %.1f 0 -8" % (base[0] + 2.0, base[1] - 1.0, base[2] - 3.4) for game, base in BASE.items()}


def slots(game, *items):
    """Os slots do tile no NBT de cada jogo: `items` com `container` e `stored` no original, `Items` com `Slot` no port."""
    if game == "port":
        return "Items:[%s]" % ",".join('{Slot:%db,id:"%s",count:%d}' % (i, item, count) for i, (item, count) in enumerate(items))
    return "items:[%s]" % ",".join('{container:%d,stored:{amount:%d,type:{id:"%s"}}}' % (i, count, item) for i, (item, count) in enumerate(items))


DROPS = [
    ("fluid_full", "mekanism:basic_fluid_tank", lambda game: contents(game, "fluid", "minecraft:lava", 5000) + "," + slots(game, ("minecraft:lava_bucket", 1), ("minecraft:bucket", 3)) + ',edit_mode:"fill"'),
    ("chemical_full", "mekanism:advanced_chemical_tank", lambda game: contents(game, "chemical", "mekanism:hydrogen", 5000) + "," + slots(game, ("minecraft:stick", 2)) + ',dumping:"dumping_excess",component_config:{config2:[I;4,0,0,0,0,0],eject2:1b}'),
]
# A explosão no centro do bloco de cima (o `PrimedTnt` explode em `getY(0.0625)`), como o `TankDropTraceTest`.
HOW = {"destroy": "setblock {pos} minecraft:air destroy", "explode": "summon minecraft:tnt {x:.1f} {y:.2f} {z:.1f} {{fuse:0s,explosion_power:6f}}", "tnt": "summon minecraft:tnt {x:.1f} {y:.2f} {z:.1f} {{fuse:0s,explosion_power:4f}}"}


def drop_cmds(game):
    """O tanque cheio (o conteúdo, os slots e o modo) destruído por `setblock ... destroy`, por uma explosão de potência 6 no bloco de cima e pela da TNT (potência
    4, que não o quebra): depois de cada um, o bloco que ficou e o item do chão."""
    bx, by, bz = BASE[game]
    x, y, z = bx + 4, by - 1, bz
    cmds = ["tick unfreeze", CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2)]
    for case, block, nbt in DROPS:
        for how, text in HOW.items():
            cmds += [
                CLEAR.format(x - 2, y, z - 2, x + 2, y + 3, z + 2),
                "fill %d %d %d %d %d %d minecraft:obsidian" % (x - 3, y - 1, z - 3, x + 3, y - 1, z + 3),
                "kill @e[type=item]",
                "setblock %d %d %d %s{%s}" % (x, y, z, block + ("[facing=north]" if "chemical" in block else ""), nbt(game)),
                "#esperar 20",
                "say T9 drop %s_%s" % (case, how),
                text.format(pos="%d %d %d" % (x, y, z), x=x + 0.5, y=y + 1.44, z=z + 0.5),
                "#esperar 20",
                "execute if block %d %d %d %s" % (x, y, z, block),
                "execute positioned %d %d %d run data get entity @e[type=item,distance=..8,limit=1,sort=nearest] Item" % (x, y, z),
                "execute positioned %d %d %d if entity @e[type=item,distance=..8]" % (x, y, z),
            ]
    cmds += ["kill @e[type=item]", "fill %d %d %d %d %d %d minecraft:smooth_stone" % (x - 3, y - 1, z - 3, x + 3, y - 1, z + 3), CLEAR.format(x - 2, y, z - 2, x + 2, y + 3, z + 2)]
    return cmds


def lava_cmds(game):
    """A cena da lava da decisão 138 com o Fluid Tank de verdade no lugar do caldeirão: um tanque criativo de lava a oeste do primeiro de quatro canos, em `PULL`, e o
    Heat Generator a leste do último. Depois de 120 ticks, o tanque do gerador e o estado ativo dele."""
    bx, by, bz = BASE[game]
    at = lambda x, y=0, z=0: "%d %d %d" % (bx + x, by + y, bz + z)
    cmds = ["tick unfreeze", CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2),
            "setblock %s mekanism:creative_fluid_tank{%s}" % (at(0), contents(game, "fluid", "minecraft:lava", MAX)),
            "setblock %s mekanismgenerators:heat_generator[facing=north]" % at(5), "#esperar 10"]
    cmds += ["setblock %s mekanism:basic_mechanical_pipe" % at(x) for x in range(1, 5)]
    cmds += ["#esperar 40", "#conexao %s west=pull" % at(1), "#esperar 120", "say T9 lava", "data get block %s" % at(5), "data get block %s" % at(0)]
    return cmds


def roteiro():
    """O roteiro do `CenaBlocos` do original: o vídeo, os cenários, os itens soltos e a lava. O `clear` do roteiro não limpa nada: cada tiro limpa a sua área."""
    game = "original"
    bx, by, bz = BASE[game]
    area = CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2)
    tp = "%.1f %.1f %.1f 0 20" % (bx + 2.5, by + 1.6, bz - 4.4)
    shots = [{"name": name, "cmds": cmds, "tp": tp} for name, cmds, tp in video_cmds(game)]
    for kind, scenario in CENARIOS + SEM_RASTRO:
        cmds = ["tick unfreeze", area, "kill @e[type=item]", "#esperar 5"]
        steps, _, _ = scenario_ops(game, kind, scenario)
        for op in steps:
            if op[0] == "cmd":
                cmds.append(op[1])
            elif op[0] == "conexao":
                cmds.append("#conexao %s %s=%s" % (op[1], op[2], op[3]))
            elif op[0] == "wait":
                cmds.append("#esperar %d" % op[1])
            elif op[0] == "step":
                cmds += ["tick step 1", "#esperar 4"]
            elif op[0] == "mark":
                cmds.append("say T9 %s:%s %d" % (kind, scenario, op[1]))
            elif op[0] == "read":
                cmds.append("data get block %s" % op[2])
        shots.append({"name": "t9_%s_%s" % (kind, scenario), "cmds": cmds, "tp": tp})
    shots.append({"name": "t9_drops", "cmds": drop_cmds(game), "tp": tp})
    shots.append({"name": "t9_lava", "cmds": lava_cmds(game), "tp": tp})
    shots.append({"name": "t9_limpeza", "cmds": ["tick unfreeze", area, "kill @e[type=item]"], "tp": tp})
    # O jogador entra longe (x 300): sem ele perto, a área da cena não está carregada e todo `setblock` responde `That position is not loaded`.
    setup = ["tp @a " + VIDEO_TP[game], "difficulty peaceful", "time set 6000", "weather clear", "gamerule advance_time false", "gamerule advance_weather false", "gamemode creative @a",
             "fill 45 -60 70 77 -60 110 minecraft:smooth_stone", "fill 52 -59 76 72 -54 88 minecraft:air"]
    return {"setup": setup, "clear": "weather clear", "hud": False, "shots": shots}


def original(log):
    """Lê o log do cliente do original: as falas `T9 ...` marcam o tick, e cada `has the following block data` depois delas é um ator."""
    game = "original"
    by_pos, readings, current, drops, lava = {}, {}, None, {}, []
    for kind, scenario in CENARIOS + SEM_RASTRO:
        _, _, actors = scenario_ops(game, kind, scenario)
        for name, (pos, what, _) in actors.items():
            by_pos[(kind, scenario, pos.replace(" ", ", "))] = (name, what)
    for line in open(log, errors="replace"):
        said = re.search(r"T9 (\S+) ?(\S+)?\s*$", line.rstrip("\n")) if "T9 " in line else None
        if said:
            tick = said.group(2)
            current = (said.group(1), int(tick) if tick and tick.lstrip("-").isdigit() else tick)
            continue
        if current is None:
            continue
        if current[0] == "drop":
            found = re.search(r"System chat: (Test (?:passed|failed).*|.* has the following entity data: .*|No entity was found)", line)
            if found and "Server thread" in line:
                print("original drop", current[1] if current[1] is not None else "", "|", found.group(1)[:700])
        data = re.search(r"(-?\d+, -?\d+, -?\d+) has the following block data: (.*)", line)
        if data and ":" in current[0] and isinstance(current[1], int):
            kind, scenario = current[0].split(":")
            who = by_pos.get((kind, scenario, data.group(1)))
            if who:
                readings.setdefault((kind, scenario), {}).setdefault(current[1], {})[who[0]] = stored(data.group(2), kind, who[1], game)
        elif data and current[0] == "lava":
            lava.append(data.group(2)[:400])
    total = 0
    for kind, scenario in CENARIOS:
        _, end, _ = ops(game, kind, scenario)
        got = readings.get((kind, scenario), {})
        if not got:
            print(f"original {kind}:{scenario}: sem leituras no log")
            total += 1
            continue
        total += compare("original", kind, scenario, got, end)
    for text in lava:
        print("original lava:", text)
    save(game, {"%s:%s" % key: value for key, value in readings.items()})
    return total


# ---------------------------------------------------------------------------------------------------------------------------------------
# O port.


def port(parts):
    os.environ.setdefault("H_NAME", "horizon")
    sys.path.insert(0, AQUI)
    import h2_identity as h
    game = "port"
    bx, by, bz = BASE[game]
    area = CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2)

    def c(text):
        return " | ".join(h.command(text))

    def run(cmds, falar=None):
        """Os comandos de um tiro do roteiro, com as diretivas dele."""
        said = None
        for cmd in cmds:
            if cmd.startswith("#esperar "):
                # Uma espera de poucos ticks é a do passo do servidor no original: aqui o ir e vir de cada comando (por ssh) já a cobre.
                if int(cmd.split()[1]) > 4:
                    time.sleep(int(cmd.split()[1]) / 20.0)
            elif cmd.startswith("#conexao "):
                import h2_blocos
                h.ctl("give", "mekanism:configurator", "1")
                h2_blocos.conexao(cmd)
            elif cmd.startswith("#agachado "):
                # O clique direito agachado do Configurator no meio da face norte do tanque, como jogador (a mira do `look-at` e o botão do mouse).
                import h2_ajustes_jogo as aj
                aj.click_at(tuple(int(v) for v in cmd.split()[1:4]), (0.5, 0.5, 0.0))
                c("tp @s " + VIDEO_TP[game])
            elif cmd.startswith("say T9 "):
                said = cmd[7:]
            else:
                out = c(cmd)
                if falar and (cmd.startswith(("data get", "execute")) or cmd.startswith("setblock") and "hanged the block" not in out):
                    falar(said, cmd, out)

    total = 0
    for part in parts:
        if part == "setup":
            for cmd in ("gamerule advance_time false", "gamerule advance_weather false", "time set noon", "weather clear", "gamerule spawn_mobs false", "gamemode creative @s",
                        "fill %d %d %d %d %d %d minecraft:smooth_stone" % (bx - 13, by - 2, bz - 12, bx + 19, by - 2, bz + 12),
                        "fill %d %d %d %d %d %d minecraft:air" % (bx - 6, by - 1, bz - 6, bx + 14, by + 4, bz + 6),
                        "tp @s %.1f %.1f %.1f 0 20" % (bx + 2.5, by, bz - 4.4), "kill @e[type=!minecraft:player]"):
                print(cmd, "->", c(cmd)[:200], flush=True)
        elif part == "limpar":
            print(c("tick unfreeze"), c(area), c("kill @e[type=item]"))
        elif part.startswith("print:"):
            # `print:NOME`: um print do jogo em `PASTA_PRINTS` do desktop (o `horizon-ctl screenshot` grava dentro do container).
            import subprocess
            name = part.split(":", 1)[1]
            c("tp @s " + VIDEO_TP[game])
            time.sleep(1.5)
            h.ctl("screenshot", f"/tmp/{name}.png")
            pasta = os.environ.get("PASTA_PRINTS", "~/.cache/rust-mekanism/tanques-9/prints")
            subprocess.run(["ssh", "desktop", f"mkdir -p {pasta} && docker exec {os.environ['H_NAME']} cat /tmp/{name}.png > {pasta}/{name}.png"], check=True)
        elif part == "f1":
            h.ctl("key", "f1", "tap")
        elif part == "video_antes":
            h.ctl("give", "mekanism:configurator", "1")
            run(video_cmds(game)[0][1])
            c("tp @s " + VIDEO_TP[game])
        elif part == "video_depois":
            run(video_cmds(game)[1][1])
        elif part == "drops":
            run(drop_cmds(game), lambda said, cmd, out: print("port", said, "|", cmd.split(" run ")[-1][:60], "->", out[:700], flush=True))
        elif part == "lava":
            run(lava_cmds(game), lambda said, cmd, out: print("port lava |", out[:500], flush=True))
        else:
            wanted = CENARIOS + SEM_RASTRO if part == "rastros" else [tuple(part.split(":"))]
            for kind, scenario in wanted:
                c("tick unfreeze"); c(area); c("kill @e[type=item]")
                time.sleep(0.5)
                steps, end, _ = scenario_ops(game, kind, scenario)
                readings, tick = {}, None
                for op in steps:
                    if op[0] == "cmd":
                        out = c(op[1])
                        if "rror" in out or "nknown" in out or "ncorrect" in out:
                            print("   comando recusado:", op[1][:120], "->", out[:200], flush=True)
                    elif op[0] == "conexao":
                        run(["#conexao %s %s=%s" % (op[1], op[2], op[3])])
                    elif op[0] == "wait":
                        time.sleep(op[1] / 20.0)
                    elif op[0] == "step":
                        c("tick step 1")
                        time.sleep(0.4)
                    elif op[0] == "mark":
                        tick = op[1]
                    elif op[0] == "read":
                        out = c("data get block %s" % op[2])
                        readings.setdefault(tick, {})[op[1]] = stored(out, kind, op[3], game)
                save(game, {"%s:%s" % (kind, scenario): readings})
                if scenario == "absorver":
                    print(f"port {kind}:absorver:", " ".join("t%d S=%d D=%d" % (t, readings[t]["S"][1], readings[t]["D"][1]) for t in sorted(readings)), flush=True)
                else:
                    total += compare("port", kind, scenario, readings, end)
    return total


if __name__ == "__main__":
    if sys.argv[1] == "roteiro":
        json.dump(roteiro(), sys.stdout, indent=1)
    elif sys.argv[1] == "original":
        sys.exit(1 if original(sys.argv[2]) else 0)
    elif sys.argv[1] == "comparar":
        sys.exit(1 if comparar(sys.argv[2], sys.argv[3]) else 0)
    elif sys.argv[1] == "port":
        sys.exit(1 if port(sys.argv[2:]) else 0)
    else:
        raise SystemExit(__doc__)
