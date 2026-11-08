#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros `neighbors_*.jsonl` do Mekanism original (`NeighborTraceTest.java`): o `redstone` das máquinas
só muda quando o jogo avisa o bloco, e o `canFunction()`, a atividade e o progresso seguem o `redstone` que ficou.

    H_NAME=horizon h2_vizinhos.py CENARIO [--dir DIRETORIO_DE_PRINTS]
    CENARIO: chamber_notified, chamber_unnotified, chamber_mid_tick, chamber_placed, chamber_output_signal, heat_notified, heat_unnotified, heat_placed, heat_output_signal

O servidor fica parado (`tick freeze`) e anda com `tick step 1`: as operações do rastro saem como comandos entre dois ticks (`setblock` avisa os vizinhos; `setblock ...
strict` não avisa, são as flags 2 e 818 do `Level.setBlock`; `interact` puxa a alavanca; `place` põe o bloco como jogador, o `setPlacedBy` do Java). Uma operação `pre`
do rastro (antes do tick da máquina) e uma `step` do tick anterior (depois dele) são o mesmo comando entre os mesmos dois ticks.

Depois de cada tick lê o NBT de cada máquina (`redstone`, `active_state`, `progress`) e compara com o rastro: `active_state` e `progress` com a linha do tick, e `redstone` com
o valor que a máquina usou no tick (o que as operações `pre` deixaram, ou o que o tick anterior terminou com): o NBT só é escrito no tick, então o campo no meio do tick
(o que cada operação dá) não se lê aqui; quem o confere é `mods/mekanism/tests/neighbors_trace.rs`, contra o mesmo rastro. O que o `onNeighborChange` do NeoForge faz com um
bloco do vanilla (`chest ... changed`) o Horizon não avisa: o cenário `*_output_signal` mede essa diferença.
"""
import json, os, re, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_identity as h

RASTROS = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "rastros")
BASE = (0, 150, 0)
MACHINE = {
    "chamber": "mekanism:enrichment_chamber",
    "heat": "mekanismgenerators:heat_generator",
}
WARMUP = 3
FIRST_COMPARED = 7


def at(rel, row=0):
    """A posição no jogo de uma posição relativa do rastro (a linha `row` fica 4 blocos para o sul de cada anterior)."""
    return (BASE[0] + rel[0], BASE[1] + rel[1] - 1, BASE[2] + rel[2] + 4 * row)


# Posições relativas da linha 0, as de `NeighborTraceTest`: máquina, fonte, cutucão e vizinho do norte.
SUBJECT, SOURCE, POKE, OTHER = (3, 1, 2), (2, 1, 2), (4, 1, 2), (3, 1, 1)


def pos(rel, row):
    x, y, z = at(rel, row)
    return f"{x} {y} {z}"


def step(ticks=1):
    h.command(f"tick step {ticks}")
    time.sleep(0.35 + 0.05 * ticks)


def read_machine(row):
    text = h.command(f"data get block {pos(SUBJECT, row)}")[0]
    if "block data" not in text:
        return None
    red = re.search(r"redstone: (\d)b", text)
    active = re.search(r"active_state: (\d)b", text)
    progress = re.search(r"progress: (\d+)", text)
    return {
        "redstone": bool(int(red.group(1))) if red else None,
        "active": bool(int(active.group(1))) if active else None,
        "progress": int(progress.group(1)) if progress else None,
    }


def strict(flags):
    return "" if flags & 1 else " strict"


def apply(op, kind, state):
    """Uma operação do rastro como comandos. Devolve False se o jogo não a tem (descarregar um tile)."""
    w = op.split(" ")
    row = int(w[1]) if len(w) > 1 and w[1].lstrip("-").isdigit() else 0
    block = MACHINE[kind]
    if w[0] == "mode":
        h.command(f"data merge block {pos(SUBJECT, row)} {{control_type:\"{w[2].lower()}\"}}") if state["fresh"].get(row) else None
    elif w[0] == "fill":
        if kind == "heat":
            h.command(f"data merge block {pos(SUBJECT, row)} {{machinery_basis:{{v:1,tanks:[{{resource:\"minecraft:lava\",amount:10000L}}]}}}}")
        else:
            h.command(f"data merge block {pos(SUBJECT, row)} {{energy:20000,Items:[{{Slot:0b,id:\"minecraft:clay\",count:64}}]}}")
    elif w[0] == "src":
        block_name = "minecraft:redstone_block" if w[2] == "on" else "minecraft:air"
        h.command(f"setblock {pos(SOURCE, row)} {block_name}{strict(int(w[3]))}")
    elif w[0] == "lever":
        if w[2] == "place":
            h.command(f"setblock {pos(SOURCE, row)} minecraft:lever[face=floor,facing=north,powered=false]{strict(int(w[3]))}")
        elif w[2] == "pull":
            x, y, z = at(SOURCE, row)
            h.command(f"tp @s {x} {y + 2} {z}")
            h.ctl("look", "0", "90")
            h.ctl("interact", str(x), str(y), str(z))
        else:
            h.command(f"setblock {pos(SOURCE, row)} minecraft:air{strict(int(w[3]))}")
    elif w[0] == "poke":
        placed = state["pokes"][row] = not state["pokes"].get(row, False)
        h.command(f"setblock {pos(POKE, row)} {'minecraft:stone' if placed else 'minecraft:air'}{strict(int(w[2]))}")
    elif w[0] == "chest":
        n = state["chest"][row] = state["chest"].get(row, 0) + 1
        # `BlockEntity.setChanged` do baú: o comando de itens avisa o contêiner (`Game::container_changed`), o `data merge` não.
        h.command(f"item replace block {pos(OTHER, row)} container.0 with minecraft:stone {1 + n % 2}")
    elif w[0] == "other":
        h.command(f"setblock {pos(OTHER, row)} mekanism:enrichment_chamber")
    elif w[0] in ("ninput", "nclear"):
        items = "[{Slot:0b,id:\"minecraft:clay\",count:5}]" if w[0] == "ninput" else "[]"
        h.command(f"data merge block {pos(OTHER, row)} {{Items:{items}}}")
    elif w[0] == "clear":
        h.command(f"setblock {pos(SUBJECT, row)} minecraft:air")
        state["fresh"][row] = True
    elif w[0] == "setblock":
        h.command(f"setblock {pos(SUBJECT, row)} {block}{strict(int(w[2]))}")
        state["fresh"][row] = True
    elif w[0] == "place":
        x, y, z = at(SUBJECT, row)
        h.command("clear @s")
        h.ctl("give", block, "1")
        h.command(f"tp @s {x} {y + 3} {z}")
        out = h.ctl("place", str(x), str(y), str(z), "--item", block)
        assert '"error"' not in out, out
        state["fresh"][row] = True
    else:
        return False
    return True


def main():
    scenario = sys.argv[1]
    kind = "chamber" if scenario.startswith("chamber") else "heat"
    lines = [json.loads(l) for l in open(os.path.join(RASTROS, f"neighbors_{scenario}.jsonl"))]
    rows = lines[0]["rows"]
    # O que o gabarito do Java põe antes do primeiro tick: as fontes já ligadas e os baús (sem avisar) e as máquinas, com o modo do primeiro `mode`.
    powered_at_start = scenario.endswith("placed")
    modes = {}
    for line in lines[1:]:
        for op in line["ops"]:
            w = op["op"].split(" ")
            if w[0] == "mode" and int(w[1]) not in modes:
                modes[int(w[1])] = w[2].lower()
    h.command("gamemode creative")
    h.command("tick unfreeze")
    x0, y0, z0 = BASE
    h.command(f"tp @s {x0 + 6} {y0 + 12} {z0 + 4 * rows}")
    h.ctl("wait", "30")
    h.command(f"fill {x0} {y0 - 2} {z0 - 2} {x0 + 12} {y0 - 2} {z0 + 4 * rows + 4} minecraft:stone")
    h.command(f"fill {x0} {y0 - 1} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + 4 * rows + 4} minecraft:air")
    h.command("tick freeze")
    for row in range(rows):
        if powered_at_start:
            h.command(f"setblock {pos(SOURCE, row)} minecraft:redstone_block strict")
        if scenario.endswith("output_signal"):
            h.command(f"setblock {pos(OTHER, row)} minecraft:chest")
        mode = modes.get(row, "disabled")
        h.command(f"setblock {pos(SUBJECT, row)} {MACHINE[kind]}{{control_type:\"{mode}\",energy:0}} strict")
    state = {"fresh": {}, "pokes": {}, "chest": {}}
    step(WARMUP)
    wrong = []
    compared = 0
    previous_post = [False] * rows
    # Os ticks do rastro a partir do 0: antes de cada um, as operações `step` da linha anterior e as `pre` desta.
    for index, line in enumerate(lines[1:]):
        t = line["t"]
        gap = [op for op in (lines[index]["ops"] if index > 0 else []) if op["phase"] == "step"] + [op for op in line["ops"] if op["phase"] == "pre"]
        for op in gap:
            apply(op["op"], kind, state)
        if kind == "heat" and index > 0:
            # A energia que o gerador produz depende do ambiente (a temperatura do bioma), que o rastro e o jogo não têm igual: a do rastro volta ao tile antes de cada tick,
            # como o `seed` do replay em Rust, e o que se compara é o `redstone`, o `canFunction()` e a atividade.
            # O tanque do gerador do port guarda 1.000 (`TANK_CAPACITY`), e o do rastro recebeu 10.000 por `setContents`, que não confere a capacidade: a lava volta a 1.000 a cada tick
            # enquanto o rastro tem lava (a queima é de 100 por tick, então a atividade não muda).
            for row, saved in enumerate(lines[index]["rows"]):
                if saved is not None and "energy" in saved:
                    lava = min(saved["lava"], 1000)
                    tank = f'{{resource:"minecraft:lava",amount:{lava}L}}' if lava else "{amount:0L}"
                    h.command(f"data merge block {pos(SUBJECT, row)} {{energy:{saved['energy']},machinery_basis:{{v:1,tanks:[{tank}]}}}}")
        step()
        used = list(previous_post)
        pre = [op for op in line["ops"] if op["phase"] == "pre"]
        if pre:
            used = [p if p is not None else False for p in pre[-1]["powered"]]
        # A linha do rastro é gravada depois das operações `step`: a de uma máquina trocada nesse passo (`clear`, `setblock`, `place`) já é a nova, e aqui a leitura é de antes dela.
        swapped = {int(op["op"].split(" ")[1]) for op in line["ops"] if op["phase"] == "step" and op["op"].split(" ")[0] in ("clear", "setblock", "place")}
        if t >= FIRST_COMPARED:
            for row in range(rows):
                if row in swapped:
                    continue
                want, got = line["rows"][row], read_machine(row)
                if want is None or got is None:
                    continue
                compared += 1
                expect = {"redstone": used[row], "active": want["active"]}
                if kind == "chamber":
                    expect["progress"] = want["progress"]
                miss = {key: (got[key], value) for key, value in expect.items() if got[key] != value}
                if miss:
                    wrong.append((t, row, miss))
        previous_post = [bool(r["powered"]) if r else False for r in line["rows"]]
        # as operações `step` desta linha saem no começo da próxima volta
        if t == 0:
            pass
    print(f"{scenario}: {len(lines) - 1} ticks, {compared} leituras, {len(wrong)} diferenças")
    if compared < (len(lines) - 1 - FIRST_COMPARED) * rows // 2:
        print("leituras demais perdidas: o teste não prova nada")
        return 2
    for item in wrong[:20]:
        print("  t=%d linha %d: %s" % item)
    # Limpa: tira as máquinas e as fontes.
    h.command(f"fill {x0} {y0 - 1} {z0 - 2} {x0 + 12} {y0 + 8} {z0 + 4 * rows + 4} minecraft:air")
    return 1 if wrong else 0


if __name__ == "__main__":
    sys.exit(main())
