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


def empty(game, kind, pos):
    if kind == "chemical":
        return "data remove block %s chemical_tanks" % pos
    if game == "port":
        return "data merge block %s {%s}" % (pos, contents(game, kind, "", 0))
    return "data remove block %s fluid_tanks" % pos


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
    out, actors, dumping, stop = [], {}, {}, None

    def place_tank(words, creative=False):
        if creative:
            name, x, y, z, tier, resource, amount = words[1], words[2], words[3], words[4], "creative", words[5], MAX
        else:
            name, x, y, z, tier, resource, amount = words[1], words[2], words[3], words[4], words[5], words[6], int(words[7])
            amount = 0 if resource == "empty" else amount
        nbt = ",".join(part for part in (contents(game, kind, resource, amount) if amount else "", sides) if part)
        out.append(("cmd", "setblock %s %s%s" % (at(x, y, z), tank % tier, "{%s}" % nbt if nbt else "")))
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
            out.append(("cmd", "setblock %s mekanism:metallurgic_infuser[facing=north]{component_config:{config2:[I;1,1,1,1,1,1]}}" % at(words[2], words[3], words[4])))
            actors[words[1]] = (at(words[2], words[3], words[4]), "machine", "")
        elif head == "active":
            out.append(("cmd", "data merge block %s {active_state:%db}" % (actors[words[1]][0], words[2] == "true")))
        elif head == "eject":
            out.append(("cmd", "data merge block %s {component_config:{eject2:%db}}" % (actors[words[1]][0], words[2] == "true")))
        elif head == "dump":
            dumping[words[1]] = (dumping.get(words[1], 0) + 1) % 3
            out.append(("cmd", 'data merge block %s {dumping:"%s"}' % (actors[words[1]][0], DUMP[dumping[words[1]]])))
        elif head in ("setfluid", "setchem"):
            pos, _, tier = actors[words[1]]
            if words[2] == "empty" or words[3] == "0":
                out.append(("cmd", empty(game, kind, pos)))
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
    # A cena assenta com o servidor andando (os canos formam a rede) e só então para.
    out += [("wait", 40), ("cmd", "tick freeze"), ("wait", 10)]
    end = 0
    for t in range(0, last + 1):
        for mark in marks.get(t, []):
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
    """Um Fluid Tank básico cheio de água e ativo sobre o primeiro de quatro canos, com um tanque básico vazio no fim; o terceiro cano falta. Com o servidor parado
    e um `tick step` de cada vez: 20 ticks sem o cano (nada anda), o cano posto (a água passa, 500 mB por tick), o segundo cano destruído (o som e o item, a água
    para), o cano posto de novo e a água até o fim."""
    bx, by, bz = BASE[game]
    at = lambda x, y=0, z=0: "%d %d %d" % (bx + x, by + y, bz + z)
    antes = [
        "tick unfreeze",
        CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2),
        "kill @e[type=item]",
        "setblock %s mekanism:basic_mechanical_pipe" % at(0),
        "setblock %s mekanism:basic_mechanical_pipe" % at(1),
        "setblock %s mekanism:basic_mechanical_pipe" % at(3),
        "setblock %s mekanism:basic_fluid_tank" % at(4),
        "setblock %s mekanism:basic_fluid_tank[active=true]{active_state:1b,%s}" % (at(0, 1), contents(game, "fluid", "minecraft:water", 32000)),
        "#esperar 40",
        "tick freeze",
        "#esperar 10",
    ]
    passos = lambda n: ["tick step 1", "#esperar 4"] * n
    fluxo = passos(20) + ["setblock %s mekanism:basic_mechanical_pipe" % at(2)] + passos(30)
    fluxo += ["setblock %s minecraft:air destroy" % at(1)] + passos(15)
    fluxo += ["setblock %s mekanism:basic_mechanical_pipe" % at(1)] + passos(40) + ["#esperar 20"]
    tp = "%.1f %.1f %.1f 0 20" % (bx + 2.5, by + 1.6, bz - 4.4)
    return [("t9_antes", antes, tp), ("t9_depois", fluxo, tp)]


DROPS = [
    ("fluid_full", "mekanism:basic_fluid_tank", lambda game: contents(game, "fluid", "minecraft:lava", 5000) + ',Items:[{Slot:0b,id:"minecraft:lava_bucket",count:1},{Slot:1b,id:"minecraft:bucket",count:3}],edit_mode:"fill"'),
    ("chemical_full", "mekanism:advanced_chemical_tank", lambda game: contents(game, "chemical", "mekanism:hydrogen", 5000) + ',Items:[{Slot:0b,id:"minecraft:stick",count:2}],dumping:"dumping_excess",component_config:{config2:[I;4,0,0,0,0,0],eject2:1b}'),
]
HOW = {"destroy": "setblock {pos} minecraft:air destroy", "explode": "summon minecraft:tnt {x}.5 {y}.44 {z}.5 {{fuse:0s,explosion_power:6f}}", "tnt": "summon minecraft:tnt {x}.5 {y}.44 {z}.5 {{fuse:0s,explosion_power:4f}}"}


def drop_cmds(game):
    """O tanque cheio (o conteúdo, os slots e o modo) destruído por `setblock ... destroy`, por uma explosão de potência 6 no bloco de cima e pela da TNT (potência
    4, que não o quebra): depois de cada um, o bloco que ficou e o item do chão."""
    bx, by, bz = BASE[game]
    x, y, z = bx + 4, by - 1, bz
    cmds = ["tick unfreeze"]
    for case, block, nbt in DROPS:
        for how, text in HOW.items():
            cmds += [
                CLEAR.format(x - 2, y, z - 2, x + 2, y + 3, z + 2),
                "fill %d %d %d %d %d %d minecraft:obsidian" % (x - 3, y - 1, z - 3, x + 3, y - 1, z + 3),
                "kill @e[type=item]",
                "setblock %d %d %d %s[facing=north]{%s}" % (x, y, z, block, nbt(game)),
                "#esperar 20",
                "say T9 drop %s_%s" % (case, how),
                text.format(pos="%d %d %d" % (x, y, z), x=x, y=y + 1, z=z),
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
    for kind, scenario in CENARIOS:
        cmds = ["tick unfreeze", area, "kill @e[type=item]", "#esperar 5"]
        steps, _, _ = ops(game, kind, scenario)
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
    setup = ["difficulty peaceful", "time set 6000", "weather clear", "gamerule advance_time false", "gamerule advance_weather false", "gamemode creative",
             "fill 45 -60 70 77 -60 110 minecraft:smooth_stone", "fill 52 -59 76 72 -54 88 minecraft:air"]
    return {"setup": setup, "clear": "tick unfreeze", "hud": False, "shots": shots}


def original(log):
    """Lê o log do cliente do original: as falas `T9 ...` marcam o tick, e cada `has the following block data` depois delas é um ator."""
    game = "original"
    by_pos, readings, current, drops, lava = {}, {}, None, {}, []
    for kind, scenario in CENARIOS:
        _, _, actors = ops(game, kind, scenario)
        for name, (pos, what, _) in actors.items():
            by_pos[(kind, scenario, pos.replace(" ", ", "))] = (name, what)
    for line in open(log, errors="replace"):
        said = re.search(r"T9 (\S+) ?(-?\d+)?\s*$", line.rstrip("\n")) if "T9 " in line else None
        if said:
            current = (said.group(1), int(said.group(2)) if said.group(2) else None)
            continue
        if current is None:
            continue
        if current[0] == "drop":
            pass
        data = re.search(r"(-?\d+, -?\d+, -?\d+) has the following block data: (.*)", line)
        if data and ":" in current[0] and current[1] is not None:
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
                time.sleep(int(cmd.split()[1]) / 20.0)
            elif cmd.startswith("#conexao "):
                import h2_blocos
                h.ctl("give", "mekanism:configurator", "1")
                h2_blocos.conexao(cmd)
            elif cmd.startswith("say T9 "):
                said = cmd[7:]
            else:
                out = c(cmd)
                if falar and (cmd.startswith(("data get", "execute")) or "rror" in out or "nknown" in out):
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
        elif part == "video_antes":
            run(video_cmds(game)[0][1])
        elif part == "video_depois":
            run(video_cmds(game)[1][1])
        elif part == "drops":
            run(drop_cmds(game), lambda said, cmd, out: print("port", said, "|", cmd.split(" run ")[-1][:60], "->", out[:700], flush=True))
        elif part == "lava":
            run(lava_cmds(game), lambda said, cmd, out: print("port lava |", out[:500], flush=True))
        else:
            wanted = CENARIOS if part == "rastros" else [tuple(part.split(":"))]
            for kind, scenario in wanted:
                c("tick unfreeze"); c(area); c("kill @e[type=item]")
                time.sleep(0.5)
                steps, end, _ = ops(game, kind, scenario)
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
                total += compare("port", kind, scenario, readings, end)
    return total


if __name__ == "__main__":
    if sys.argv[1] == "roteiro":
        json.dump(roteiro(), sys.stdout, indent=1)
    elif sys.argv[1] == "original":
        sys.exit(1 if original(sys.argv[2]) else 0)
    elif sys.argv[1] == "port":
        sys.exit(1 if port(sys.argv[2:]) else 0)
    else:
        raise SystemExit(__doc__)
