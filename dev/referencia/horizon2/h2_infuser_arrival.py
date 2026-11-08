#!/usr/bin/env python3
"""Repete no horizon2 o rastro infuser_arrival.jsonl (`ArrivalTraceTest`): o Metallurgic Infuser com o tanque vazio recebendo o item de
entrada e o item que vira infusão, em três ordens (a entrada primeiro, a conversão primeiro e os dois no mesmo tick).

Cada item chega pelo shift-clique do menu (o que o jogador faz), com o servidor parado (`tick freeze` e `tick step 1`), e a cada tick
compara a entrada, o slot de infusão, a saída, o tanque e o progresso com a linha do rastro. A energia é a de uma máquina nova (20,
quatro ticks de receita), posta no NBT antes de cada fase.

Uso: h2_infuser_arrival.py [PAR ...] | full_tank   (par: bio_dirt, fungi_dirt, redstone_dust, carbon_coal, ...; sem argumentos, todos). `H_POS=x,y,z` troca a posição da máquina
(o padrão é a de `h2_identity.POS`, num mundo plano); `H_NAME=horizon` aponta para o container `horizon`.

Armadilhas: `container X Y Z item n` põe no primeiro slot livre, que no Infuser é o de infusão (slot 0), e um `data merge ... Items` com
a lista fora da ordem dos slots não vira itens da máquina. Nenhum dos dois serve para montar uma chegada: use o menu.
"""
import json, re, sys, time

import os

import h2_identity as h

if os.environ.get("H_POS"):
    h.POS = tuple(int(part) for part in os.environ["H_POS"].split(","))
POS = h.POS
# O jogador põe cada item no inventário (slots 9 e 10 do menu) e o shift-clique o leva à máquina.
PLAYER_SLOT = {"input": 9, "conversion": 10}
INVENTORY = {"input": "inventory.0", "conversion": "inventory.1"}
AFTER = 9  # ticks depois da última chegada
ENERGY_TICKS = 4  # a energia de uma máquina nova (20) dá 4 ticks de receita (5 por tick)


def state():
    """O estado da máquina como o rastro o grava: entrada, saída, infusão, tanque (id e quantidade) e progresso."""
    chat = h.command(f"data get block {POS[0]} {POS[1]} {POS[2]}")[0]
    items = {}
    for body in re.findall(r"\{count: (\d+), Slot: (\d+)b, id: \"(?:minecraft|mekanism):(\w+)\"\}", chat):
        items[int(body[1])] = [body[2], int(body[0])]
    tank = re.search(r"amount: (\d+)L, type: \"mekanism:(\w+)\"", chat)
    progress = re.search(r"progress: (\d+)", chat)
    return {
        "input": items.get(1, ["", 0]),
        "output": items.get(2, ["", 0]),
        "infusion": items.get(0, ["", 0]),
        "tank": int(tank.group(1)) if tank else 0,
        "tank_id": tank.group(2) if tank else "",
        "progress": int(progress.group(1)) if progress else 0,
    }


def expected(line):
    return {key: line[key] for key in ("input", "output", "infusion", "tank", "tank_id", "progress")}


def phases_of(lines):
    """Cada fase: da linha de `reset` até a linha antes do próximo `reset`, com as chegadas e o deslocamento delas."""
    starts = [i for i, line in enumerate(lines) if "reset" in line["marks"]]
    phases = []
    for index, start in enumerate(starts):
        end = starts[index + 1] if index + 1 < len(starts) else len(lines)
        block = lines[start:end]
        arrivals = {}
        for offset, line in enumerate(block):
            for mark in line["marks"]:
                parts = mark.split()
                if parts[0] == "arrive":
                    arrivals.setdefault(offset, []).append((parts[1], parts[2], parts[3]))
        pair = next(label for items in arrivals.values() for (label, _, _) in items)
        order = [which for offset in sorted(arrivals) for (_, which, _) in arrivals[offset]]
        same_tick = len(arrivals) == 1
        phases.append({"pair": pair, "lines": block, "arrivals": arrivals, "order": order, "same_tick": same_tick})
    return phases


def item_spec(item):
    namespace = "mekanism" if item in MEKANISM_ITEMS else "minecraft"
    return f"{namespace}:{item}"


MEKANISM_ITEMS = {"bio_fuel", "alloy_infused", "alloy_reinforced", "dust_diamond", "enriched_diamond", "dust_gold", "enriched_gold",
                  "dust_refined_obsidian", "enriched_refined_obsidian", "dust_coal", "enriched_carbon", "dust_iron", "enriched_iron"}


def open_menu():
    for _attempt in range(3):
        h.ctl("interact", *map(str, POS))
        time.sleep(0.4)
        if '"screen":"container"' in h.ctl("screen").replace(" ", ""):
            return True
    return False


def run_phase(phase):
    lines = phase["lines"]
    arrivals = phase["arrivals"]
    h.command("tick freeze")
    h.ctl("close")
    h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
    h.ctl("setblock", *map(str, POS), "mekanism:metallurgic_infuser")
    h.command("tick step 3")
    time.sleep(0.4)
    h.command(f"data merge block {POS[0]} {POS[1]} {POS[2]} {{energy:20}}")
    h.command("tick step 3")
    time.sleep(0.4)
    h.command("clear @s")
    items = {which: item for items in arrivals.values() for (_, which, item) in items}
    for which, item in items.items():
        h.command(f"item replace entity @s {INVENTORY[which]} with {item_spec(item)} 2")
    if not open_menu():
        print("DIFF", phase["pair"], "o menu não abriu")
        return False
    first, last = min(arrivals), max(arrivals)
    same = True
    for offset in range(0, last + AFTER + 1):
        for (_, which, _) in arrivals.get(offset, []):
            h.ctl("click-slot", str(PLAYER_SLOT[which]), "shift")
            time.sleep(0.3)
        if offset >= first and offset < len(lines):
            got, want = state(), expected(lines[offset])
            # A máquina nova guarda energia para 4 ticks de receita (20, a 5 por tick), e o rastro a mantém cheia: depois do quarto
            # tick de progresso só o resto se compara.
            if want["progress"] > ENERGY_TICKS:
                got.pop("progress"), want.pop("progress")
            if got != want:
                same = False
                print(f"  DIFF {phase['pair']} {'+'.join(phase['order'])} tick {offset}: esperado {want} obtido {got}")
        h.command("tick step 1")
        time.sleep(0.12)
    h.ctl("close")
    print(("OK   " if same else "DIFF ") + phase["pair"], "+".join(phase["order"]), "no mesmo tick" if phase["same_tick"] else "")
    return same


def full_tank():
    """`infuser_full_tank.jsonl` (`RestosTraceTest.infuserFullTank`): ferro na entrada, carvão no slot de infusão e o tanque cheio de carbono (1.000), postos por `data merge`
    (sem a regra do slot). O carvão espera até a primeira operação (200 ticks) gastar 10 de carbono; a energia fica cheia a cada tick, como no rastro."""
    lines = [json.loads(l) for l in open(f"{h.TRACES}/infuser_full_tank.jsonl")]
    where = f"{POS[0]} {POS[1]} {POS[2]}"
    h.command("tick freeze")
    h.command(f"setblock {where} minecraft:air")
    h.ctl("setblock", *map(str, POS), "mekanism:metallurgic_infuser")
    h.command("tick step 3")
    time.sleep(0.4)
    h.command(f'data merge block {where} {{energy:20000,Items:[{{Slot:0b,id:"minecraft:coal",count:8}},{{Slot:1b,id:"minecraft:iron_ingot",count:8}}],'
              'chemical_tanks:[{container:0b,stored:{type:"mekanism:carbon",amount:1000L}}]}')
    same = True
    for want in lines[1:]:
        h.command(f"data merge block {where} {{energy:20000}}")
        h.command("tick step 1")
        time.sleep(0.12)
        got = state()
        expected_state = {key: want[key] for key in ("input", "output", "infusion", "tank", "progress")}
        got = {key: got[key] for key in expected_state}
        if got != expected_state:
            same = False
            print(f"  DIFF full_tank t={want['t']}: esperado {expected_state} obtido {got}")
    print(("OK   " if same else "DIFF ") + f"full_tank: {len(lines) - 1} ticks")
    h.command(f"setblock {where} minecraft:air")
    h.command("tick unfreeze")
    sys.exit(0 if same else 1)


def main():
    if sys.argv[1:] == ["full_tank"]:
        full_tank()
    lines = [json.loads(l) for l in open(f"{h.TRACES}/infuser_arrival.jsonl")]
    wanted = set(sys.argv[1:])
    ok = True
    ran = 0
    for phase in phases_of(lines):
        if wanted and phase["pair"] not in wanted:
            continue
        ok &= run_phase(phase)
        ran += 1
    h.command("tick unfreeze")
    print(f"{ran} fases", "TUDO IGUAL" if ok and ran else "HÁ DIFERENÇAS")
    sys.exit(0 if ok and ran else 1)


if __name__ == "__main__":
    main()
