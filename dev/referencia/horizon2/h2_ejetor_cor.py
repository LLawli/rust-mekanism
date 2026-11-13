#!/usr/bin/env python3
"""As cores do ejetor chegando ao transportador, no jogo (frente `fase3-faltas`, Fase 3), contra o rastro do original `ejector_color.jsonl` (`testes/EjectorColorTraceTest.java`).

    H_NAME=horizon h2_ejetor_cor.py [cena ...]     # sem nome, todas as cenas do rastro; cada uma imprime OK ou DIFF e o que cada ponta tem no fim
    NOME=... h2_ejetor_cor.py video                # o vídeo com som da cena `color_blue`

Cada cena monta o que o teste do original monta: um Crusher de origem virado para o leste com `RUN` pedras no slot de saída e a frente em `OUTPUT` ejetando, dois transportadores sem cor, e do
segundo um ramo ao norte e outro ao sul, que levam a um baú ou a um Crusher de destino (entrada estrita e cor de entrada no lado que olha o ramo). O que muda por cena são as cores. A cor
de um transportador entra pelo NBT do `setblock`; a do ejetor e as da entrada, pelo `component_ejector`.
"""
import json, os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_ajustes_jogo as a
import h2_config_lados as c

TOOLS = os.path.dirname(os.path.abspath(__file__))
RASTRO = os.path.join(TOOLS, "..", "rastros", "ejector_color.jsonl")
ITEMS = 20
RUN = int(os.environ.get("RUN", "400"))
CRUSHER = "mekanism:crusher"
TRANSPORTER = "mekanism:basic_logistical_transporter"
# Os nomes de `EnumColor` no NBT: `DARK_BLUE` é `blue`.
NBT_COLOR = {"DARK_BLUE": "blue", "BLACK": "black", "RED": "red"}
ORDINAL = {"BLACK": 0, "DARK_BLUE": 1, "RED": 12}
cmd, wait, at, txt = a.cmd, a.wait, a.at, a.txt
SOURCE, T0, T1, NORTH_T, SOUTH_T, NORTH_END, SOUTH_END = at(1, 0, 3), at(2, 0, 3), at(3, 0, 3), at(3, 0, 2), at(3, 0, 4), at(3, 0, 1), at(3, 0, 5)


def transporter(pos, color):
    nbt = '{color:"%s"}' % NBT_COLOR[color] if color else ""
    cmd("setblock %s %s%s" % (txt(pos), TRANSPORTER, nbt))


def montar(row):
    a.clear()
    cmd("gamemode creative @s")
    cmd("tick freeze")
    transporter(T0, None)
    transporter(T1, None)
    transporter(NORTH_T, row["north_color"])
    if row["south_color"] is not None or row["south"] >= 0:
        transporter(SOUTH_T, row["south_color"])
        cmd("setblock %s minecraft:chest" % txt(SOUTH_END))
    if row["machine"]:
        cmd("setblock %s %s[facing=east]" % (txt(NORTH_END), CRUSHER))
    else:
        cmd("setblock %s minecraft:chest" % txt(NORTH_END))
    cmd("setblock %s %s[facing=east]" % (txt(SOURCE), CRUSHER))
    wait(20)
    time.sleep(0.5)
    ejector = []
    if row["output"]:
        ejector.append('color:"%s"' % NBT_COLOR[row["output"]])
    cmd("data merge block %s {Items:[{Slot:1b,id:\"minecraft:cobblestone\",count:%d}],%s,component_ejector:{%s}}" % (txt(SOURCE), ITEMS, c.config_nbt(item=[4, 0, 0, 0, 0, 0], eject=True), ",".join(ejector)))
    if row["machine"]:
        colors = [-1, ORDINAL[row["input_color"]] if row["input_color"] else -1, -1, -1, -1, -1]
        parts = ["strict_input:1b"] if row["strict"] else []
        if row["input_color"]:
            parts.append("input_color:[I;%s]" % ",".join(map(str, colors)))
        cmd("data merge block %s {%s,component_ejector:{%s}}" % (txt(NORTH_END), c.config_nbt(item=[0, 1, 0, 0, 0, 0]), ",".join(parts)))


def contar():
    def total(pos, slot=None):
        items = c.read_items(pos)
        if slot is None:
            return sum(count for (_, count) in items.values())
        return items.get(slot, ("", 0))[1]
    return {"source": total(SOURCE, 1), "north": total(NORTH_END, 0), "south": total(SOUTH_END, 0)}


def cena(row):
    montar(row)
    cmd("tick unfreeze")
    for _ in range(RUN // 100):
        wait(100)
    got = contar()
    if row["south"] < 0:
        got["south"] = -1
    want = {key: row[key] for key in ("source", "north", "south")}
    ok = got == want
    print(("OK   " if ok else "DIFF ") + row["case"], "jogo", got, "original", want)
    return ok


def main():
    names = [n for n in sys.argv[1:] if n != "video"]
    rows = [json.loads(line) for line in open(RASTRO)]
    rows = [row for row in rows if "case" in row and (not names or row["case"] in names)]
    ok = True
    for row in rows:
        ok &= cena(row)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()
