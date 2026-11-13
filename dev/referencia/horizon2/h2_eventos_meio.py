#!/usr/bin/env python3
"""Repete em jogo os comandos de vários blocos de `TransmitterEventsTraceTest.ops` (frente `eventos-meio`, decisão 183 (b)) e compara, operação por operação, a lista de eventos que
a rede de transmissores recebe com `dev/referencia/rastros/transmitter_events_ops.jsonl` (o original manda cada evento na hora, de dentro da chamada que o causa).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_eventos_meio.py [ops]

Usa as funções de `h2_rede_ganchos.py` (a sonda `MBT` do stderr, `TRANSMITTER_PROBE = true` na cópia de teste). A explosão não tem sorteio igual nos dois jogos: só se mostra.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_rede_ganchos as g

CABLE = g.CABLE
ADVANCED = "mekanism:advanced_universal_cable"
CHAMBER = g.CHAMBER


def fill(a, b, block):
    return lambda: g.chat(f"fill {g.text(g.abs_pos(a))} {g.text(g.abs_pos(b))} {block}")


def clone(a, b, to, mode=""):
    return lambda: g.chat(f"clone {g.text(g.abs_pos(a))} {g.text(g.abs_pos(b))} {g.text(g.abs_pos(to))} {mode}".strip())


def setblock(pos, block):
    return lambda: g.chat(f"setblock {g.text(g.abs_pos(pos))} {block}")


def explode():
    x, y, z = g.abs_pos((3, 1, 2))
    return lambda: g.chat(f"summon minecraft:tnt {x}.5 {y}.5 {z}.5 {{fuse:1}}")


def ops():
    g.prepare()
    steps = [
        ("fill the line of cables", fill((1, 1, 2), (5, 1, 2), CABLE)),
        ("fill the row of machines", fill((1, 1, 1), (5, 1, 1), CHAMBER)),
        ("fill the middle of the line with air", fill((2, 1, 2), (3, 1, 2), "minecraft:air")),
        ("fill the gap with cables again", fill((2, 1, 2), (3, 1, 2), CABLE)),
        ("fill advanced cables over the line", fill((1, 1, 2), (5, 1, 2), f"{ADVANCED} replace {CABLE}")),
        ("clone the line to z 3", clone((1, 1, 2), (5, 1, 2), (1, 1, 3))),
        ("clone the line to z 3 shifted by one with move", clone((1, 1, 3), (5, 1, 3), (2, 1, 3), "replace move")),
        ("setblock stone next to the line", setblock((0, 1, 2), "minecraft:stone")),
        ("fill the machines with air", fill((1, 1, 1), (5, 1, 1), "minecraft:air")),
        ("explosion in the middle of the line", explode()),
    ]
    segments = g.run_segments(steps)
    return g.compare("ops", segments)


if __name__ == "__main__":
    sys.exit(ops())
