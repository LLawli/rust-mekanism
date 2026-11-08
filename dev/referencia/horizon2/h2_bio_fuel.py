#!/usr/bin/env python3
"""Repete no container `horizon` (H_NAME=horizon) os rastros do `bio_fuel` do Mekanism original (`RestosTraceTest.furnaceBioFuel` e `blastFurnaceBioFuel`,
`MachineTraceTest.heatGeneratorBioFuelRun`): um forno e um alto-forno do vanilla queimando 4 `bio_fuel` (o relógio aceso, o total da queima, o combustível no slot e o
estado `lit`), e o Heat Generator convertendo `bio_fuel` em lava (a lava e o combustível a cada tick).

Servidor parado (`tick freeze`), um `tick step 1` por linha do rastro, o estado lido do NBT do bloco. O que as marcas do rastro fazem entre dois ticks sai como `data merge`.

Uso: H_NAME=horizon H_POS=x,y,z h2_bio_fuel.py furnace|blast|heat   (H_POS: onde o bloco fica, num lugar com ar e visível)
"""
import json, os, re, sys, time

import h2_identity as h

if os.environ.get("H_POS"):
    h.POS = tuple(int(part) for part in os.environ["H_POS"].split(","))
POS = h.POS
RASTROS = h.TRACES


def pos():
    return f"{POS[0]} {POS[1]} {POS[2]}"


def step():
    h.command("tick step 1")
    time.sleep(0.12)


def lines_of(name):
    return [json.loads(line) for line in open(f"{RASTROS}/{name}.jsonl")]


def furnace_state():
    chat = h.command(f"data get block {pos()}")[0]
    fuel = re.search(r'\{count: (\d+), Slot: 1b, id: "mekanism:bio_fuel"\}', chat)
    lit_time = re.search(r"lit_time_remaining: (\d+)", chat)
    lit_total = re.search(r"lit_total_time: (\d+)", chat)
    block = h.ctl("block", *map(str, POS))
    return {
        "lit_time": int(lit_time.group(1)) if lit_time else 0,
        "lit_total": int(lit_total.group(1)) if lit_total else 0,
        "fuel": int(fuel.group(1)) if fuel else 0,
        "lit": "lit=true" in block,
    }


def furnace(kind, name):
    lines = lines_of(name)
    h.command("tick freeze")
    h.command(f"setblock {pos()} minecraft:air")
    h.command(f"setblock {pos()} minecraft:{kind}")
    h.command("tick step 3")
    time.sleep(0.5)
    h.command(f"item replace block {pos()} container.0 with minecraft:raw_iron 64")
    h.command(f"item replace block {pos()} container.1 with mekanism:bio_fuel 4")
    same = True
    compared = 0
    for want in lines[2:]:
        step()
        got = furnace_state()
        expected = {"lit_time": want["lit_time"], "lit_total": want["lit_total"], "fuel": want["fuel"][1], "lit": want["lit"]}
        compared += 1
        if got != expected:
            same = False
            print(f"  DIFF {name} t={want['t']}: esperado {expected} obtido {got}")
    print(("OK   " if same else "DIFF ") + f"{name}: {compared} ticks")
    h.command(f"setblock {pos()} minecraft:air")
    return same


def heat_state():
    chat = h.command(f"data get block {pos()}")[0]
    lava = re.search(r"amount: (\d+)L", chat)
    fuel = re.search(r'\{count: (\d+), Slot: 0b, id: "mekanism:bio_fuel"\}', chat)
    return {"lava": int(lava.group(1)) if lava else 0, "fuel": int(fuel.group(1)) if fuel else 0}


def heat():
    name = "heat_generator_bio_fuel_run"
    lines = lines_of(name)
    h.command("tick freeze")
    h.command(f"setblock {pos()} minecraft:air")
    h.command(f"setblock {pos()} mekanismgenerators:heat_generator")
    h.command("tick step 3")
    time.sleep(0.5)
    same = True
    previous = lines[0]
    for want in lines[1:]:
        # O ar do mundo de teste não é o do gameTest: a temperatura, e com ela a energia que o gerador produz, andam diferente, e o gerador só queima lava com espaço no
        # buffer. A energia vai para a do rastro antes de cada tick, para o que se mede ser só a conversão do combustível em lava e a queima.
        h.command(f"data merge block {pos()} {{energy:{previous['energy']}}}")
        previous = want
        step()
        for mark in want["marks"]:
            for action in mark.split(", "):
                words = action.split(" ")
                if words[0] == "fuel":
                    h.command(f'data merge block {pos()} {{Items:[{{Slot:0b,id:"mekanism:{words[1]}",count:{words[2]}}}]}}')
                elif words[0] == "lava":
                    amount = int(words[1])
                    tank = f'resource:"minecraft:lava",amount:{amount}L'
                    h.command(f"data merge block {pos()} {{machinery_basis:{{v:1,tanks:[{{{tank}}}]}}}}")
        got = heat_state()
        expected = {"lava": want["lava"], "fuel": want["fuel"][1] if want["fuel"][0] == "bio_fuel" else 0}
        if got != expected:
            same = False
            print(f"  DIFF {name} t={want['t']}: esperado {expected} obtido {got}")
    print(("OK   " if same else "DIFF ") + f"{name}: {len(lines) - 1} ticks")
    h.command(f"setblock {pos()} minecraft:air")
    return same


def main():
    which = sys.argv[1:] or ["furnace", "blast", "heat"]
    ok = True
    for item in which:
        if item == "furnace":
            ok &= furnace("furnace", "furnace_bio_fuel")
        elif item == "blast":
            ok &= furnace("blast_furnace", "blast_furnace_bio_fuel")
        elif item == "heat":
            ok &= heat()
    h.command("tick unfreeze")
    print("TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
