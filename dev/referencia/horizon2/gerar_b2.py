#!/usr/bin/env python3
"""Gera o conteudo de teste `cubob` (opcao B2 da decisao 66): propriedades de estado por lado (porta e LED aceso) e blockstate multipart.
Uso: gerar_b2.py DESTINO   (as pecas vem de `PORTS` e `LEDS` de mods/mekanism/native/energy_cube.rs)"""
import json, os, re, sys, itertools
ROOT = "/mnt/dados/builds/rust-mekanism-ws/cubo-forma"
src = open(f"{ROOT}/mods/mekanism/native/energy_cube.rs").read()
dest = sys.argv[1]
SIDES = ["front", "left", "right", "back", "top", "bottom"]
DIRS = ["down", "up", "north", "south", "west", "east"]
FACING = {"north": {}, "east": {"y": 90}, "south": {"y": 180}, "west": {"y": 270}, "up": {"x": 270}, "down": {"x": 90}}

def table(name):
    body = src.split(f"const {name}: [&[Part]; 6] = [")[1].split("\n];")[0]
    groups = body.split("    &[")[1:]
    out = []
    for g in groups:
        parts = []
        for line in g.splitlines():
            if "Part {" not in line:
                continue
            fr = re.search(r"from: \[([^\]]*)\]", line).group(1)
            to = re.search(r"to: \[([^\]]*)\]", line).group(1)
            faces_txt = re.search(r"faces: \[(.*)\], emissive: (true|false)", line)
            faces = re.findall(r"Some\(\((\d), \[([^\]]*)\]\)\)|None", faces_txt.group(1))
            fs = []
            for sprite, uv in faces:
                fs.append(None if sprite == "" else (int(sprite), [float(x) for x in uv.split(",")]))
            parts.append(([float(x) for x in fr.split(",")], [float(x) for x in to.split(",")], fs, faces_txt.group(2) == "true"))
        out.append(parts)
    assert len(out) == 6, (name, len(out))
    return out

PORTS, LEDS = table("PORTS"), table("LEDS")

def model(parts, lit):
    elements = []
    for i, (fr, to, faces, emissive) in enumerate(parts):
        if lit:
            fr = [v - 0.02 if False else v for v in fr]
        fd = {}
        for d, f in zip(DIRS, faces):
            if f is None:
                continue
            sprite, uv = f
            if lit:
                uv = [uv[0] - 2.0, uv[1], uv[2] - 2.0, uv[3]]
            face = {"uv": uv, "texture": f"#{sprite + 1}"}
            if emissive or lit:
                face["light_emission"] = 15
            fd[d] = face
        elements.append({"from": fr, "to": to, "faces": fd})
    return {"parent": "minecraft:block/block",
            "textures": {"1": "mekanism:block/models/energy_cube", "2": "mekanism:block/models/ports",
                         "3": "mekanism:block/models/ports_led", "4": "mekanism:block/models/leds",
                         "particle": "mekanism:block/energy_cube_particle"},
            "elements": elements}

def w(path, obj):
    p = os.path.join(dest, path)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    json.dump(obj, open(p, "w"), indent=None, separators=(",", ":"))

open_toml = os.path.join(dest, "horizon-mod.toml")
os.makedirs(dest, exist_ok=True)
open(open_toml, "w").write('id = "cubob"\nname = "Cubo B2"\nversion = "0.0.1"\ndescription = "Prototipo da opcao B2 (decisao 66)."\nauthors = ["teste"]\napi = "0.2"\nside = "both"\ncapabilities = ["registries"]\n')
for i, s in enumerate(SIDES):
    w(f"assets/cubob/models/block/port_{s}.json", model(PORTS[i], False))
    w(f"assets/cubob/models/block/lit_{s}.json", model(LEDS[i], True))
parts = []
for f, rot in FACING.items():
    parts.append({"when": {"facing": f}, "apply": {"model": "mekanism:block/energy_cube/basic_static", **rot}})
    for s in SIDES:
        parts.append({"when": {"facing": f, f"port_{s}": "true"}, "apply": {"model": f"cubob:block/port_{s}", **rot}})
        parts.append({"when": {"facing": f, f"lit_{s}": "true"}, "apply": {"model": f"cubob:block/lit_{s}", **rot}})
w("assets/cubob/blockstates/cube.json", {"multipart": parts})
base = json.load(open(f"{ROOT}/mods/mekanism/content/data/mekanism/horizon/block/basic_energy_cube.json"))
props = {"facing": {"type": "enum", "values": list(FACING)}}
default = {"facing": "north"}
for pre in ("port", "lit"):
    for s in SIDES:
        props[f"{pre}_{s}"] = {"type": "boolean"}
        default[f"{pre}_{s}"] = "false"
names = ["facing"] + [f"port_{s}" for s in SIDES]
values = {}
for combo in itertools.product(list(FACING), *[["false", "true"]] * 6):
    values[",".join(combo)] = base["shapes"]["values"][combo[0] if combo[0] in base["shapes"]["values"] else "north"]
block = {"template": "minecraft:lodestone",
         "properties": {"hardness": 2.0, "requires_tool": True, "map_color": 59, "sounds": "minecraft:stone", "explosion_resistance": 2.4, "occludes": False},
         "state_properties": props, "default_state": default,
         "shapes": {"properties": names, "values": values}}
w("data/cubob/horizon/block/cube.json", block)
print("estados", 6 * 2 ** 12, "chaves de forma", len(values))
