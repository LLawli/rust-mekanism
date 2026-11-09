#!/usr/bin/env python3
"""Gera o conteúdo dos blocos de máquina e dos blocos avulsos com tile (frente `blocos-maquinas`), sem função.

Uso: blocos_conteudo.py [CHECKOUT_DO_MEKANISM]

Lê `rastros/blocos.jsonl` (o que `testes/BlocosTraceTest.java` tirou do original em jogo: propriedades, estados, formas, luz, tags e componentes de cada bloco) e os
arquivos do original (a datagen, `src/datagen/generated/mekanism`, e os recursos, `src/main/resources`), e escreve em `mods/mekanism`:

- `content/data/mekanism/horizon/block/<id>.json`: o gabarito `minecraft:lodestone`, as propriedades, as propriedades de estado do original, e a forma de seleção e de colisão
  de cada estado (a união das caixas do original, coberta por poucas caixas: o Horizon aceita 16 por forma, e o limite sai pela injeção `shape_limit`);
- `content/data/mekanism/machinery_basis/placement/<id>.json`: como o bloco escolhe o `facing` e o `fluid_logged` ao ser posto (`AttributeStateFacing`, `BlockStateHelper`);
- `content/assets/mekanism/{blockstates,items,models,textures}`: o estado de bloco, a definição do item, os modelos (e os pais) e as texturas, copiados do original;
- `content/data/mekanism/loot_table/blocks/<id>.json`: a tabela de drop da datagen;
- `content/assets/mekanism/lang/en_us.json`: o nome do bloco e a chave `description.*`, na redação da datagen;
- `native/block_rows_table.rs`: a tabela de linhas de jogo (luz do estado ativo, reação ao pistão, oclusão) que a injeção `active_light` aplica.

Idempotente: reescreve o que gera e confere o que copia (um arquivo que já existe e difere do original é erro).
"""
import json
import os
import shutil
import sys
from fractions import Fraction

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
ORIGINAL = sys.argv[1] if len(sys.argv) > 1 else "/mnt/dados/caches/rust-mekanism-ref/mekanism"
CONTENT = os.path.join(ROOT, "mods/mekanism/content")
NATIVE = os.path.join(ROOT, "mods/mekanism/native")
GEN = f"{ORIGINAL}/src/datagen/generated/mekanism"
RES = f"{ORIGINAL}/src/main/resources"
RASTRO = os.path.join(HERE, "rastros/blocos.jsonl")

MACHINES = (
    "antiprotonic_nucleosynthesizer chemical_crystallizer chemical_dissolution_chamber chemical_infuser chemical_injection_chamber chemical_oxidizer chemical_washer "
    "combiner digital_miner dimensional_stabilizer electric_pump electrolytic_separator fluidic_plenisher formulaic_assemblicator fuelwood_heater isotopic_centrifuge "
    "laser laser_amplifier laser_tractor_beam logistical_sorter modification_station nutritional_liquifier oredictionificator osmium_compressor painting_machine "
    "pigment_extractor pigment_mixer precision_sawmill pressurized_reaction_chamber purification_chamber qio_dashboard qio_drive_array qio_exporter qio_importer "
    "qio_redstone_adapter quantum_entangloporter resistive_heater rotary_condensentrator security_desk seismic_vibrator solar_neutron_activator teleporter chargepad "
    "industrial_alarm radioactive_waste_barrel personal_barrel personal_chest cardboard_box"
).split()
FACTORY_TYPES = "combining compressing crushing enriching infusing injecting purifying sawing smelting".split()
TIERS = "basic advanced elite ultimate".split()
FACTORIES = [f"{t}_{k}_factory" for t in TIERS for k in FACTORY_TYPES]
SCOPE = MACHINES + FACTORIES
TEMPLATE = "minecraft:lodestone"
MAX_BOXES = 16


def rows():
    """Os blocos do original em jogo: `blocos.jsonl` (propriedades, estados, tags, item), `blocos_formas.jsonl` (a tabela de formas de cada bloco) e `blocos_colocacao.jsonl`
    (o `facing` e o `fluid_logged` ao pôr). Um estado herda de `base` o que não diferiu do estado padrão."""
    shapes, placement = {}, {}
    for name, target, key in (("blocos_formas", shapes, "shapes"), ("blocos_colocacao", placement, "placement")):
        with open(os.path.join(HERE, f"rastros/{name}.jsonl")) as handle:
            for line in handle:
                row = json.loads(line)
                target[row["id"]] = row[key]
    out = {}
    with open(RASTRO) as handle:
        for line in handle:
            row = json.loads(line)
            row["shapes"] = shapes[row["id"]]
            row["placement"] = placement[row["id"]]
            row["states"] = [{**row["base"], **state} for state in row["states"]]
            out[row["id"].split(":")[1]] = row
    return out


def write_json(path, value, indent=2):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as handle:
        json.dump(value, handle, indent=indent, ensure_ascii=False)
        handle.write("\n")


# --- formas -------------------------------------------------------------------------------------------------------------------------------------------------------


def grid(boxes):
    bs = [tuple(Fraction(c) for c in b) for b in boxes]
    xs = [sorted({b[i] for b in bs} | {b[i + 3] for b in bs}) for i in range(3)]
    idx = [{v: i for i, v in enumerate(a)} for a in xs]
    filled = set()
    for b in bs:
        for i in range(idx[0][b[0]], idx[0][b[3]]):
            for j in range(idx[1][b[1]], idx[1][b[4]]):
                for k in range(idx[2][b[2]], idx[2][b[5]]):
                    filled.add((i, j, k))
    return xs, filled


def maximal_boxes(filled):
    found = set()
    for cell in filled:
        for order in ((0, 1, 2), (0, 2, 1), (1, 0, 2), (1, 2, 0), (2, 0, 1), (2, 1, 0)):
            lo, hi = list(cell), list(cell)
            for axis in order:
                for direction in (1, -1):
                    while True:
                        new_lo, new_hi = list(lo), list(hi)
                        if direction == 1:
                            new_hi[axis] += 1
                            slab = new_hi[axis]
                        else:
                            new_lo[axis] -= 1
                            slab = new_lo[axis]
                        ranges = [range(slab, slab + 1) if a == axis else range(lo[a], hi[a] + 1) for a in range(3)]
                        if all((x, y, z) in filled for x in ranges[0] for y in ranges[1] for z in ranges[2]):
                            lo, hi = new_lo, new_hi
                        else:
                            break
            found.add((tuple(lo), tuple(hi)))
    return found


def cells_of(box):
    lo, hi = box
    return {(x, y, z) for x in range(lo[0], hi[0] + 1) for y in range(lo[1], hi[1] + 1) for z in range(lo[2], hi[2] + 1)}


COVERS = {}
CACHE = os.path.expanduser("~/.cache/rust-mekanism/blocos-covers.json")


def cover(boxes):
    """A união de `boxes` (coordenadas exatas) coberta por poucas caixas que podem se sobrepor: a união no Horizon é um OU de caixas."""
    key = json.dumps(boxes)
    if not COVERS and os.path.exists(CACHE):
        COVERS.update(json.load(open(CACHE)))
    if key in COVERS:
        return COVERS[key]
    xs, filled = grid(boxes)
    candidates = sorted((b, cells_of(b)) for b in maximal_boxes(filled))
    uncovered = set(filled)
    chosen = []
    while uncovered:
        best = max(candidates, key=lambda c: (len(c[1] & uncovered), len(c[1])))
        chosen.append(best)
        uncovered -= best[1]
    changed = True
    while changed:
        changed = False
        for item in sorted(chosen, key=lambda c: len(c[1])):
            rest = set().union(*[c[1] for c in chosen if c is not item]) if len(chosen) > 1 else set()
            if item[1] <= rest:
                chosen.remove(item)
                changed = True
                break
    out = []
    for (lo, hi), _ in sorted(chosen):
        out.append([float(xs[0][lo[0]]), float(xs[1][lo[1]]), float(xs[2][lo[2]]), float(xs[0][hi[0] + 1]), float(xs[1][hi[1] + 1]), float(xs[2][hi[2] + 1])])
    COVERS[key] = out
    os.makedirs(os.path.dirname(CACHE), exist_ok=True)
    json.dump(COVERS, open(CACHE, "w"))
    return out


FULL = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0]


def shapes_of(row, kind):
    """`shapes` ou `collision_shapes` do Horizon para o bloco, ou None se todo estado for o cubo cheio. A forma só depende do `facing`."""
    facing = any(p["name"] == "facing" for p in row["properties"])
    by_facing = {}
    for state in row["states"]:
        key = state["props"].get("facing", "")
        text = json.dumps(row["shapes"][state["shape"]][kind])
        if by_facing.setdefault(key, text) != text:
            raise SystemExit(f"{row['id']}: a forma {kind} muda com outra propriedade além de facing")
    covered = {key: cover(json.loads(text)) for key, text in by_facing.items()}
    if all(c == [FULL] for c in covered.values()):
        return None
    if facing:
        values = [p["values"] for p in row["properties"] if p["name"] == "facing"][0]
        return {"properties": ["facing"], "values": {v: covered[v] for v in values}}
    return {"default": covered[""]}


# --- bloco --------------------------------------------------------------------------------------------------------------------------------------------------------


def sound_block(row):
    sounds = {s["sound"] for s in row["states"]}
    assert sounds == {"minecraft:block.stone.break"}, (row["id"], sounds)
    return "minecraft:stone"


def block_declaration(row):
    first = row["states"][0]
    properties = {"hardness": row["hardness"], "requires_tool": row["requires_tool"], "map_color": first["map_color"], "sounds": sound_block(row)}
    # `Properties.strength` e o arquivo do Horizon aceitam 0 ou mais; a resistência -1 do `AttributeCustomResistance` (o cofre, o barril e a mesa de segurança) vai pela
    # tabela de linhas (`block_rows_table.rs`).
    if row["explosion_state"] >= 0:
        properties["explosion_resistance"] = row["explosion_state"]
        assert row["explosion_state"] == row["resistance"], row["id"]
    static_light = [s["light"] for s in row["states"] if s["props"].get("active") in (None, "false") and s["props"].get("fluid_logged") in (None, "empty")]
    if row["id"].endswith("teleporter"):
        properties["light"] = max(static_light)
    declaration = {"template": TEMPLATE, "properties": properties}
    state_properties = {}
    for prop in row["properties"]:
        values = prop["values"]
        if sorted(values) == ["false", "true"]:
            state_properties[prop["name"]] = {"type": "boolean"}
        else:
            state_properties[prop["name"]] = {"type": "enum", "values": values}
    if state_properties:
        declaration["state_properties"] = dict(sorted(state_properties.items()))
        declaration["default_state"] = dict(sorted(row["default"].items()))
    if row["item"]["max_stack"] != 64:
        declaration["components"] = {"minecraft:max_stack_size": row["item"]["max_stack"]}
    assert row["item"]["rarity"] == "COMMON"
    outline = shapes_of(row, "outline")
    collision = shapes_of(row, "collision")
    if outline:
        declaration["shapes"] = outline
    if collision and collision != outline:
        declaration["collision_shapes"] = collision
    elif outline and collision is None:
        declaration["collision_shapes"] = {"default": [FULL]}
    return declaration


def placement_declaration(row):
    out = {}
    facing = [p for p in row["properties"] if p["name"] == "facing"]
    if facing:
        out["facing"] = {"property": "facing", "mode": row["facing_placement"].lower(), "values": facing[0]["values"]}
    if any(p["name"] == "fluid_logged" for p in row["properties"]):
        out["fluid_logged"] = "fluid_logged"
    return out or None


# --- linhas de jogo -----------------------------------------------------------------------------------------------------------------------------------------------


def row_facts(row):
    """O que o original dá à linha de jogo de cada estado: a luz (do `active`, da lava e fixa), a reação ao pistão e a oclusão. A oclusão é a mesma em todo estado sem fluido
    e outra com `fluid_logged` água ou lava (um fluido dentro do bloco: `getLightDampening` 1 e `propagatesSkylightDown` falso)."""
    states = row["states"]
    groups = {}
    for s in states:
        wet = s["props"].get("fluid_logged") not in (None, "empty")
        groups.setdefault(wet, set()).add((s["push"], s["can_occlude"], s["solid_render"], s["dampening"], s["skylight_down"], s["render_shape"], s["random_ticks"]))
    for wet, keys in groups.items():
        assert len(keys) == 1, (row["id"], wet, keys)
    dry = groups[False].copy().pop()
    wet = groups.get(True, groups[False]).copy().pop()
    push, can_occlude, solid_render, dampening, skylight_down, render_shape, random_ticks = dry
    assert render_shape == "MODEL" and not random_ticks
    assert wet[:3] == dry[:3], (row["id"], dry, wet)
    active_light = max([s["light"] for s in states if s["props"].get("active") == "true" and s["props"].get("fluid_logged") in (None, "empty")] or [0])
    static_light = max([s["light"] for s in states if s["props"].get("active") in (None, "false") and s["props"].get("fluid_logged") in (None, "empty")] or [0])
    for s in states:
        want = max(static_light, active_light if s["props"].get("active") == "true" else 0, 15 if s["props"].get("fluid_logged") == "lava" else 0)
        assert s["light"] == want, (row["id"], s["props"], s["light"], want)
    return {"id": row["id"], "active_light": active_light if active_light != static_light else 0, "push": push, "can_occlude": can_occlude, "solid_render": solid_render,
            "dampening": dampening, "skylight_down": skylight_down, "wet_dampening": wet[3], "wet_skylight_down": wet[4],
            "explosion": row["explosion_state"] if row["explosion_state"] < 0 else None}


def rows_table(facts):
    lines = [
        "// GERADO por `dev/referencia/blocos_conteudo.py` a partir de `rastros/blocos.jsonl` (`testes/BlocosTraceTest.java`, o original em jogo). Não edite à mão.",
        "//",
        "// O que o original dá à linha de jogo de cada estado dos blocos de máquina e dos blocos avulsos com tile que o Horizon não deixa o mod dizer em `properties`:",
        "// a luz do estado ativo (`AttributeStateActive`: `Attributes.ACTIVE_LIGHT` 8, `ACTIVE_MELT_LIGHT` 13, `ACTIVE_FULL_LIGHT` 15), a reação ao pistão (`BlockMekanism.",
        "// getPistonPushReaction`: `IMMOVEABLE` com bloco de entidade) e a oclusão (`isSolidRender`, `canOcclude`, `getLightDampening`, `propagatesSkylightDown`: o cubo",
        "// cheio do gabarito não vale para uma forma menor). A injeção `active_light` aplica a tabela em cada linha, antes das `properties` declaradas.",
        "",
        "/// Uma linha da tabela: o bloco e o que a linha de jogo dele leva.",
        "#[derive(Clone, Copy, Debug, PartialEq)]",
        "pub struct Facts {",
        "    pub id: &'static str,",
        "    /// A luz do estado com `active=true`, quando o original acende o bloco (0: o bloco não tem `active` com luz).",
        "    pub active_light: u8,",
        "    pub push: &'static str,",
        "    pub can_occlude: bool,",
        "    pub solid_render: bool,",
        "    pub dampening: u8,",
        "    pub skylight_down: bool,",
        "    /// A oclusão do estado com água ou lava dentro (`fluid_logged`), que o fluido muda.",
        "    pub wet_dampening: u8,",
        "    pub wet_skylight_down: bool,",
        "    /// A resistência a explosões que o arquivo do Horizon não aceita (negativa: `AttributeCustomResistance`).",
        "    pub explosion: Option<f32>,",
        "}",
        "",
        "/// A luz da lava dentro do bloco (`IStateFluidLoggable.getFluidLightLevel`).",
        "pub const LAVA_LIGHT: u8 = 15;",
        "",
        f"pub const BLOCKS: [Facts; {len(facts)}] = [",
    ]
    for f in facts:
        lines.append(
            f"    Facts {{ id: \"{f['id']}\", active_light: {f['active_light']}, push: \"{f['push']}\", can_occlude: {str(f['can_occlude']).lower()}, "
            f"solid_render: {str(f['solid_render']).lower()}, dampening: {f['dampening']}, skylight_down: {str(f['skylight_down']).lower()}, "
            f"wet_dampening: {f['wet_dampening']}, wet_skylight_down: {str(f['wet_skylight_down']).lower()}, "
            f"explosion: {'None' if f['explosion'] is None else 'Some(' + repr(f['explosion']) + ')'} }},"
        )
    lines += [
        "];",
        "",
        "/// A linha da tabela de um bloco do Mekanism, pelo id.",
        "pub fn facts(id: &str) -> Option<&'static Facts> {",
        "    BLOCKS.iter().find(|facts| facts.id == id)",
        "}",
        "",
        "/// A luz do estado de `facts` com as propriedades `combo` (`active`, `fluid_logged`): a maior entre a do `active`, a da lava e a que o bloco já tinha.",
        "pub fn light(facts: &Facts, combo: &[(String, String)], base: u8) -> u8 {",
        "    let has = |name: &str, value: &str| combo.iter().any(|(k, v)| k == name && v == value);",
        "    let mut light = base;",
        "    if facts.active_light > 0 && has(\"active\", \"true\") {",
        "        light = light.max(facts.active_light);",
        "    }",
        "    if has(\"fluid_logged\", \"lava\") {",
        "        light = light.max(LAVA_LIGHT);",
        "    }",
        "    light",
        "}",
        "",
    ]
    return "\n".join(lines)


# --- arquivos copiados -------------------------------------------------------------------------------------------------------------------------------------------


def blockstate_source(block):
    for base in (f"{RES}/assets/mekanism/blockstates/{block}.json", f"{GEN}/assets/mekanism/blockstates/{block}.json"):
        if os.path.exists(base):
            return base
    raise SystemExit(f"{block}: sem blockstate no original")


def refs(path):
    """Os modelos que um blockstate, uma definição de item ou um modelo cita (`model`, `parent`, `holidays`)."""
    data = json.load(open(path))
    models, textures = set(), set()

    def walk(x):
        if isinstance(x, dict):
            for key, value in x.items():
                if key in ("model", "parent") and isinstance(value, str) and ":" in value:
                    models.add(value)
                if key == "holidays":
                    models.update(value.values())
                if key == "textures" and isinstance(value, dict):
                    textures.update(v for v in value.values() if isinstance(v, str) and not v.startswith("#"))
                walk(value)
        elif isinstance(x, list):
            for value in x:
                walk(value)

    walk(data)
    return models, textures


def model_source(name):
    namespace, path = name.split(":")
    for base in (f"{RES}/assets/{namespace}/models/{path}.json", f"{GEN}/assets/{namespace}/models/{path}.json"):
        if os.path.exists(base):
            return base
    return None


def assets_plan():
    files = {}
    models, textures = {}, set()

    def visit(name):
        if not name.startswith("mekanism:") or name in models:
            return
        source = model_source(name)
        if source is None:
            raise SystemExit(f"modelo {name} sem arquivo no original")
        models[name] = source
        more, tex = refs(source)
        textures.update(tex)
        for item in more:
            visit(item)

    for block in SCOPE:
        state = blockstate_source(block)
        files[f"assets/mekanism/blockstates/{block}.json"] = state
        item = f"{GEN}/assets/mekanism/items/{block}.json"
        files[f"assets/mekanism/items/{block}.json"] = item
        for source in (state, item):
            more, _ = refs(source)
            for name in more:
                visit(name)
    for name, source in models.items():
        files[f"assets/mekanism/models/{name.split(':')[1]}.json"] = source
    for texture in sorted(textures):
        if not texture.startswith("mekanism:"):
            continue
        path = texture.split(":")[1]
        png = f"{RES}/assets/mekanism/textures/{path}.png"
        if not os.path.exists(png):
            raise SystemExit(f"textura {texture} sem arquivo no original")
        files[f"assets/mekanism/textures/{path}.png"] = png
        if os.path.exists(png + ".mcmeta"):
            files[f"assets/mekanism/textures/{path}.png.mcmeta"] = png + ".mcmeta"
    return files


def copy_assets():
    files = assets_plan()
    copied = 0
    for relative, source in sorted(files.items()):
        target = os.path.join(CONTENT, relative)
        if os.path.exists(target):
            if open(target, "rb").read() != open(source, "rb").read():
                raise SystemExit(f"{relative} já existe e difere do original")
            continue
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copyfile(source, target)
        copied += 1
    print(f"assets: {len(files)} arquivos, {copied} novos")


def copy_loot():
    for block in SCOPE:
        shutil.copyfile(f"{GEN}/data/mekanism/loot_table/blocks/{block}.json", os.path.join(CONTENT, f"data/mekanism/loot_table/blocks/{block}.json"))
    print(f"loot_table: {len(SCOPE)}")


def lang(rows_by_id):
    path = os.path.join(CONTENT, "assets/mekanism/lang/en_us.json")
    ours = json.load(open(path))
    original = json.load(open(f"{GEN}/assets/mekanism/lang/en_us.json"))
    add = {}
    for block in SCOPE:
        add[f"block.mekanism.{block}"] = original[f"block.mekanism.{block}"]
        description = [c for c in rows_by_id[block]["item"]["components"] if c.startswith("mekanism:description=")]
        assert len(description) <= 1, block
        # O `cardboard_box` não tem componente de descrição (`ItemBlockCardboardBox`); os outros 83 têm uma chave `description.*`.
        for component in description:
            key = component.split("key='")[1].split("'")[0]
            add[key] = original[key]
    anchor = "block.mekanism.ultimate_universal_cable"
    out = {}
    for key, value in ours.items():
        if key in add:
            continue
        out[key] = value
        if key == anchor:
            out.update(add)
    assert anchor in out
    with open(path, "w") as handle:
        json.dump(out, handle, indent=2, ensure_ascii=False)
        handle.write("\n")
    print(f"lang: {len(add)} chaves")


FUNCTIONAL_EXTRA = (
    "security_desk radioactive_waste_barrel personal_chest personal_barrel chargepad laser laser_amplifier laser_tractor_beam quantum_entangloporter oredictionificator "
    "fuelwood_heater modification_station qio_drive_array qio_dashboard qio_exporter qio_importer qio_redstone_adapter"
).split()


def ported(kind):
    folder = os.path.join(CONTENT, f"data/mekanism/horizon/{kind}")
    return {name[:-5] for name in os.listdir(folder)}


def vanilla_tabs(by_id, order):
    """O que `MekanismCreativeTabs.addToExistingTabs` põe nas abas de blocos funcionais e de redstone, com os blocos que o port tem, na ordem do original (`:99-115`):
    `FUNCTIONAL_BLOCKS` é cada bloco do registro que é transmissor ou `Machine`, e depois a lista fixa; `REDSTONE_BLOCKS` é o alarme, cada bloco com comparador e cada
    cabo de energia (`BlockTransmitter` com `AttributeTier` que não é de transporte), e depois o transporte desviador."""
    have = ported("block")
    functional = [b for b in order if by_id[b]["functional_tab"] and b in have] + [b for b in FUNCTIONAL_EXTRA if b in have]
    redstone = ["industrial_alarm"] + [b for b in order if (by_id[b]["redstone_tab"] or b.endswith("_universal_cable")) and b in have and b != "industrial_alarm"]
    return functional, redstone


def creative(by_id):
    """A aba do mod (`creative_tab/mekanism.json`: os itens na ordem de `MekanismItems`, depois os blocos na de `MekanismBlocks`, só os que o port tem) e as listas das abas do
    vanilla em `native/creative_tabs_table.rs`."""
    order_file = os.path.join(HERE, "rastros/craft_creative_order.json")
    items = json.load(open(order_file))["items"]
    order = [row["id"].split(":")[1] for row in by_id.values()]
    have = ported("item") | ported("block")
    path = os.path.join(CONTENT, "data/mekanism/horizon/creative_tab/mekanism.json")
    tab = json.load(open(path))
    tab["items"] = [f"mekanism:{i}" for i in items + order if i in have and i != "module"]
    write_json(path, tab)
    functional, redstone = vanilla_tabs(by_id, order)
    table = os.path.join(NATIVE, "creative_tabs_table.rs")
    text = open(table).read()
    import re

    def replace(text, tab_id, ids, extra=()):
        entries = "".join(f'            "mekanism:{i}",\n' for i in list(ids) + list(extra))
        pattern = re.compile(r'(\(\s*"minecraft:' + tab_id + r'",\s*&\[\n)(.*?)(\s*\],\s*\),)', re.S)
        assert pattern.search(text), tab_id
        return pattern.sub(lambda m: m.group(1) + entries.rstrip("\n") + m.group(3), text, count=1)

    text = replace(text, "functional_blocks", functional)
    text = replace(text, "redstone_blocks", redstone)
    open(table, "w").write(text)
    print(f"abas: mekanism {len(tab['items'])}, funcionais {len(functional)}, redstone {len(redstone)}")


PARTIAL_TAGS = (
    "minecraft/tags/block/mineable/pickaxe.json",
    "minecraft/tags/block/blocks_motion_no_leaves.json",
    "minecraft/tags/block/guarded_by_piglins.json",
)


def datagen_tags():
    tags = {}
    for namespace in ("c", "minecraft", "mekanism"):
        base = f"{GEN}/data/{namespace}/tags"
        for kind in ("item", "block"):
            folder = f"{base}/{kind}"
            if not os.path.isdir(folder):
                continue
            for dirpath, _, names in os.walk(folder):
                for name in names:
                    path = os.path.join(dirpath, name)
                    key = f"{namespace}/tags/{os.path.relpath(path, base)}"
                    tags[key] = [v if isinstance(v, str) else v["id"] for v in json.load(open(path))["values"]]
    return tags


def sync_tags():
    """Cada tag do original que ficou completa com os ids do port vira arquivo (cópia); a tag do vanilla que o original lista por inteiro e o port achata ganha os ids novos.
    Completa: todo `mekanism:` que ela cita existe no port e toda `#mekanism:` aninhada é uma tag completa. As tags `c:` e `minecraft:` citadas não contam."""
    own = {f"mekanism:{i}" for i in ported("item") | ported("block")}
    tags = datagen_tags()
    complete = {}

    def is_complete(key, seen=()):
        if key in complete:
            return complete[key]
        ok = True
        for entry in tags[key]:
            if entry.startswith("#mekanism:"):
                kind = key.split("/")[2]
                nested = f"mekanism/tags/{kind}/{entry[len('#mekanism:'):]}.json"
                if nested not in tags or nested in seen or not is_complete(nested, seen + (key,)):
                    ok = False
            elif entry.startswith("mekanism:") and entry not in own:
                ok = False
        complete[key] = ok
        return ok

    def expand(key, seen=()):
        out = []
        for entry in tags[key]:
            if entry.startswith("#mekanism:") or (entry.startswith("#") and f"{entry[1:].split(':')[0]}/tags/{key.split('/')[2]}/{entry[1:].split(':')[1]}.json" in tags):
                namespace, rest = entry[1:].split(":")
                nested = f"{namespace}/tags/{key.split('/')[2]}/{rest}.json"
                if nested in tags and nested not in seen:
                    out.extend(expand(nested, seen + (nested,)))
                continue
            out.append(entry)
        return out

    written = changed = 0
    for key, values in sorted(tags.items()):
        cites_own = any(v in own for v in values) or any(
            v.startswith("#mekanism:") and any(e in own for e in expand(key)) for v in values)
        if not cites_own:
            continue
        target = os.path.join(CONTENT, "data", key)
        if key in PARTIAL_TAGS:
            current = json.load(open(target))["values"]
            add = [e for e in dict.fromkeys(expand(key, (key,))) if e in own and e not in current]
            if add:
                write_json(target, {"values": current + add})
                changed += 1
            continue
        if is_complete(key):
            if not os.path.exists(target):
                os.makedirs(os.path.dirname(target), exist_ok=True)
                shutil.copyfile(f"{GEN}/data/{key}", target)
                written += 1
    print(f"tags: {written} novas, {changed} achatadas ampliadas")


NEOFORGE_JAR = "/mnt/dados/caches/rust-mekanism-ref/mc-sources/neoforge-26.3.0.100-sources.jar"
# As tags de item `c:` do NeoForge (o jar `neoforge-26.3.0.100`) que as receitas de mesa dos blocos de máquina citam e o port ainda não tinha, e a do Mekanism que o
# Combiner cita. As de balde, baú e barril (Electric Pump, Osmium Compressor, Personal Chest e Barrel, Formulaic Assemblicator, Modification Station) são das frentes de
# fluidos e de itens de equipamento.
NEOFORGE_TAGS = ["glass_panes", "glass_panes/colorless", "storage_blocks/diamond", "clumps/resin"]
DATAGEN_TAGS = ["mekanism/tags/item/stone_crafting_materials.json"]


def extra_tags():
    import zipfile

    written = 0
    with zipfile.ZipFile(NEOFORGE_JAR) as jar:
        for name in NEOFORGE_TAGS:
            target = os.path.join(CONTENT, f"data/c/tags/item/{name}.json")
            if os.path.exists(target):
                continue
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target, "wb") as handle:
                handle.write(jar.read(f"data/c/tags/item/{name}.json"))
            written += 1
    for key in DATAGEN_TAGS:
        target = os.path.join(CONTENT, "data", key)
        if not os.path.exists(target):
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copyfile(f"{GEN}/data/{key}", target)
            written += 1
    print(f"tags de outras fontes: {written} novas")


def sync_recipes():
    """Cada receita de mesa e de forno da datagen que ficou completa com os itens, blocos e tags do port vira arquivo (cópia). Completa, como em `tests/craft_data.rs`: cada
    item e tag citada existe no port ou no vanilla, e o resultado não tem componente que o port não registra (só o tipo de upgrade)."""
    recipes = json.load(open(os.path.join(HERE, "rastros/craft_recipes.json")))
    vanilla = json.load(open(os.path.join(HERE, "rastros/horizon_vanilla_ids.json")))
    vanilla_items, vanilla_tags = set(vanilla["items"]), set(vanilla["item_tags"])
    own = {f"mekanism:{i}" for i in ported("item") | ported("block")}

    def exists(entry):
        if entry.startswith("#"):
            namespace, rest = entry[1:].split(":")
            if namespace == "minecraft":
                return entry[1:] in vanilla_tags
            return os.path.isfile(os.path.join(CONTENT, f"data/{namespace}/tags/item/{rest}.json"))
        if entry.startswith("minecraft:"):
            return entry in vanilla_items
        return entry in own

    written = 0
    for relative, recipe in sorted(recipes.items()):
        complete = all(exists(r) for r in recipe["refs"]) and exists(recipe["result"]) and all(c == "mekanism:upgrade_type" for c in recipe["result_components"])
        target = os.path.join(CONTENT, "data", relative)
        if complete and not os.path.exists(target):
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copyfile(f"{GEN}/data/{relative}", target)
            written += 1
    print(f"receitas: {written} novas")


def main():
    by_id = rows()
    missing = [b for b in SCOPE if b not in by_id]
    if missing:
        raise SystemExit(f"o rastro não tem {missing}")
    facts = []
    for block in SCOPE:
        row = by_id[block]
        write_json(os.path.join(CONTENT, f"data/mekanism/horizon/block/{block}.json"), block_declaration(row))
        placement = placement_declaration(row)
        target = os.path.join(CONTENT, f"data/mekanism/machinery_basis/placement/{block}.json")
        if placement:
            write_json(target, placement)
        elif os.path.exists(target):
            os.remove(target)
        facts.append(row_facts(row))
    with open(os.path.join(NATIVE, "block_rows_table.rs"), "w") as handle:
        handle.write(rows_table(facts))
    print(f"blocos: {len(SCOPE)}")
    copy_assets()
    copy_loot()
    lang(by_id)
    creative(by_id)
    sync_tags()
    extra_tags()
    # As receitas dependem das tags e das tags dependerem dos itens: a segunda passada pega o que a primeira completou.
    for _ in range(2):
        sync_recipes()


if __name__ == "__main__":
    main()
