#!/usr/bin/env python3
"""Escreve `cenas/cabo_modelo.json`, o roteiro das partes do modelo dos transmissores (frente `cabo-modelo`, Fase 4): o original (`testes/CenaBlocos.java`, `oficial_blocos.sh`) e o port
(`horizon2/h2_cabo_modelo.py`) rodam o mesmo roteiro e tiram os mesmos prints, que `comparar_blocos.py` confere.

Seis tiros com os 22 transmissores, cada um no meio de um trio: um da mesma família no oeste (o braço `NORMAL`, e um cabo com um braço só), um vizinho que aceita no leste (`PUSH`) e outro no sul
(`PULL`), e o norte, o alto e o baixo sem conexão (`NONE`, de frente para a câmera); e os trechos retos de cada família em x, em z e em y, mais um cabo e um cano sozinhos, em L e em T. Os
tipos crus `PUSH` e `PULL` entram pelo comando `#conexao X Y Z lado=tipo`: o original os põe por `Transmitter.setConnectionTypeRaw`, o port pelo clique do Configurator."""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
Y = -60
Z = 82
FAMILIES = {
    "universal_cable": "mekanism:enrichment_chamber",
    "pressurized_tube": "mekanism:metallurgic_infuser[facing=north]",
    "thermodynamic_conductor": "mekanismgenerators:heat_generator[facing=north]",
    "mechanical_pipe": "minecraft:water_cauldron[level=3]",
    "logistical_transporter": "minecraft:chest",
}
TIERS = ("basic", "advanced", "elite", "ultimate")
TAIL = ["restrictive_transporter", "diversion_transporter"]
# O acceptor de cada transmissor (o do original: a capability do vizinho: energia, químico, calor, fluido e item).
ACCEPTOR = {**{f"{t}_{k}": a for k, a in FAMILIES.items() for t in TIERS}, **{name: "minecraft:chest" for name in TAIL}}
STRAIGHT = ["basic_universal_cable", "basic_pressurized_tube", "basic_thermodynamic_conductor", "basic_mechanical_pipe", "basic_logistical_transporter", "restrictive_transporter", "diversion_transporter"]


def put(x, y, z, block):
    return f"setblock {x} {y} {z} {block}"


def trios(name, blocks):
    """Quatro trios por tiro, a quatro blocos um do outro: `W` (transmissor), `T` (o do tiro), `E` (aceitador) e `S` (aceitador)."""
    cmds, tracked = [], []
    for k, block in enumerate(blocks):
        x = 60 + 4 * k
        accept = ACCEPTOR[block]
        cmds += [put(x, Y, Z, f"mekanism:{block}"), put(x + 1, Y, Z, f"mekanism:{block}"), put(x + 2, Y, Z, accept), put(x + 1, Y, Z + 1, accept)]
        tracked.append((x + 1, Y, Z))
    cmds.append("#esperar 20")
    for x, y, z in tracked:
        cmds.append(f"#conexao {x} {y} {z} east=push south=pull")
    cmds.append("#esperar 20")
    return {"name": name, "cmds": cmds, "tp": "66.0 -56.6 75.5 0 28"}


def run(x, y, z, axis, block, n=4):
    dx, dy, dz = {"x": (1, 0, 0), "y": (0, 1, 0), "z": (0, 0, 1)}[axis]
    return [put(x + dx * i, y + dy * i, z + dz * i, f"mekanism:{block}") for i in range(n)]


def straights(axis):
    cmds = []
    for k, block in enumerate(STRAIGHT):
        if axis == "x":
            cmds += run(60, Y, 78 + 2 * k, "x", block)
        elif axis == "z":
            cmds += run(59 + 2 * k, Y, 79, "z", block)
        else:
            cmds += run(59 + 2 * k, Y, 82, "y", block)
    return cmds


def main():
    spec = json.load(open(os.path.join(HERE, "cenas/blocos.json")))
    shots = []
    families = [[f"{t}_{k}" for t in TIERS] for k in FAMILIES]
    for names, blocks in zip(("cabos", "tubos", "condutores", "canos", "transportadores"), families):
        shots.append(trios(f"cabo_modelo_{names}", blocks))
    shots.append(trios("cabo_modelo_restritivo_e_desviador", TAIL))
    shots.append({"name": "cabo_modelo_retos_x", "cmds": straights("x"), "tp": "62.0 -50.0 75.0 0 50"})
    shots.append({"name": "cabo_modelo_retos_z", "cmds": straights("z"), "tp": "65.0 -52.0 75.0 0 40"})
    shots.append({"name": "cabo_modelo_retos_y", "cmds": straights("y"), "tp": "65.0 -58.0 74.0 0 5"})
    alone = []
    for k, block in enumerate(("basic_universal_cable", "basic_mechanical_pipe")):
        x = 60 + 6 * k
        alone += [put(x, Y, Z, f"mekanism:{block}"),  # sozinho: o centro inteiro, `NONE` nas seis faces
                  put(x + 2, Y, Z, f"mekanism:{block}"), put(x + 3, Y, Z, f"mekanism:{block}"), put(x + 3, Y, Z + 1, f"mekanism:{block}"),  # L
                  put(x + 5, Y, Z + 3, f"mekanism:{block}"), put(x + 4, Y, Z + 3, f"mekanism:{block}"), put(x + 6, Y, Z + 3, f"mekanism:{block}"), put(x + 5, Y, Z + 4, f"mekanism:{block}")]  # T
    shots.append({"name": "cabo_modelo_isolados", "cmds": alone, "tp": "66.0 -54.0 77.0 0 35"})
    out = {"setup": spec["setup"], "clear": spec["clear"], "shots": shots}
    with open(os.path.join(HERE, "cenas/cabo_modelo.json"), "w") as handle:
        json.dump(out, handle, indent=1)
        handle.write("\n")
    print(len(shots), "tiros")


if __name__ == "__main__":
    main()
