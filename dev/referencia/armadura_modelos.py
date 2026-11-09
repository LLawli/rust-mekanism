#!/usr/bin/env python3
"""Gera os modelos avulsos do MekaSuit a partir do `mekasuit.obj` do conteúdo (frente `armadura-vestida`).

O original monta, a cada combinação de peças vestidas, os pedaços do OBJ que cada peça desenha (`MekaSuitArmor.createQuads`: o filtro da peça, a posição do corpo pelo nome, o
brilho e o vidro pelo nome, as peças `excl_` e `shared_`) e os assa na hora com a visibilidade de cada objeto (`OBJModelData.getParts`). O Horizon registra os modelos avulsos uma vez,
no começo, então cada grupo que o original pode pedir vira um modelo: `models/entity/mekasuit/<peça>_<posição>_<classe>[_<etiqueta>].json`, filho de `mekanism:entity/mekasuit`, com
a visibilidade de todos os objetos (o Horizon mostra o que a visibilidade não cita, o NeoForge também; o original cita todos).

Uso: armadura_modelos.py   (sem argumentos: lê e escreve em `mods/mekanism/content`)
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
CONTENT = os.path.join(ROOT, "mods/mekanism/content/assets/mekanism")
OBJ = os.path.join(CONTENT, "models/entity/mekasuit.obj")
OUT = os.path.join(CONTENT, "models/entity/mekasuit")

# `ModelPos.get`: o primeiro da ordem do enum cujo teste casa o nome.
POSITIONS = ["head", "body", "left_arm", "right_arm", "left_leg", "right_leg"]
# `checkEquipment`: a palavra do nome de cada peça.
PIECES = [("head", "helmet"), ("chest", "chest"), ("legs", "leggings"), ("feet", "boots")]


def objects():
    names = []
    for line in open(OBJ):
        words = line.split()
        if words and words[0] == "o":
            names.append(words[1])
    return names


def position(name):
    lowered = name.lower()
    for candidate in POSITIONS:
        if candidate in lowered:
            return candidate
    raise SystemExit(f"parte sem posição: {name}")


def classify(name):
    # `parseTransparency`: o vidro antes do brilho antes do resto.
    if "glass" in name:
        return "translucent"
    if "_led" in name:
        return "emissive"
    return "opaque"


def tag(name):
    if name.startswith("excl_"):
        return "excl"
    if name.startswith("shared_"):
        return "shared"
    return ""


def groups(names):
    found = {}
    for name in names:
        for piece, word in PIECES:
            if word in name:
                key = (piece, position(name), classify(name), tag(name))
                found.setdefault(key, []).append(name)
    return found


def model_name(key):
    piece, pos, cls, label = key
    return "_".join(part for part in (piece, pos, cls, label) if part)


def main():
    names = objects()
    os.makedirs(OUT, exist_ok=True)
    written = set()
    for key, shown in sorted(groups(names).items()):
        visibility = {name: name in shown for name in sorted(names)}
        path = os.path.join(OUT, f"{model_name(key)}.json")
        with open(path, "w") as file:
            json.dump({"parent": "mekanism:entity/mekasuit", "visibility": visibility}, file, indent=2)
            file.write("\n")
        written.add(os.path.basename(path))
    for stale in os.listdir(OUT):
        if stale not in written:
            os.remove(os.path.join(OUT, stale))
    print(f"{len(written)} modelos em {OUT}")


if __name__ == "__main__":
    main()
