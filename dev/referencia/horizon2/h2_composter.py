#!/usr/bin/env python3
"""Repete no horizon2 o rastro composter_enrichment_chamber.jsonl, tick a tick (`tick freeze` e `tick step 1`).

A Enrichment Chamber em (70,-60,60) ejeta pela face da fase (`mekanism_eject_face`), e a composteira está nesse lado, no nível da fase.
Compara a cada tick o nível da composteira e a saída da máquina com o rastro do original.
"""
import json, re, subprocess, sys, time

import os
TOOLS = os.path.dirname(os.path.abspath(__file__))
TRACE = os.path.join(TOOLS, "..", "rastros", "composter_enrichment_chamber.jsonl")
POS = (70, -60, 60)
DURATIONS = [60, 70, 40, 40, 40, 40, 40, 40]
SIDES = {"below": ((0, -1, 0), "down"), "above": ((0, 1, 0), "up"), "east": ((1, 0, 0), "east")}


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def command(text):
    return json.loads(ctl("command", text))["chat"]


def level_at(pos):
    state = json.loads(ctl("block", *map(str, pos)))
    text = json.dumps(state)
    match = re.search(r"level=(\d+)", text)
    return int(match.group(1)) if match else -1


def output():
    chat = command(f"data get block {POS[0]} {POS[1]} {POS[2]} Items")[0]
    match = re.search(r'count: (\d+), Slot: 1b, id: "minecraft:(\w+)"', chat)
    return [match.group(2), "", "", int(match.group(1))] if match else ["", "", "", 0]


def step():
    command("tick step 1")
    # O passo roda no servidor; dá tempo de o tick acontecer antes de ler.
    time.sleep(0.12)


def main():
    lines = [json.loads(l) for l in open(TRACE)]
    phases = []
    for line in lines:
        if "phase" in line:
            if line["t"] == 0:
                phases.append([])
            phases[-1].append(line)
    command("tick freeze")
    ctl("setblock", *map(str, POS), "mekanism:enrichment_chamber")
    time.sleep(1)
    ok = True
    compared = 0
    for phase, duration in zip(phases, DURATIONS):
        first = phase[0]
        name = first["phase"]
        side, face = SIDES[name.split("_")[0]]
        at = tuple(p + d for p, d in zip(POS, side))
        for around in ((0, -1, 0), (0, 1, 0), (1, 0, 0)):
            pos = tuple(p + d for p, d in zip(POS, around))
            command(f"setblock {pos[0]} {pos[1]} {pos[2]} {'minecraft:dirt' if around == (0, -1, 0) else 'minecraft:air'}")
        command(f"setblock {at[0]} {at[1]} {at[2]} minecraft:composter[level={first['composter']}]")
        out = first["output"]
        command(
            f"data merge block {POS[0]} {POS[1]} {POS[2]} "
            f"{{Items:[{{Slot:1b,id:\"minecraft:{out[0]}\",count:{out[3]}}}],mekanism_eject:1b,mekanism_eject_face:\"{face}\"}}"
        )
        for t in range(0, duration + 1):
            if t > 0:
                step()
            if t >= len(phase):
                continue
            want = phase[t]
            got_level, got_out = level_at(at), output()
            same = got_level == want["composter"] and got_out == want["output"]
            compared += 1
            if not same:
                ok = False
                print(f"DIFF {name} t={t}: esperado nível {want['composter']} saída {want['output']}, obtido nível {got_level} saída {got_out}")
        print(f"fase {name} percorrida")
    command("tick unfreeze")
    print(f"{compared} ticks comparados;", "TUDO IGUAL" if ok else "HÁ DIFERENÇAS")
    sys.exit(0 if ok else 1)


main()
