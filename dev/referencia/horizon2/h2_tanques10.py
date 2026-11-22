#!/usr/bin/env python3
"""A 10.ª rodada da frente `tanques` em jogo, no container do port (H_NAME): o leite como fluido, o item do Fluid Tank numa entidade contra o rastro
`tank_fluid_item_entity` e as opções de `general.toml` que os tanques leem.

    H_NAME=horizon BASE=x,y,z h2_tanques10.py [leite] [entidade] [config] [aba]

`leite`: o balde de leite esvazia num Fluid Tank e um balde vazio o tira de volta (`BucketResourceHandler` do balde de leite), e o item no modo balde não põe leite no
mundo (`FluidType.canBePlacedInLevel`). `entidade`: o lobo manso toma o clique antes do item, a posse tomada numa entidade no criativo não fica, o jogador criativo
ganha um item novo com o leite e a mão não muda, a ovelha só dá a posse, e o tanque criativo aceita o leite e segue vazio. `config`: `dumpExcessKeepRatio` e
`radiation.enabled` trocados no arquivo com o mundo aberto valem para os tanques que já existem. `aba`: os tanques cheios da aba criativa (`ui`), para as opções
`prefilled` (o arquivo trocado só vale num mundo aberto depois).
A cena só é montada se todos os blocos dela forem ar. O `general.toml` é lido e escrito no desktop por `ssh`.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h
import h2_tanques_item as t

X, Y, Z = t.BASE
TARGET, AQUI = t.TARGET, t.AQUI
GENERAL = "~/.local/share/jogo-isolado/%s/data/horizon/config/Mekanism/general.toml" % os.environ.get("H_NAME", "horizon")


def c(text):
    return " | ".join(h.command(text))


def tank_of(where):
    """`(fluido, mB)` do Fluid Tank em `where`."""
    out = c(f"data get block {where}")
    found = re.search(r"tanks: \[\{([^}]*)\}", out)
    if not found:
        return ("", 0)
    amount = re.search(r"amount: (\d+)L", found.group(1))
    resource = re.search(r'resource: "([^"]+)"', found.group(1))
    amount = int(amount.group(1)) if amount else 0
    return (resource.group(1) if resource and amount else "", amount)


def chemical_of(where):
    out = c(f"data get block {where}")
    found = re.search(r"chemical_tanks: \[\{(.*?)\}\]", out)
    if not found:
        return ("", 0)
    amount = int(re.search(r"amount: (\d+)L", found.group(1)).group(1))
    resource = re.search(r'type: "([^"]+)"', found.group(1))
    return (resource.group(1) if amount and resource else "", amount)


def aim_block():
    h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 60")
    time.sleep(0.4)
    h.ctl("look-at", "block", str(X), str(Y), str(Z))
    time.sleep(0.3)


def leite():
    h.command("gamemode survival")
    h.command("clear @s")
    h.command(f"setblock {TARGET} mekanism:basic_fluid_tank")
    time.sleep(0.5)
    h.command("item replace entity @s hotbar.0 with minecraft:milk_bucket")
    h.ctl("select", "1")
    aim_block()
    h.ctl("use-item")
    time.sleep(0.8)
    got, text = t.hand()
    t.confere("leite: o balde de leite esvazia no tanque", tank_of(TARGET) == ("minecraft:milk", 1000), str(tank_of(TARGET)))
    t.confere("leite: fica o balde vazio na mão", bool(got) and got[0] == "minecraft:bucket", text[:120])
    h.ctl("use-item")
    time.sleep(0.8)
    got, text = t.hand()
    t.confere("leite: o balde vazio tira o leite do tanque", tank_of(TARGET) == ("", 0), str(tank_of(TARGET)))
    t.confere("leite: o balde de leite volta à mão", bool(got) and got[0] == "minecraft:milk_bucket", text[:120])
    # O tanque com leite por comando, para o print (o leite se vê dentro).
    h.command(f'data merge block {TARGET} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:milk",amount:24000L}}]}}}}')
    time.sleep(0.5)
    t.confere("leite: o tanque guarda o leite posto por comando", tank_of(TARGET) == ("minecraft:milk", 24000), str(tank_of(TARGET)))
    h.command(f"setblock {TARGET} minecraft:air")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")
    # O item no modo balde com leite, agachado sobre o chão: o leite não tem bloco e não é posto (rastro `use_place_milk`: `fail`).
    h.command("clear @s")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', 'milk', 5000, mode=True)}")
    h.ctl("select", "1")
    t.stand(True)
    h.ctl("use-item")
    time.sleep(0.6)
    h.ctl("use-item")
    time.sleep(0.6)
    t.release()
    got, text = t.hand()
    t.confere("leite: o item não põe leite no mundo", t.is_air(TARGET) and bool(got) and got[2:] == ["minecraft:milk", 5000], f"{t.block(TARGET)} {got}")
    h.command("gamemode creative")
    h.command("clear @s")


def summon(kind, extra=""):
    h.command(f"summon minecraft:{kind} {X + 0.5} {Y + 1} {Z + 0.5} {{NoAI:1b{extra}}}")
    time.sleep(0.4)


def aim_mob():
    h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 20")
    time.sleep(0.5)
    h.ctl("look-at", str(X + 0.5), str(Y + 1.6), str(Z + 0.5))
    time.sleep(0.3)


def use(times=1):
    for _ in range(times):
        h.ctl("use-item")
        time.sleep(0.7)


def entidade():
    h.command(f"setblock {TARGET} minecraft:glass")
    uuid = re.search(r"\[I; ([^\]]+)\]", c("data get entity @s UUID")).group(1)
    # O lobo manso senta: o `interact` dele toma o clique, e o item fica sem dono (rastro `wolf_takes_the_click`).
    h.command("gamemode survival")
    h.command("clear @s")
    summon("wolf", f",Owner:[I;{uuid}]")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', mode=True)}")
    h.ctl("select", "1")
    aim_mob()
    antes = c(f"data get entity @e[type=minecraft:wolf,{AQUI},limit=1] Sitting")
    use()
    depois = c(f"data get entity @e[type=minecraft:wolf,{AQUI},limit=1] Sitting")
    _, text = t.hand()
    t.confere("entidade: o lobo manso toma o clique (senta)", antes[-2:] != depois[-2:], f"{antes[-30:]} -> {depois[-30:]}")
    t.confere("entidade: o item segue sem dono diante do lobo", "mekanism:owner" not in text, text[:160])
    h.command(f"kill @e[type=minecraft:wolf,{AQUI}]")
    # A ovelha: o primeiro uso toma posse, o segundo passa (rastro `sheep`).
    h.command("clear @s")
    summon("sheep")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', mode=True)}")
    aim_mob()
    use()
    got, text = t.hand()
    t.confere("entidade: diante da ovelha o primeiro uso toma posse", "mekanism:owner" in text and got[3] == 0, text[:160])
    use()
    got, text = t.hand()
    t.confere("entidade: a ovelha não dá leite", bool(got) and got[3] == 0, str(got))
    h.command(f"kill @e[type=minecraft:sheep,{AQUI}]")
    # O tanque criativo com dono: o leite entra e some (rastro `milk_creative_item`).
    summon("cow")
    h.command("clear @s")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('creative', mode=True)}")
    aim_mob()
    use(2)
    got, text = t.hand()
    t.confere("entidade: o tanque criativo aceita o leite e segue vazio", "mekanism:owner" in text and got[3] == 0, text[:160])
    # A pilha de 2: um item por vez (rastro `milk_stack_of_2`).
    h.command("clear @s")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', mode=True, count=2)}")
    aim_mob()
    use(2)
    got, _ = t.hand()
    outros = t.inventory()
    t.confere("entidade: a pilha de 2 enche um e manda o cheio ao inventário", bool(got) and got[1] == 1 and got[3] == 0 and [o[1:] for o in outros] == [["mekanism:basic_fluid_tank", 1, "minecraft:milk", 1000]], f"{got} {outros}")
    use()
    got, _ = t.hand()
    t.confere("entidade: o que ficou na mão enche no uso seguinte", bool(got) and got[1:] == [1, "minecraft:milk", 1000], str(got))
    # O criativo sem dono: dois usos, e o item segue sem dono e vazio (rastro `creative_claim_cow`).
    h.command("clear @s")
    h.command("gamemode creative")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', mode=True)}")
    aim_mob()
    use(2)
    got, text = t.hand()
    t.confere("entidade: no criativo a posse tomada na vaca não fica", "mekanism:owner" not in text and got[3] == 0 and t.inventory() == [], f"{text[:160]} {t.inventory()}")
    # O criativo com dono: a mão não muda e um item novo com o leite entra no inventário, uma vez só (rastro `creative_milk_cow_owned`).
    h.command("gamemode survival")
    h.command("clear @s")
    h.command(f"item replace entity @s hotbar.0 with {t.tank('basic', mode=True)}")
    aim_mob()
    use()
    _, text = t.hand()
    t.confere("entidade: a posse tomada na vaca em sobrevivência fica", "mekanism:owner" in text, text[:160])
    h.command("gamemode creative")
    aim_mob()
    use(2)
    got, _ = t.hand()
    outros = t.inventory()
    t.confere("entidade: no criativo a mão não muda e um item com leite entra, uma vez", bool(got) and got[3] == 0 and [o[1:] for o in outros] == [["mekanism:basic_fluid_tank", 1, "minecraft:milk", 1000]], f"{got} {outros}")
    h.command(f"kill @e[type=minecraft:cow,{AQUI}]")
    h.command(f"setblock {TARGET} minecraft:air")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")
    h.command("clear @s")


def general(old, new):
    """Troca um trecho do `general.toml` do desktop e diz se trocou (uma ocorrência só)."""
    import base64
    payload = base64.b64encode(json.dumps([old, new]).encode()).decode()
    code = ("import base64,os,json;p=os.path.expanduser('%s');o,n=json.loads(base64.b64decode('%s'));s=open(p).read();print(s.count(o));open(p,'w').write(s.replace(o,n))"
            % (GENERAL, payload))
    out = subprocess.run(["ssh", "desktop", 'python3 -c "%s"' % code], capture_output=True, text=True)
    return out.stdout.strip() == "1"


SIDES = "component_config:{config2:[I;7,7,7,7,7,7],eject2:%s}"


def config():
    h.command("gamemode creative")
    h.command("tick unfreeze")
    a, b = f"{X} {Y} {Z}", f"{X + 1} {Y} {Z}"
    # `dumpExcessKeepRatio`: o tanque básico (64.000) em `DUMPING_EXCESS` desce até 0,85 da capacidade, e com o arquivo em 0,5 segue até a metade.
    h.command(f'setblock {a} mekanism:basic_chemical_tank[facing=north]{{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:hydrogen",amount:64000L}}}}],dumping:"dumping_excess"}}')
    time.sleep(3.0)
    t.confere("config: o descarte para em 0,85 da capacidade", chemical_of(a) == ("mekanism:hydrogen", 54400), str(chemical_of(a)))
    t.confere("config: o arquivo tem a chave", general("dumpExcessKeepRatio = 0.85", "dumpExcessKeepRatio = 0.5"))
    time.sleep(5.0)
    t.confere("config: com 0,5 no arquivo o mesmo tanque desce à metade", chemical_of(a) == ("mekanism:hydrogen", 32000), str(chemical_of(a)))
    general("dumpExcessKeepRatio = 0.5", "dumpExcessKeepRatio = 0.85")
    h.command(f"setblock {a} minecraft:air")
    # `radiation.enabled`: o tanque criativo de polônio ejeta para o básico ao lado, que o recusa com a radiação ligada e o aceita com ela desligada.
    h.command(f"setblock {b} mekanism:basic_chemical_tank[facing=north]{{{SIDES % '0b'}}}")
    h.command(f'setblock {a} mekanism:creative_chemical_tank[facing=north]{{chemical_tanks:[{{container:0b,stored:{{type:"mekanism:polonium",amount:9223372036854775807L}}}}],{SIDES % "1b"}}}')
    time.sleep(3.0)
    t.confere("config: com a radiação ligada o tanque comum recusa o polônio", chemical_of(b) == ("", 0), str(chemical_of(b)))
    t.confere("config: o arquivo tem a chave da radiação", general("#Enable worldwide radiation effects.\n\tenabled = true", "#Enable worldwide radiation effects.\n\tenabled = false"))
    time.sleep(5.0)
    got = chemical_of(b)
    t.confere("config: com a radiação desligada no arquivo o mesmo tanque aceita o polônio", got[0] == "mekanism:polonium" and got[1] > 0, str(got))
    general("#Enable worldwide radiation effects.\n\tenabled = false", "#Enable worldwide radiation effects.\n\tenabled = true")
    time.sleep(3.0)
    h.command(f"setblock {a} minecraft:air")
    h.command(f"setblock {b} minecraft:air")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")


def fim_da_aba(pagina2, x, y):
    """Abre a tela criativa, escolhe a aba cujo centro fica em `(x, y)` da tela de 195 por 136 (na segunda página se `pagina2`), rola até o fim e devolve as 45
    pilhas da grade."""
    h.ctl("open-inventory")
    time.sleep(0.8)
    w, hh = json.loads(h.ctl("ui"))["gui_size"]
    left, top = (w - 195) / 2, (hh - 136) / 2
    # Os botões `<` e `>` das páginas de abas, acima dos cantos: a tela guarda a página da última vez, então volta-se à primeira antes.
    h.ctl("click-at", str(int(left + 11)), str(int(top - 40)))
    time.sleep(0.4)
    if pagina2:
        h.ctl("click-at", str(int(left + 184)), str(int(top - 40)))
        time.sleep(0.5)
    h.ctl("click-at", str(int(left + x)), str(int(top + y)))
    time.sleep(0.5)
    h.ctl("scroll-at", str(int(left + 90)), str(int(top + 60)), "-400")
    time.sleep(0.8)
    stacks = [(s.get("stack") or {}) for s in json.loads(h.ctl("ui"))["widgets"] if s["kind"] == "slot"][:45]
    return stacks


def cheios(stacks, item):
    return len([s for s in stacks if s.get("item") == item and "components" in s])


def aba():
    """Os tanques criativos cheios no fim de Tools & Utilities (a fileira de baixo, coluna 0: só os de fluido) e no fim da aba do Mekanism (a segunda página,
    fileira de cima, coluna 0: os de químico fecham a aba, e a grade mostra 45). Imprime `ABA tools_fluido=N mod_quimico=N mod_fluido=N`."""
    h.command("gamemode creative")
    tools = fim_da_aba(False, 13, 136 + 14)
    h.ctl("screenshot", "/tmp/aba_tools.png")
    h.ctl("close")
    time.sleep(0.3)
    mod = fim_da_aba(True, 13, -14)
    h.ctl("screenshot", "/tmp/aba_mod.png")
    h.ctl("close")
    print("ABA tools_fluido=%d mod_quimico=%d mod_fluido=%d mod_itens=%s" % (
        cheios(tools, "mekanism:creative_fluid_tank"), cheios(mod, "mekanism:creative_chemical_tank"), cheios(mod, "mekanism:creative_fluid_tank"),
        sorted({s.get("item", "") for s in mod})[:4]), flush=True)


# ---------------------------------------------------------------------------------------------------------------------------------------
# A cena dos dois jogos (os mesmos comandos no port e no `CenaBlocos` do original): o leite que um tanque ligado pelo Configurator empurra pelos canos, lido por
# NBT a cada tick; a válvula de um tanque que recebe pelo topo; e o item de um Energy Cube destruído pelo nível.
#
#     h2_tanques10.py roteiro > ../cenas/tanques10.json     o roteiro do `CenaBlocos`
#     H_NAME=horizon h2_tanques10.py cena PARTE...          no port: `setup`, `leite_antes`, `leite_video`, `leite_medida`, `valvula`, `cubo`, `print:NOME`, `limpar`
#     h2_tanques10.py original LOG                          lê o log do cliente do original e compara com as leituras do port

TICKS, CLIQUES = 80, (10, 40, 50)
VALVULA_ATE = 10
LEITURAS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "..", "tanques-10-scratch", "r10", "leituras_%s.json")


def leite_cmds(game, medir):
    """Um Fluid Tank básico com 32.000 mB de leite sobre o primeiro de três canos, um básico vazio no fim e um avançado com leite ao lado. Com o servidor parado e um
    `tick step` de cada vez: o clique agachado do Configurator liga o tanque de cima no tick 10 (`setActive`: 500 mB por tick passam ao do fim), desliga no 40 e
    liga de novo no 50. Com `medir`, antes de cada passo o NBT dos dois tanques é lido."""
    import h2_tanques9 as t9
    bx, by, bz = t9.BASE[game]
    at = lambda x, y=0, z=0: "%d %d %d" % (bx + x, by + y, bz + z)
    antes = [
        "tick unfreeze",
        t9.CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2),
        "kill @e[type=item]",
        "setblock %s mekanism:basic_mechanical_pipe" % at(0),
        "setblock %s mekanism:basic_mechanical_pipe" % at(1),
        "setblock %s mekanism:basic_mechanical_pipe" % at(2),
        "setblock %s mekanism:basic_fluid_tank" % at(3),
        "setblock %s mekanism:basic_fluid_tank{%s}" % (at(0, 1), t9.contents(game, "fluid", "minecraft:milk", 32000)),
        "setblock %s mekanism:advanced_fluid_tank{%s}" % (at(5), t9.contents(game, "fluid", "minecraft:milk", 48000)),
        "#esperar 40",
        "tick freeze",
        "#esperar 10",
    ]
    depois = []
    for tick in range(TICKS):
        if tick in CLIQUES:
            depois += ["#agachado %s" % at(0, 1), "#esperar 10"]
        if medir:
            depois += ["say T10 leite %d" % tick, "data get block %s" % at(0, 1), "data get block %s" % at(3)]
        depois += ["tick step 1", "#esperar 4"]
    depois.append("#esperar 20")
    return antes, depois, {at(0, 1): "S", at(3): "D"}


def valvula_cmds(game):
    """O cenário `tank_valve_top` do rastro até o tick `VALVULA_ATE`, com o servidor parado: o tanque de baixo recebe água pelo topo e a válvula aparece."""
    import h2_tanques9 as t9
    bx, by, bz = t9.BASE[game]
    cmds = ["tick unfreeze", t9.CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2), "kill @e[type=item]", "#esperar 5"]
    steps, _, _ = t9.scenario_ops(game, "fluid", "tank_valve_top")
    for op in steps:
        if op[0] == "cmd":
            cmds.append(op[1])
        elif op[0] == "conexao":
            cmds.append("#conexao %s %s=%s" % (op[1], op[2], op[3]))
        elif op[0] == "wait":
            cmds.append("#esperar %d" % op[1])
        elif op[0] == "step":
            cmds += ["tick step 1", "#esperar 4"]
        elif op[0] == "mark" and op[1] >= VALVULA_ATE:
            break
    return cmds + ["#esperar 10"]


def cubo_cmds(game):
    """Um Energy Cube básico com metade da energia destruído por `setblock ... destroy`: o item do chão (`Level.destroyBlock(pos, true)`)."""
    import h2_tanques9 as t9
    bx, by, bz = t9.BASE[game]
    x, y, z = bx + 4, by - 1, bz
    pos = "%d %d %d" % (x, y, z)
    return ["tick unfreeze", t9.CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2), "kill @e[type=item]",
            "setblock %s mekanism:basic_energy_cube[facing=north]" % pos, "#esperar 30", "#cubo %s 0.5" % pos, "#esperar 20",
            "say T10 drop cubo_destroy", "setblock %s minecraft:air destroy" % pos, "#esperar 20",
            "execute positioned %s run data get entity @e[type=item,distance=..8,limit=1,sort=nearest] Item" % pos,
            "execute positioned %s if entity @e[type=item,distance=..8]" % pos, "kill @e[type=item]"]


def roteiro():
    import h2_tanques9 as t9
    game = "original"
    bx, by, bz = t9.BASE[game]
    tp = t9.VIDEO_TP[game]
    antes, depois, _ = leite_cmds(game, True)
    shots = [{"name": "t10_leite_antes", "cmds": antes, "tp": tp}, {"name": "t10_leite_depois", "cmds": depois, "tp": tp},
             {"name": "t10_valvula", "cmds": valvula_cmds(game), "tp": tp}, {"name": "t10_cubo", "cmds": cubo_cmds(game), "tp": tp},
             {"name": "t10_limpeza", "cmds": ["tick unfreeze", t9.CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2), "kill @e[type=item]"], "tp": tp}]
    # O jogador entra longe (x 300): sem ele perto, a área da cena não está carregada.
    setup = ["tp @a " + tp, "difficulty peaceful", "time set 6000", "weather clear", "gamerule advance_time false", "gamerule advance_weather false", "gamemode creative @a",
             "fill 45 -60 70 77 -60 110 minecraft:smooth_stone", "fill 52 -59 76 72 -54 88 minecraft:air"]
    return {"setup": setup, "clear": "weather clear", "hud": False, "shots": shots}


def original(log):
    """As falas `T10 leite N` marcam o tick, e cada `has the following block data` depois delas é um dos dois tanques; depois de `T10 drop`, o item do chão."""
    import h2_tanques9 as t9
    game = "original"
    _, _, quem = leite_cmds(game, True)
    readings, tick, drop = {}, None, False
    for line in open(log, errors="replace"):
        said = re.search(r"T10 (\S+) (\S+)\s*$", line.rstrip("\n")) if "T10 " in line else None
        if said:
            tick, drop = (int(said.group(2)), False) if said.group(1) == "leite" else (None, True)
            continue
        if drop:
            found = re.search(r"System chat: (Test (?:passed|failed).*|.* has the following entity data: .*|No entity was found)", line)
            if found and "Server thread" in line:
                print("original cubo |", found.group(1)[:700])
        data = re.search(r"(-?\d+), (-?\d+), (-?\d+) has the following block data: (.*)", line)
        if data and tick is not None:
            name = quem.get("%s %s %s" % data.group(1, 2, 3))
            if name:
                readings.setdefault(str(tick), {})[name] = list(t9.stored(data.group(4), "fluid", "tank", game))
    json.dump(readings, open(LEITURAS % game, "w"), indent=1, sort_keys=True)
    try:
        port = json.load(open(LEITURAS % "port"))
    except FileNotFoundError:
        print("sem as leituras do port")
        return 1
    diffs, states = [], 0
    for t in sorted(set(readings) | set(port), key=int):
        for name in ("S", "D"):
            states += 1
            a, b = port.get(t, {}).get(name), readings.get(t, {}).get(name)
            if a != b:
                diffs.append((t, name, a, b))
    print(f"leite com o tanque ligado pelo Configurator: {states} leituras, {len(diffs)} diferenças entre o port e o original")
    for diff in diffs[:12]:
        print("   t={} {}: port {} original {}".format(*diff))
    for t in ("0", "10", "11", "39", "40", "41", "50", "51", "79"):
        print("   t=%s port %s original %s" % (t, port.get(t), readings.get(t)))
    return len(diffs)


def cena(parts):
    import h2_tanques9 as t9
    game = "port"
    bx, by, bz = t9.BASE[game]
    readings, falhas = {}, 0

    def run(cmds, quem=None):
        tick, drop = None, False
        for cmd in cmds:
            if cmd.startswith("#esperar "):
                if int(cmd.split()[1]) > 4:
                    time.sleep(int(cmd.split()[1]) / 20.0)
            elif cmd.startswith("#conexao "):
                import h2_blocos
                h.ctl("give", "mekanism:configurator", "1")
                h2_blocos.conexao(cmd)
            elif cmd.startswith("#agachado "):
                import h2_ajustes_jogo as aj
                aj.click_at(tuple(int(v) for v in cmd.split()[1:4]), (0.5, 0.5, 0.0))
                c("tp @s " + t9.VIDEO_TP[game])
            elif cmd.startswith("#cubo "):
                import h2_blocos
                c(h2_blocos.comando_do_cubo(cmd, cmds))
            elif cmd.startswith("say T10 "):
                words = cmd.split()
                tick, drop = (int(words[3]), False) if words[2] == "leite" else (None, True)
            else:
                out = c(cmd)
                if tick is not None and cmd.startswith("data get block ") and quem:
                    readings.setdefault(str(tick), {})[quem[cmd[len("data get block "):]]] = list(t9.stored(out, "fluid", "tank", game))
                elif drop and cmd.startswith("execute"):
                    print("port cubo |", out[:700], flush=True)
                elif cmd.startswith("tick step"):
                    time.sleep(0.4)

    for part in parts:
        print(f"== cena {part}", flush=True)
        if part == "setup":
            t9.port(["setup"])
        elif part == "limpar":
            c("tick unfreeze"); c(t9.CLEAR.format(bx - 2, by - 1, bz - 2, bx + 10, by + 3, bz + 2)); c("kill @e[type=item]")
        elif part.startswith("print:"):
            name = part.split(":", 1)[1]
            c("tp @s " + t9.VIDEO_TP[game])
            time.sleep(1.5)
            h.ctl("screenshot", f"/tmp/{name}.png")
            pasta = os.environ.get("PASTA_PRINTS", "~/.cache/rust-mekanism/tanques-10/prints")
            subprocess.run(["ssh", "desktop", f"mkdir -p {pasta} && docker exec {os.environ['H_NAME']} cat /tmp/{name}.png > {pasta}/{name}.png"], check=True)
        elif part == "f1":
            h.ctl("key", "f1", "tap")
        elif part == "leite_antes":
            h.ctl("give", "mekanism:configurator", "1")
            run(leite_cmds(game, False)[0])
            c("tp @s " + t9.VIDEO_TP[game])
        elif part == "leite_video":
            # O roteiro do vídeo (uns 50 s): o tanque de leite parado, o clique do Configurator o liga e o leite passa pelos canos, outro clique o para, outro o
            # liga de novo; depois, com o servidor solto, o item do Fluid Tank no modo balde ordenha a vaca (o primeiro uso toma posse, o segundo tira o leite).
            import h2_ajustes_jogo as aj
            alvo = (bx, by + 1, bz)
            passos = lambda n: [(c("tick step 2"), time.sleep(0.45)) for _ in range(n)]
            clique = lambda: (aj.click_at(alvo, (0.5, 0.5, 0.0)), c("tp @s " + t9.VIDEO_TP[game]), time.sleep(0.5))
            time.sleep(2.0)
            passos(4); clique(); passos(14); clique(); passos(4); clique(); passos(14)
            c("tick unfreeze")
            c("summon minecraft:cow %.1f %d %.1f {NoAI:1b,Rotation:[180f,0f]}" % (bx + 2.5, by - 1, bz - 1.2))
            c("clear @s")
            c("gamemode survival")
            c("item replace entity @s hotbar.0 with " + t.tank("basic", mode=True))
            h.ctl("select", "1")
            time.sleep(1.0)
            h.ctl("look-at", str(bx + 2.5), str(by - 0.2), str(bz - 1.2))
            time.sleep(0.8)
            for _ in range(3):
                h.ctl("use-item")
                time.sleep(2.2)
            _, text = t.hand()
            print("vídeo: a mão ao fim", text[:200], flush=True)
        elif part == "leite_fim":
            # Depois da gravação: a vaca some sem deixar itens, e o jogador volta ao criativo.
            c("gamemode creative")
            c("tp @e[type=minecraft:cow,distance=..12] ~ -200 ~")
            c("clear @s")
        elif part == "leite_medida":
            h.ctl("give", "mekanism:configurator", "1")
            antes, depois, quem = leite_cmds(game, True)
            run(antes)
            c("tp @s " + t9.VIDEO_TP[game])
            run(depois, quem)
            json.dump(readings, open(LEITURAS % game, "w"), indent=1, sort_keys=True)
            for tk in ("0", "10", "11", "39", "40", "41", "50", "51", "79"):
                print("   t=%s %s" % (tk, readings.get(tk)), flush=True)
        elif part == "valvula":
            run(valvula_cmds(game))
            c("tp @s " + t9.VIDEO_TP[game])
        elif part == "cubo":
            run(cubo_cmds(game))
    return falhas


def main():
    quais = sys.argv[1:] or ["leite", "entidade", "config"]
    antes = h.command("data get entity @s Pos")[0]
    back = " ".join(v.rstrip("d") for v in re.search(r"\[(.*?)\]", antes).group(1).split(", "))
    h.command("tick unfreeze")
    if quais != ["aba"]:
        t.scene()
    try:
        for qual in quais:
            print(f"== {qual}", flush=True)
            globals()[qual]()
    finally:
        t.release()
        h.command("tick unfreeze")
        if quais != ["aba"]:
            t.clear_scene()
        h.command("gamemode creative")
        h.command(f"tp @s {back}")
    print(f"falhas: {t.falhas}", flush=True)
    return t.falhas


if __name__ == "__main__":
    if sys.argv[1:2] == ["roteiro"]:
        print(json.dumps(roteiro(), indent=1))
    elif sys.argv[1:2] == ["original"]:
        sys.exit(1 if original(sys.argv[2]) else 0)
    elif sys.argv[1:2] == ["cena"]:
        os.environ.setdefault("H_NAME", "horizon")
        sys.exit(1 if cena(sys.argv[2:]) else 0)
    else:
        sys.exit(1 if main() else 0)
