#!/usr/bin/env python3
"""Repete em jogo os comandos de vários blocos de `TransmitterEventsTraceTest.ops` (frente `eventos-meio`, decisão 183 (b)) e compara, operação por operação, a lista de eventos que
a rede de transmissores recebe com `dev/referencia/rastros/transmitter_events_ops.jsonl` (o original manda cada evento na hora, de dentro da chamada que o causa).

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_eventos_meio.py [ops|chunks_mixed|all]

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


M32 = 0xFFFFFFFF


def java_hash(x, y, z):
    """`Vec3i.hashCode`."""
    return ((y + z * 31) * 31 + x) & M32


def mix(h):
    """`HashCommon.mix`."""
    h = (h * 0x9E3779B9) & M32
    return h ^ (h >> 16)


class OpenMap:
    """O `Object2ObjectOpenHashMap` do fastutil (`ChunkAccess.blockEntities`): só a ordem das chaves (`put`, `remove` com `shiftKeys`, o aumento da tabela e a iteração do fim para o começo)."""

    def __init__(self):
        self.n, self.size = 32, 0
        self.keys = [None] * self.n
        self.max_fill = 24

    def slot(self, key):
        return mix(java_hash(*key)) & (self.n - 1)

    def put(self, key):
        m, p = self.n - 1, self.slot(key)
        while self.keys[p] is not None:
            if self.keys[p] == key:
                return
            p = (p + 1) & m
        self.keys[p] = key
        self.size += 1
        if self.size > self.max_fill:
            self.rehash(self.n * 2)

    def rehash(self, n):
        old = self.keys
        self.n, self.keys, self.max_fill = n, [None] * n, int(n * 0.75)
        for key in reversed(old):
            if key is None:
                continue
            p = mix(java_hash(*key)) & (n - 1)
            while self.keys[p] is not None:
                p = (p + 1) & (n - 1)
            self.keys[p] = key

    def remove(self, key):
        m, pos = self.n - 1, self.slot(key)
        while self.keys[pos] is not None and self.keys[pos] != key:
            pos = (pos + 1) & m
        if self.keys[pos] is None:
            return
        self.size -= 1
        while True:
            last, pos = pos, (pos + 1) & m
            while True:
                if self.keys[pos] is None:
                    self.keys[last] = None
                    return
                slot = self.slot(self.keys[pos])
                stays = (last >= slot or slot > pos) if last <= pos else (last >= slot and slot > pos)
                if stays:
                    break
                pos = (pos + 1) & m
            self.keys[last] = self.keys[pos]

    def order(self):
        return [k for k in reversed(self.keys) if k is not None]


def mixed_positions(from_x, count, step, chest_every):
    """`TransmitterEventsTraceTest.mixed`: as posições na ordem em que são postas (z de 0 a 2 da estrutura) e se cada uma é um baú."""
    out = []
    for i in range(count):
        k = (i * step) % 48
        out.append(((from_x + k % 16, g.BASE[1], g.BASE[2] + k // 16), i % chest_every == chest_every - 1))
    return out


def chunks_mixed():
    """Dois chunks com cabos e baús postos fora de ordem (e três tirados e dois postos de novo no segundo), que saem com o jogador longe: a ordem do `onChunkUnloaded` de cada cabo é a do
    mapa de todos os block entities do chunk (`transmitter_events_chunks_mixed.jsonl`: o Java, que o modelo `OpenMap` repete)."""
    g.prepare(width=60, depth=4)
    first = mixed_positions(16, 14, 7, 4)
    second = mixed_positions(32, 36, 7, 5)
    for pos, chest in first + second:
        g.chat(f"setblock {g.text(pos)} {'minecraft:chest' if chest else CABLE}")
    at = lambda k: (32 + k % 16, g.BASE[1], g.BASE[2] + k // 16)
    for k in (14, 21, 35):
        g.chat(f"setblock {g.text(at(k))} minecraft:air")
    for k in (35, 14):
        g.chat(f"setblock {g.text(at(k))} {CABLE}")
    g.wait_ticks(10)
    expected = {}
    for chunk, placed, removed, back in ((1, first, [], []), (2, second, [at(k) for k in (14, 21, 35)], [at(k) for k in (35, 14)])):
        model = OpenMap()
        for pos, _ in placed:
            model.put(pos)
        for pos in removed:
            model.remove(pos)
        for pos in back:
            model.put(pos)
        chests = {pos for pos, chest in placed if chest}
        expected[chunk] = [list(p) for p in model.order() if p not in chests]
    before = len(g.log_lines())
    x, y, z = g.BASE
    g.chat(f"tp @s {-20 * 16 + 8} {y} {z + 8}")
    g.wait_ticks(80)
    events = g.log_lines()[before:]
    wrong = 0
    for chunk in (1, 2):
        got = [e["p"] for e in events if e["e"] == "onChunkUnloaded" and e["p"][0] >> 4 == chunk and e["p"][1] == y]
        removed = [e["p"] for e in events if e["e"] == "setRemoved" and e["p"][0] >> 4 == chunk and e["p"][1] == y]
        ok = got == expected[chunk] and removed == expected[chunk]
        wrong += not ok
        print(f"[{'igual' if ok else 'DIFERENTE'}] chunk {chunk}: {len(got)} cabos descarregados, {len(removed)} removidos, esperado {len(expected[chunk])}")
        if not ok:
            print("   java (modelo):", expected[chunk])
            print("   unloaded:", got)
            print("   removed :", removed)
    return wrong


def ops():
    g.prepare()
    def scene():
        for rel, block in (((3, 1, 3), "minecraft:stone"), ((3, 2, 3), "minecraft:torch"), ((3, 2, 2), CABLE)):
            g.chat(f"setblock {g.text(g.abs_pos(rel))} {block}")

    steps = [
        ("fill the line of cables", fill((1, 1, 2), (5, 1, 2), CABLE)),
        ("fill the row of machines", fill((1, 1, 1), (5, 1, 1), CHAMBER)),
        ("fill the middle of the line with air", fill((2, 1, 2), (3, 1, 2), "minecraft:air")),
        ("fill the gap with cables again", fill((2, 1, 2), (3, 1, 2), CABLE)),
        ("build a cable, a stone and a torch on it", scene),
        ("break the stone under the torch next to the cable", setblock((3, 1, 3), "minecraft:air")),
        ("clone the line to z 3", clone((1, 1, 2), (5, 1, 2), (1, 1, 3))),
        ("clone the line to z 3 shifted by one with move", clone((1, 1, 3), (5, 1, 3), (2, 1, 3), "replace move")),
        ("setblock stone next to the line", setblock((0, 1, 2), "minecraft:stone")),
        ("fill advanced cables over the line", fill((1, 1, 2), (5, 1, 2), f"{ADVANCED} replace {CABLE}")),
        ("fill the machines with air", fill((1, 1, 1), (5, 1, 1), "minecraft:air")),
        ("explosion in the middle of the line", explode()),
    ]
    segments = g.run_segments(steps)
    return g.compare("ops", segments)


if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    wrong = 0
    if which in ("ops", "all"):
        wrong += ops()
    if which in ("chunks_mixed", "all"):
        wrong += chunks_mixed()
    sys.exit(1 if wrong else 0)
