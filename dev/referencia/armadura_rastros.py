#!/usr/bin/env python3
"""Gera os rastros da frente `armadura-vestida` (o desenho vestido do MekaSuit, do jetpack, das botas e do tanque) a partir do código e dos assets do original.

Uso: armadura_rastros.py [CHECKOUT_DO_MEKANISM] [CHECKOUT_DO_MINECRAFT_FONTES]

O que sai, em `dev/referencia/rastros/`:
- `armor_assets.json`: o hash (FNV-1a 64 e tamanho) de cada arquivo de `mods/mekanism/content` que o port copia do original para o desenho vestido (o modelo OBJ do
  MekaSuit, o `.mtl`, as texturas), para `tests/worn_gear.rs` conferir byte a byte;
- `armor_gear.json`: os números do código de cliente do original, lidos do texto do Java: `MekaSuitArmor` (a translação de base, o deslocamento de cada `ModelPos`, a
  translação do bebê, os pares de peças, a ordem de `EquipmentSlot`, o filtro de cada peça), `ChestArmor` (o jetpack e o tanque), `FreeRunnerArmor` (as botas), a rotação
  do estado do modelo OBJ (`OBJModelData`) e as partes do OBJ (nome, material e quantidade de quads de cada objeto).
O Rust não lê o checkout do original: lê estes arquivos. Cada número sai de uma expressão regular que tem de achar exatamente o que espera: se o Java mudar, o script para.
"""
import json
import os
import re
import sys

ORIGINAL = sys.argv[1] if len(sys.argv) > 1 else "/mnt/dados/caches/rust-mekanism-ref/mekanism"
MINECRAFT = sys.argv[2] if len(sys.argv) > 2 else "/mnt/dados/caches/rust-mekanism-ref/mc-sources/mc"
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
CONTENT = os.path.join(ROOT, "mods/mekanism/content")
OUT = os.path.join(HERE, "rastros")
RES = f"{ORIGINAL}/src/main/resources/assets/mekanism"
ARMOR = f"{ORIGINAL}/src/main/java/mekanism/client/render/armor"
MODEL = f"{ORIGINAL}/src/main/java/mekanism/client/model"

# Os arquivos que o port copia do original, do `assets/mekanism` dele para o do conteúdo.
COPIED = [
    "models/entity/mekasuit.json",
    "models/entity/mekasuit.obj",
    "models/entity/mekasuit.mtl",
    "textures/entity/armor/mekasuit_armor_body.png",
    "textures/entity/armor/mekasuit_armor_exoskeleton.png",
    "textures/entity/armor/mekasuit_armor_helmet.png",
    "textures/entity/armor/mekasuit_player.png",
]


def fnv(data: bytes) -> str:
    h = 0xCBF29CE484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}:{len(data)}"


def canonical(text: str) -> bytes:
    return json.dumps(json.loads(text), sort_keys=True, separators=(",", ":"), ensure_ascii=False).encode()


def digest(path: str) -> str:
    data = open(path, "rb").read()
    return fnv(canonical(data.decode())) if path.endswith(".json") else fnv(data)


def java_number(text: str) -> float:
    return float(text.rstrip("Ff"))


def exactly_one(pattern: str, text: str, what: str, flags=0):
    found = re.findall(pattern, text, flags)
    if len(found) != 1:
        sys.exit(f"{what}: esperava 1 ocorrência de {pattern!r}, achei {len(found)}")
    return found[0]


def vector(text: str):
    return [java_number(part) for part in text.split(",")]


def mekasuit():
    text = open(f"{ARMOR}/MekaSuitArmor.java").read()
    base = vector(exactly_one(r"BASE_TRANSLATION = new Vector3f\(([^)]*)\)", text, "BASE_TRANSLATION"))
    positions = []
    for match in re.finditer(r"^\s+(HEAD|BODY|LEFT_ARM|RIGHT_ARM|LEFT_LEG|RIGHT_LEG|LEFT_WING|RIGHT_WING)\(BASE_TRANSLATION(?:\.add\(([^)]*?), new Vector3f\(\)\))?, s -> s\.contains\(\"([a-z_]+)\"\)\)[,;]", text, re.M):
        name, add, spec = match.groups()
        positions.append({"name": name, "add": vector(add) if add else [0.0, 0.0, 0.0], "contains": spec})
    if [p["name"] for p in positions] != ["HEAD", "BODY", "LEFT_ARM", "RIGHT_ARM", "LEFT_LEG", "RIGHT_LEG", "LEFT_WING", "RIGHT_WING"]:
        sys.exit(f"ModelPos: {positions}")
    # `scaleBaby`: a escala (todos menos a cabeça) e a translação de cada posição.
    baby_block = exactly_one(r"scaleBaby\(PoseStack poseStack, STATE state\) \{(.*?)\n        \}\n\n        public void translateModel", text, "scaleBaby", re.S)
    scales_all_but_head = "if (this != HEAD) {\n                poseStack.scale(state.ageScale, state.ageScale, state.ageScale);" in baby_block
    baby = {}
    for names, a, b, c in re.findall(r"case ([A-Z_, ]+) -> poseStack\.translate\(([-0-9.F]+), ([-0-9.F]+), ([-0-9.F]+)\);", baby_block):
        for name in names.split(", "):
            baby[name] = [java_number(a), java_number(b), java_number(c)]
    pairs = {}
    for name, slot, adjacent in re.findall(r"public static final MekaSuitArmor (\w+) = new MekaSuitArmor\(EquipmentSlot\.(\w+), EquipmentSlot\.(\w+)\)", text):
        pairs[name] = {"slot": slot, "adjacent": adjacent}
    check = {}
    for slot, word in re.findall(r"case (HEAD|CHEST|LEGS|FEET) -> text\.contains\(\"(\w+)\"\);", text):
        check[slot] = word
    tags = {
        "inactive": exactly_one(r'INACTIVE_TAG = "(\w+)"', text, "INACTIVE_TAG"),
        "exclusive": exactly_one(r'EXCLUSIVE_TAG = "(\w+)"', text, "EXCLUSIVE_TAG"),
        "shared": exactly_one(r'SHARED_TAG = "(\w+)"', text, "SHARED_TAG"),
        "glass": exactly_one(r'GLASS_TAG = "(\w+)"', text, "GLASS_TAG"),
        "emissive": exactly_one(r'EMISSIVE_TAG = "(\w+)"', text, "EMISSIVE_TAG"),
    }
    # A regra de `createQuads` que o port reproduz, conferida no texto: a peça exclusiva some com a vizinha vestida; a compartilhada some quando a vizinha está vestida e
    # vem depois na ordem de `EquipmentSlot`.
    for needle in (
        "} else if (name.startsWith(EXCLUSIVE_TAG)) {\n                if (wornParts.contains(adjacentType)) {",
        "} else if (name.startsWith(SHARED_TAG) && wornParts.contains(adjacentType) && adjacentType.ordinal() > type.ordinal()) {",
        "if (quad.contains(GLASS_TAG)) {\n                    translucentQuads.add(quad);\n                } else if (quad.contains(EMISSIVE_TAG)) {",
        "boolean isBaby = state.isBaby && state.entityType != EntityTypes.ARMOR_STAND;",
    ):
        if needle not in text and needle not in open(f"{ARMOR}/ICustomArmor.java").read():
            sys.exit(f"MekaSuitArmor mudou: {needle[:60]!r}")
    return {
        "base_translation": base,
        "positions": positions,
        "baby_scales_all_but_head": scales_all_but_head,
        "baby_translation": baby,
        "instances": pairs,
        "equipment_check": check,
        "tags": tags,
    }


def chest():
    text = open(f"{ARMOR}/ChestArmor.java").read()
    body = exactly_one(r"baseModel\.body\.translateAndRotate\(poseStack\);(.*?)bodyItem\.submit", text, "ChestArmor", re.S)
    numbers = re.findall(r"poseStack\.(translate|scale)\(([^;]*)\);", body)
    # A ordem do texto: a escala do bebê, a translação do bebê do jetpack, a do adulto (jetpack, tanque), depois as do jetpack e as do tanque.
    expected = [
        ("scale", "state.ageScale, state.ageScale, state.ageScale"),
        ("translate", "0, -0.05, 0"),
        ("translate", "0, 0.3, 0"),
        ("translate", "0, 0.41, 0"),
        ("translate", "0, 0, 0.145"),
        ("scale", "1.02F, -1.02F, -1.02F"),
        ("translate", "0, 0, 0.28"),
        ("scale", "1, -1, -1"),
    ]
    if numbers != expected:
        sys.exit(f"ChestArmor mudou: {numbers}")
    return {
        "jetpack": {"baby": [0.0, -0.05, 0.0], "adult": [0.0, 0.3, 0.0], "translate": [0.0, 0.0, 0.145], "scale": [1.02, -1.02, -1.02]},
        "scuba_tank": {"baby": None, "adult": [0.0, 0.41, 0.0], "translate": [0.0, 0.0, 0.28], "scale": [1.0, -1.0, -1.0]},
    }


def free_runners():
    text = open(f"{ARMOR}/FreeRunnerArmor.java").read()
    apply = exactly_one(r"private void applyTransforms\(.*?\{(.*?)\n    \}", text, "applyTransforms", re.S)
    numbers = re.findall(r"poseStack\.(translate|scale)\(([^;]*)\);", apply)
    expected = [
        ("scale", "state.ageScale, state.ageScale, state.ageScale"),
        ("translate", "0, -0.2, 0"),
        ("translate", "xo, 1, 0.525"),
        ("scale", "1.02F, -1.02F, -1.02F"),
    ]
    if numbers != expected:
        sys.exit(f"FreeRunnerArmor mudou: {numbers}")
    xo = {
        "left": java_number(exactly_one(r"applyTransforms\(poseStack, baseModel\.leftLeg, isBaby, state, ([-0-9.]+)\)", text, "xo esquerdo")),
        "right": java_number(exactly_one(r"applyTransforms\(poseStack, baseModel\.rightLeg, isBaby, state, ([-0-9.]+)\)", text, "xo direito")),
    }
    models = re.findall(r"FreeRunnerArmor\((MekanismModelCache\.INSTANCE\.\w+), (MekanismModelCache\.INSTANCE\.\w+)\)", text)
    return {
        "baby": {"scale_age": True, "translate": [0.0, -0.2, 0.0]},
        "xo": xo,
        "translate_z": 0.525,
        "translate_y": 1.0,
        "scale": [1.02, -1.02, -1.02],
        "models": [list(pair) for pair in models],
    }


def registration():
    """`ClientRegistration.registerRenderStateModifiers`: as partes do vanilla que cada peça esconde e a capa."""
    text = open(f"{ORIGINAL}/src/main/java/mekanism/client/ClientRegistration.java").read()
    body = exactly_one(r"public static void registerRenderStateModifiers\(.*?\n    \}\n", text, "registerRenderStateModifiers", re.S)
    # Cada ramo vai do `if` que olha a peça até o próximo `if` do mesmo nível (quatro espaços a mais que o do método).
    head = exactly_one(r"if \(state\.headEquipment\.getItem\(\) instanceof ItemMekaSuitArmor\) \{(.*?)\n                \}\n", body, "helmet", re.S)
    chest = exactly_one(r"if \(chest instanceof ItemMekaSuitArmor\) \{(.*?)\n                \} else if \(chest instanceof ItemJetpack", body, "chest", re.S)
    chest_other = exactly_one(r"\} else if \(chest instanceof ItemJetpack \|\| chest instanceof ItemScubaTank\) \{(.*?)\n                \}\n", body, "jetpack e tanque", re.S)
    legs = exactly_one(r"if \(state\.legsEquipment\.getItem\(\) instanceof ItemMekaSuitArmor\) \{(.*?)\n                \}\n", body, "pants", re.S)
    names = {"HEAD": "head", "BODY": "body", "LEFT_ARM": "left_arm", "RIGHT_ARM": "right_arm", "LEFT_LEG": "left_leg", "RIGHT_LEG": "right_leg"}

    def hidden(block):
        return [names[part] for part in re.findall(r"overrideModelPartVisibility\(PartNames\.(\w+), false\)", block)]

    return {
        "mekasuit_helmet": {"hides": hidden(head), "hides_cape": "showCape = false" in head},
        "mekasuit_bodyarmor": {"hides": hidden(chest), "hides_cape": "showCape = false" in chest},
        "jetpack_or_scuba_tank": {"hides": hidden(chest_other), "hides_cape": "showCape = false" in chest_other},
        "mekasuit_pants": {"hides": hidden(legs), "hides_cape": "showCape = false" in legs},
    }


def model_cache():
    text = open(f"{MODEL}/MekanismModelCache.java").read()
    return {
        "left_free_runner": exactly_one(r'LEFT_FREE_RUNNER = registerItemJSON\("([\w/]+)"\)', text, "LEFT_FREE_RUNNER"),
        "left_free_runner_armored": exactly_one(r'LEFT_FREE_RUNNER_ARMORED = registerItemJSON\("([\w/]+)"\)', text, "LEFT_FREE_RUNNER_ARMORED"),
        "right_free_runner": exactly_one(r'RIGHT_FREE_RUNNER = registerItemJSON\("([\w/]+)"\)', text, "RIGHT_FREE_RUNNER"),
        "right_free_runner_armored": exactly_one(r'RIGHT_FREE_RUNNER_ARMORED = registerItemJSON\("([\w/]+)"\)', text, "RIGHT_FREE_RUNNER_ARMORED"),
        "mekasuit": exactly_one(r'MEKASUIT = registerOBJ\("([\w/]+)"\)', text, "MEKASUIT"),
    }


def obj_state():
    text = open(f"{MODEL}/BaseModelCache.java").read()
    exactly_one(r"matrix\.rotate\(Axis\.ZP\.rotation\(Mth\.PI\)\);\n\s+this\.modelState = new ComposedModelState\(BlockModelRotation\.IDENTITY, new Transformation\(matrix\)\);", text, "OBJModelData.modelState")
    return {"rotation": "ZP", "radians": "PI"}


def slots():
    text = open(f"{MINECRAFT}/net/minecraft/world/entity/EquipmentSlot.java").read()
    order = re.findall(r"^    ([A-Z]+)\(EquipmentSlot\.Type", text, re.M)
    return order


def obj_parts():
    parts = []
    current = None
    material = None
    for line in open(f"{RES}/models/entity/mekasuit.obj"):
        words = line.split()
        if not words:
            continue
        if words[0] == "o":
            current = {"name": words[1], "material": None, "quads": 0, "triangles": 0}
            parts.append(current)
        elif words[0] == "usemtl":
            current["material"] = words[1]
        elif words[0] == "f":
            corners = len(words) - 1
            if corners == 4:
                current["quads"] += 1
            elif corners == 3:
                current["triangles"] += 1
            else:
                sys.exit(f"face com {corners} cantos em {current['name']}")
    return parts


def main():
    os.makedirs(OUT, exist_ok=True)
    assets = {}
    for rel in COPIED:
        mine = os.path.join(CONTENT, "assets/mekanism", rel)
        original = os.path.join(RES, rel)
        if not os.path.isfile(original):
            sys.exit(f"falta no original: {rel}")
        if os.path.isfile(mine) and digest(mine) != digest(original):
            sys.exit(f"{rel} difere do original")
        assets[f"assets/mekanism/{rel}"] = digest(original)
    json.dump(assets, open(f"{OUT}/armor_assets.json", "w"), indent=1, sort_keys=True)
    gear = {
        "mekasuit": mekasuit(),
        "chest": chest(),
        "free_runners": free_runners(),
        "models": model_cache(),
        "registration": registration(),
        "obj_state": obj_state(),
        "equipment_slots": slots(),
        "obj_parts": obj_parts(),
    }
    json.dump(gear, open(f"{OUT}/armor_gear.json", "w"), indent=1)
    print(f"armor_assets.json: {len(assets)} arquivos; armor_gear.json: {len(gear['obj_parts'])} partes do OBJ")


if __name__ == "__main__":
    main()
