#!/usr/bin/env python3
"""O item do Fluid Tank usado no mundo, no container do port (H_NAME), contra o rastro `rastros/tank_fluid_item_use.jsonl` do original.

    H_NAME=horizon BASE=x,y,z h2_tanques_item.py [uso] [caldeirao] [dispensador] [leite] [modo] [posto] [coluna] [cartao]

`uso`: os 19 passos do rastro, como jogador. A cena fica no ar, em BASE: o alvo em BASE, um vidro debaixo (o chão do rastro), um anel de vidro nos quatro lados (o
fluido posto não escorre) e o jogador em cima do vidro do norte, olhando para o meio do chão por dentro do alvo: em pé a mira para na fonte do alvo, pela face de
cima (`SOURCE_ONLY`), e agachado ou sem fonte passa por ele e para no chão (`NONE`), como o jogador do rastro. Cada passo usa o item duas vezes (a primeira, no modo
balde, só toma posse) e compara a mão, o que foi para o inventário e o bloco do alvo. Os dois passos com o modo desligado não têm par no jogo: o rastro chama
`ItemBlockFluidTank.use` direto (que passa), e um clique de verdade passa antes pelo `useOn`, que põe o bloco; aqui eles conferem que o tanque foi posto.
`caldeirao`, `dispensador`, `leite`, `modo`: o resto de `ItemBlockFluidTank`, pela leitura do Java (sem rastro). `posto`: o modo que o item traz ao ser posto
(`mekanism:edit_mode` no Fluid Tank, `mekanism:dump_mode` no Chemical Tank). `coluna`: o NBT de um Fluid Tank mexido por fora no tick em que o de cima empurra para
ele. `cartao`: o Configurator e o cartão de configuração no Chemical Tank.
A cena só é montada se todos os blocos dela forem ar.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = tuple(int(v) for v in os.environ.get("BASE", "3000,200,3000").split(","))
X, Y, Z = BASE
TARGET = f"{X} {Y} {Z}"
FLOOR = f"{X} {Y - 1} {Z}"
RING = [f"{X} {Y} {Z - 1}", f"{X} {Y} {Z + 1}", f"{X - 1} {Y} {Z}", f"{X + 1} {Y} {Z}"]
CREATIVE = 9223372036854775807
# Os seletores partem da cena, não do jogador.
AQUI = f"x={X},y={Y},z={Z},distance=..8"
falhas = 0


def confere(nome, certo, detalhe=""):
    global falhas
    print(("ok   " if certo else "FALHA"), nome, detalhe if not certo else "", flush=True)
    falhas += 0 if certo else 1


def is_air(where):
    return "passed" in " ".join(h.command(f"execute if block {where} minecraft:air")).lower()


def block(where):
    """`["minecraft:water", fonte]`, ou `["minecraft:air", False]`."""
    for name in ("minecraft:air", "minecraft:water", "minecraft:lava"):
        if "passed" in " ".join(h.command(f"execute if block {where} {name}")).lower():
            if name == "minecraft:air":
                return [name, False]
            source = "passed" in " ".join(h.command(f"execute if block {where} {name}[level=0]")).lower()
            return [name, source]
    return [" ".join(h.command(f"data get block {where}"))[:80], False]


def tank(tier, fluid="", amount=0, count=1, mode=None, extra=""):
    parts = []
    if amount:
        parts.append(f'mekanism:fluids={{containers:[{{amount:{amount}L,type:{{id:"minecraft:{fluid}"}}}}]}}')
    if mode is not None:
        parts.append(f"mekanism:bucket_mode={'true' if mode else 'false'}")
    if extra:
        parts.append(extra)
    item = f"mekanism:{tier}_fluid_tank"
    return item + (f"[{','.join(parts)}]" if parts else "") + (f" {count}" if count != 1 else "")


def spec_stack(spec, mode):
    part = spec.split(":")
    if part[0] == "ftankfull":
        return tank(part[1], part[2], CREATIVE, int(part[3]), mode)
    return tank(part[1], part[2], int(part[3]), int(part[4]), mode)


def stack_of(text):
    """`[item, quantidade, fluido, mB]` de um SNBT de pilha."""
    ids = re.findall(r'id: "([^"]+)"', text)
    if not ids:
        return None
    # O id da pilha vem depois dos componentes (o primeiro `id` pode ser o do fluido).
    item = re.match(r"(.*)", ids[-1])
    count = re.search(r"count: (\d+)", text)
    fluid = re.search(r'"mekanism:fluids": \{containers: \[\{amount: (\d+)L, type: \{id: "([^"]+)"', text)
    return [item.group(1), int(count.group(1)) if count else 1, fluid.group(2) if fluid else "", int(fluid.group(1)) if fluid else 0]


def hand():
    out = " ".join(h.command("data get entity @s SelectedItem"))
    return stack_of(out), out


def bodies(text):
    """Os compostos de primeiro nível de uma lista SNBT."""
    depth, start, out = 0, None, []
    for i, c in enumerate(text):
        if c == "{":
            if depth == 0:
                start = i
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                out.append(text[start:i + 1])
    return out


def inventory():
    """As outras pilhas do inventário, `[slot, item, quantidade, fluido, mB]`."""
    out = " ".join(h.command("data get entity @s Inventory"))
    stacks = []
    for body in bodies(out.split("entity data: ", 1)[-1]):
        slot = re.search(r"Slot: (\d+)b", body)
        if slot and int(slot.group(1)) != 0:
            stacks.append([int(slot.group(1))] + stack_of(body))
    return stacks


def scene():
    for where in [TARGET, FLOOR] + RING + [f"{X} {Y + 1} {Z - 1}", f"{X} {Y + 2} {Z - 1}", f"{X} {Y + 1} {Z}"]:
        if not is_air(where):
            raise SystemExit(f"{where} não é ar: a cena não é montada")
    for where in [FLOOR] + RING:
        h.command(f"setblock {where} minecraft:glass")


def clear_scene():
    for where in [TARGET, FLOOR] + RING:
        h.command(f"setblock {where} minecraft:air")


def stand(sneak):
    h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 60")
    time.sleep(0.4)
    if sneak:
        h.ctl("key", "shift", "down")
        time.sleep(0.4)
    h.ctl("look-at", str(X + 0.5), str(Y), str(Z + 0.5))
    time.sleep(0.3)


def release():
    h.ctl("key", "shift", "up")
    time.sleep(0.2)


def uso():
    lines = [json.loads(line) for line in open(os.path.join(RASTROS, "tank_fluid_item_use.jsonl"))]
    for line in lines:
        if "step" not in line:
            continue
        at = line["step"]
        h.command("gamemode " + ("creative" if line["creative"] else "survival"))
        h.command("clear @s")
        h.command(f"setblock {TARGET} minecraft:{line['block']}")
        h.command(f"item replace entity @s hotbar.0 with {spec_stack(line['stack'], line['mode'])}")
        h.ctl("select", "1")
        stand(line["sneak"])
        h.ctl("use-item")
        time.sleep(0.6)
        first, text = hand()
        if line["mode"]:
            # O primeiro uso só toma posse: a mão ganha o dono e o resto não muda.
            confere(f"{at}: o primeiro uso toma posse e não mexe no mundo", "mekanism:owner" in text and block(TARGET)[0] == "minecraft:" + line["block"], text[:160])
            # O `use-item` do `horizon-ctl` solta o agachar (medido com a mão vazia e com um graveto): aperta de novo antes do segundo uso.
            if line["sneak"]:
                h.ctl("key", "shift", "down")
                time.sleep(0.4)
            h.ctl("look-at", str(X + 0.5), str(Y), str(Z + 0.5))
            h.ctl("use-item")
            time.sleep(0.6)
            got, text = hand()
            confere(f"{at}: a mão", got == line["hand"][:4], f"{got} != {line['hand'][:4]}")
            extra = inventory()
            want = [entry[:5] for entry in line["inventory"]]
            confere(f"{at}: o inventário", extra == want, f"{extra} != {want}")
            after = block(TARGET)
            want = [line["after"][0], line["after"][2]]
            confere(f"{at}: o alvo depois", after == want, f"{after} != {want}")
        else:
            # Modo desligado: o `useOn` põe o tanque (o rastro só chama o `use`, que passa).
            placed = " ".join(h.command(f"data get block {TARGET}"))
            confere(f"{at}: com o modo desligado o item põe o bloco", "fluid_tanks" in placed or "mekanism" in placed, placed[:120])
        release()
        h.command(f"setblock {TARGET} minecraft:air")
    h.command("gamemode creative")
    h.command("clear @s")


def caldeirao():
    h.command("gamemode survival")
    passos = [
        ("água do caldeirão cheio para o item vazio", "water_cauldron[level=3]", tank("basic", mode=True), ["minecraft:water", 1000], "minecraft:cauldron"),
        ("um nível só não fecha a troca (o caldeirão solta um balde inteiro)", "water_cauldron[level=1]", tank("basic", mode=True), ["", 0], "minecraft:water_cauldron"),
        ("o item enche o caldeirão vazio", "cauldron", tank("basic", "water", 5000, mode=True), ["minecraft:water", 4000], "minecraft:water_cauldron[level=3]"),
        ("lava do item para o caldeirão", "cauldron", tank("advanced", "lava", 2000, mode=True), ["minecraft:lava", 1000], "minecraft:lava_cauldron"),
        ("lava do caldeirão para o item", "lava_cauldron", tank("advanced", "lava", 2000, mode=True), ["minecraft:lava", 3000], "minecraft:cauldron"),
        ("menos de um balde não enche", "cauldron", tank("basic", "water", 999, mode=True), ["minecraft:water", 999], "minecraft:cauldron"),
        ("fora do modo balde o caldeirão não troca (e o item põe o bloco em cima dele)", "water_cauldron[level=3]", tank("basic", mode=False), None, "minecraft:water_cauldron[level=3]"),
    ]
    for nome, antes, item, mao, depois in passos:
        h.command("clear @s")
        h.command(f"setblock {TARGET} minecraft:{antes}")
        # O dono já vem no item (o do jogador), para o clique não ser o da posse.
        h.command(f"item replace entity @s hotbar.0 with {item}")
        h.ctl("select", "1")
        stand(False)
        if "bucket_mode=true" in item:
            h.ctl("use-item")  # toma posse (a mira está no caldeirão; a posse vem antes de tudo no `use`, e o caldeirão responde antes dela)
            time.sleep(0.5)
        got, text = hand()
        # O caldeirão responde no `useItemOn` do bloco, antes do `use` do item: o primeiro clique já troca.
        ok_block = "passed" in " ".join(h.command(f"execute if block {TARGET} {depois}")).lower()
        if mao is not None and not (got and got[2:] == mao and ok_block) and "bucket_mode=true" in item:
            h.ctl("use-item")
            time.sleep(0.5)
            got, text = hand()
            ok_block = "passed" in " ".join(h.command(f"execute if block {TARGET} {depois}")).lower()
        certo = (got is None if mao is None else bool(got) and got[2:] == mao) and ok_block
        confere(f"caldeirão: {nome}", certo, f"mão {got}, bloco {'certo' if ok_block else 'errado'}")
        release()
        h.command(f"setblock {X} {Y + 1} {Z} minecraft:air")
    h.command(f"setblock {TARGET} minecraft:air")
    h.command("gamemode creative")
    h.command("clear @s")


def dispensador():
    # O dispensador no lugar do vidro do norte, virado para o sul (o alvo).
    disp = f"{X} {Y} {Z - 1}"
    passos = [
        ("pega a fonte da frente", "water", tank("basic", mode=True), ["mekanism:basic_fluid_tank", 1, "minecraft:water", 1000], ["minecraft:air", False]),
        ("põe um balde no ar", "air", tank("basic", "water", 5000, mode=True), ["mekanism:basic_fluid_tank", 1, "minecraft:water", 4000], ["minecraft:water", True]),
        ("cheio diante de uma fonte: nada passa, o item é solto", "water", tank("basic", "water", 32000, mode=True), None, ["minecraft:water", True]),
        ("fora do modo balde o item é solto", "water", tank("basic", mode=False), None, ["minecraft:water", True]),
    ]
    for nome, antes, item, fica, depois in passos:
        h.command(f"kill @e[type=minecraft:item,{AQUI}]")
        h.command(f"setblock {disp} minecraft:dispenser[facing=south]")
        h.command(f"setblock {TARGET} minecraft:{antes}")
        time.sleep(0.4)
        h.command(f"item replace block {disp} container.0 with {item}")
        h.command(f"setblock {X} {Y + 1} {Z - 1} minecraft:redstone_block")
        time.sleep(1.2)
        inside = " ".join(h.command(f"data get block {disp} Items"))
        got = stack_of(inside) if "id:" in inside else None
        after = block(TARGET)
        dropped = " ".join(h.command(f"data get entity @e[type=minecraft:item,limit=1,sort=nearest,{AQUI}] Item"))
        ok = got == fica and (after == depois or fica is None and after[0] == depois[0]) and (("fluid_tank" in dropped) == (fica is None))
        confere(f"dispensador: {nome}", ok, f"dentro {got}, alvo {after}, solto {dropped[:80]}")
        h.command(f"setblock {X} {Y + 1} {Z - 1} minecraft:air")
        h.command(f"setblock {disp} minecraft:air")
        h.command(f"setblock {TARGET} minecraft:air")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")
    h.command(f"setblock {disp} minecraft:glass")


def leite():
    h.command("gamemode survival")
    h.command(f"setblock {TARGET} minecraft:glass")
    for bicho, som in (("cow", "vaca"), ("goat", "cabra")):
        h.command("clear @s")
        h.command(f"summon minecraft:{bicho} {X + 0.5} {Y + 1} {Z + 0.5} {{NoAI:1b}}")
        h.command(f"item replace entity @s hotbar.0 with {tank('basic', mode=True)}")
        h.ctl("select", "1")
        h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 20")
        time.sleep(0.5)
        h.ctl("look-at", str(X + 0.5), str(Y + 1.6), str(Z + 0.5))
        h.ctl("use-item")
        time.sleep(0.6)
        first, text = hand()
        confere(f"leite ({som}): o primeiro clique toma posse", "mekanism:owner" in text and first[3] == 0, text[:140])
        h.ctl("use-item")
        time.sleep(0.6)
        got, text = hand()
        confere(f"leite ({som}): um balde de leite no item", bool(got) and got[2:] == ["minecraft:milk", 1000], str(got))
        h.command(f"kill @e[type=minecraft:{bicho},{AQUI}]")
    h.command(f"setblock {TARGET} minecraft:air")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")
    h.command("gamemode creative")
    h.command("clear @s")


def modo():
    h.command("clear @s")
    h.command(f"item replace entity @s hotbar.0 with {tank('basic')}")
    h.ctl("select", "1")
    h.ctl("key", "n", "tap")
    time.sleep(0.6)
    _, text = hand()
    confere("modo: a tecla do modo liga o modo balde", '"mekanism:bucket_mode": 1b' in text, text[:160])
    h.ctl("key", "n", "tap")
    time.sleep(0.6)
    _, text = hand()
    confere("modo: a tecla de novo desliga", '"mekanism:bucket_mode": 1b' not in text, text[:160])
    h.command("clear @s")


def posto():
    """O modo que o item traz ao ser posto."""
    h.command("gamemode creative")
    for item, chave in (
        ('mekanism:basic_fluid_tank[mekanism:edit_mode="fill"]', 'edit_mode: "fill"'),
        ('mekanism:basic_fluid_tank[mekanism:edit_mode="empty"]', 'edit_mode: "empty"'),
        ('mekanism:basic_chemical_tank[mekanism:dump_mode="dumping"]', 'dumping: "dumping"'),
        ('mekanism:basic_chemical_tank[mekanism:dump_mode="dumping_excess"]', 'dumping: "dumping_excess"'),
    ):
        h.command("clear @s")
        out = " ".join(h.command(f"item replace entity @s hotbar.0 with {item}"))
        h.ctl("select", "1")
        h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 60")
        time.sleep(0.4)
        h.ctl("place", str(X), str(Y), str(Z), "--item", item.split("[")[0])
        time.sleep(1.2)
        data = " ".join(h.command(f"data get block {TARGET}"))
        confere(f"posto: {item}", chave in data, out[:80] + " | " + data[:200])
        h.command(f"setblock {TARGET} minecraft:air")
    h.command("clear @s")


def fluid_of(where):
    data = " ".join(h.command(f"data get block {where}"))
    amount = re.search(r'amount: (\d+)L', data)
    return int(amount.group(1)) if amount else 0


def coluna():
    """Dois Fluid Tanks em coluna, o de cima ligado (empurra para baixo, 1.000 mB por tick no básico). No tick em que um comando esvazia o de baixo, ele fica só com
    o que o de cima empurrou nesse tick: o NBT mexido por fora vale antes do empurrão (no Java o comando mexe no próprio tanque)."""
    top = f"{X} {Y + 1} {Z}"
    h.command("tick freeze")
    # A ordem em que os dois são postos decide qual anda primeiro no tick: `ORDEM=cima` põe o de cima antes.
    for where in ((top, TARGET) if os.environ.get("ORDEM") == "cima" else (TARGET, top)):
        h.command(f"setblock {where} mekanism:basic_fluid_tank")
    h.command("tick step 3")
    time.sleep(0.8)
    h.command(f'data merge block {TARGET} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:water",amount:5000L}}]}}}}')
    h.command(f'data merge block {top} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:water",amount:20000L}}]}},active_state:1b}}')
    h.command("tick step 2")
    time.sleep(0.8)
    antes = (fluid_of(top), fluid_of(TARGET))
    print("coluna: antes", antes, flush=True)
    # O comando esvazia o de baixo; o tick seguinte empurra 1.000 mB para ele.
    h.command(f'data merge block {TARGET} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:water",amount:0L}}]}}}}')
    h.command("tick step 1")
    time.sleep(0.6)
    depois = (fluid_of(top), fluid_of(TARGET))
    print("coluna: depois", depois, flush=True)
    confere("coluna: o de baixo esvaziado por fora fica só com o empurrão do tick", depois[1] == antes[0] - depois[0] and depois[1] <= 1000, f"{antes} -> {depois}")
    # O contrário: o comando mexe no de cima.
    h.command(f'data merge block {top} {{machinery_basis:{{v:1,tanks:[{{resource:"minecraft:water",amount:3000L}}]}}}}')
    baixo = fluid_of(TARGET)
    h.command("tick step 1")
    time.sleep(0.6)
    fim = (fluid_of(top), fluid_of(TARGET))
    push = fim[1] - baixo
    confere("coluna: o de cima mexido por fora empurra a partir do valor novo", push > 0 and fim[0] == 3000 - push, f"{fim}, o de baixo tinha {baixo}")
    h.command(f"setblock {top} minecraft:air")
    h.command(f"setblock {TARGET} minecraft:air")
    h.command("tick unfreeze")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")


def cartao():
    """O Configurator e o cartão de configuração no Chemical Tank: o cartão copia os lados e o modo de descarte de um tanque para outro (`writeConfigurationData`
    leva o `dumping`), e o Configurator agachado em `CONFIGURATE_CHEMICALS` troca o lado clicado."""
    h.command("gamemode creative")
    a, b = TARGET, f"{X} {Y + 1} {Z}"
    h.command(f"setblock {a} mekanism:basic_chemical_tank[facing=north]")
    h.command(f"setblock {b} mekanism:basic_chemical_tank[facing=north]")
    time.sleep(0.8)
    h.command(f'data merge block {a} {{dumping:"dumping_excess",component_config:{{config2:[I;7,7,1,1,4,1],eject2:0b}}}}')
    time.sleep(0.5)
    h.command("clear @s")
    h.command("item replace entity @s hotbar.0 with mekanism:configuration_card")
    h.ctl("select", "1")
    h.command(f"tp @s {X + 0.5} {Y + 1} {Z - 0.5} 0 0")
    time.sleep(0.4)
    h.ctl("key", "shift", "down")
    time.sleep(0.3)
    h.ctl("interact", str(X), str(Y), str(Z), "north")
    time.sleep(0.6)
    h.ctl("key", "shift", "up")
    _, text = hand()
    confere("cartão: agachado copia os dados do tanque", "configuration_data" in text and "dumping" in text, text[:300])
    h.ctl("interact", str(X), str(Y + 1), str(Z), "north")
    time.sleep(0.6)
    data = " ".join(h.command(f"data get block {b}"))
    confere("cartão: o outro tanque recebe os lados e o modo de descarte", 'dumping: "dumping_excess"' in data and "config2: [I; 7, 7, 1, 1, 4, 1]" in data and "eject2: 0b" in data, data[:400])
    # O Configurator em `CONFIGURATE_CHEMICALS`, agachado: troca o lado clicado (a frente, que no item novo é `OUTPUT`, 4) pelo próximo tipo.
    h.command(f"setblock {b} minecraft:air")
    h.command(f"setblock {b} mekanism:basic_chemical_tank[facing=north]")
    time.sleep(0.8)
    antes = re.search(r"config2: \[I; ([^\]]*)\]", " ".join(h.command(f"data get block {b}")))
    h.command('item replace entity @s hotbar.0 with mekanism:configurator[mekanism:configurator_mode="configurate_chemicals"]')
    h.ctl("select", "1")
    h.ctl("key", "shift", "down")
    time.sleep(0.3)
    h.ctl("interact", str(X), str(Y + 1), str(Z), "north")
    time.sleep(0.6)
    h.ctl("key", "shift", "up")
    depois = re.search(r"config2: \[I; ([^\]]*)\]", " ".join(h.command(f"data get block {b}")))
    print("configurator:", antes and antes.group(1), "->", depois and depois.group(1), flush=True)
    confere("Configurator: agachado troca o tipo do lado clicado", bool(antes and depois) and antes.group(1) != depois.group(1) and antes.group(1).split(", ")[1:] == depois.group(1).split(", ")[1:], "")
    h.command(f"setblock {b} minecraft:air")
    h.command(f"setblock {a} minecraft:air")
    h.command("clear @s")
    h.command(f"kill @e[type=minecraft:item,{AQUI}]")


def main():
    quais = sys.argv[1:] or ["uso", "caldeirao", "dispensador", "leite", "modo", "posto", "coluna", "cartao"]
    antes = h.command("data get entity @s Pos")[0]
    back = " ".join(v.rstrip("d") for v in re.search(r"\[(.*?)\]", antes).group(1).split(", "))
    h.command("tick unfreeze")
    scene()
    try:
        for qual in quais:
            print(f"== {qual}", flush=True)
            globals()[qual]()
    finally:
        release()
        h.command("tick unfreeze")
        clear_scene()
        h.command("gamemode creative")
        h.command(f"tp @s {back}")
    print(f"falhas: {falhas}", flush=True)
    sys.exit(1 if falhas else 0)


if __name__ == "__main__":
    main()
