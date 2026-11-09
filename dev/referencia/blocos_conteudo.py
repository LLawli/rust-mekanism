#!/usr/bin/env python3
"""Gera o conteúdo dos blocos de máquina, dos blocos avulsos com tile (frente `blocos-maquinas`) e dos de armazenamento, indução, multibloco e transmissores (frente `blocos-estrutura`), sem função.

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
# A frente `blocos-estrutura`: os blocos de `MekanismBlocks` que sobravam. Por tier (os quatro e o criativo onde o original o tem): o depósito, o tanque de fluido, o tanque de
# químico e o cubo de energia; a indução; as partes de multibloco; e os transmissores (os quatro tiers de cada, mais o restritivo e o desviador).
TIERS5 = TIERS + ["creative"]
STORAGE = [f"{t}_{k}" for k in ("bin", "fluid_tank", "chemical_tank", "energy_cube") for t in TIERS5]
INDUCTION = [f"{t}_induction_{k}" for k in ("cell", "provider") for t in TIERS] + ["induction_casing", "induction_port"]
MULTIBLOCK = (
    "boiler_casing boiler_valve dynamic_tank dynamic_valve sps_casing sps_port thermal_evaporation_block thermal_evaporation_controller thermal_evaporation_valve "
    "structural_glass supercharged_coil superheating_element pressure_disperser"
).split()
# Família, tamanho (`BlockSmallTransmitter` ou `BlockLargeTransmitter`) e a tag de bloco dos transmissores que se juntam (`mekanism:transmitters/<tag>`, da datagen).
TRANSMITTER_KINDS = {
    "universal_cable": ("small", None),
    "pressurized_tube": ("small", "chemical"),
    "thermodynamic_conductor": ("small", "heat"),
    "mechanical_pipe": ("large", "fluid"),
    "logistical_transporter": ("large", "items"),
}
CABLES = [f"{t}_universal_cable" for t in TIERS]
TRANSMITTERS = [f"{t}_{k}" for k in TRANSMITTER_KINDS if k != "universal_cable" for t in TIERS] + ["restrictive_transporter", "diversion_transporter"]
STRUCTURE = STORAGE + INDUCTION + MULTIBLOCK + TRANSMITTERS + ["bounding_block"]
SCOPE = MACHINES + FACTORIES + STRUCTURE
# O que a tabela de linhas leva: o escopo e os quatro cabos, que a frente anterior deixou com a linha do gabarito do chorus plant.
ROWS_SCOPE = SCOPE + CABLES
ENERGY_CUBES = [f"{t}_energy_cube" for t in TIERS5]
FLUID_TANKS = [f"{t}_fluid_tank" for t in TIERS5]
TEMPLATE = "minecraft:lodestone"
CHORUS = "minecraft:chorus_plant"
MAX_BOXES = 16


def rows(prefix="blocos"):
    """Os blocos do original em jogo: `blocos.jsonl` (propriedades, estados, tags, item), `blocos_formas.jsonl` (a tabela de formas de cada bloco) e `blocos_colocacao.jsonl`
    (o `facing` e o `fluid_logged` ao pôr). Um estado herda de `base` o que não diferiu do estado padrão. `prefix` escolhe o conjunto: `blocos` é o do Mekanism e
    `blocos_generators` o do Mekanism Generators (`generators_conteudo.py`), com os mesmos três arquivos e o mesmo formato."""
    shapes, placement = {}, {}
    for name, target, key in ((f"{prefix}_formas", shapes, "shapes"), (f"{prefix}_colocacao", placement, "placement")):
        with open(os.path.join(HERE, f"rastros/{name}.jsonl")) as handle:
            for line in handle:
                row = json.loads(line)
                target[row["id"]] = row[key]
    out = {}
    with open(os.path.join(HERE, f"rastros/{prefix}.jsonl")) as handle:
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


# O som de quebra que o original dá ao bloco e o bloco do vanilla cujo som o port usa (`properties.sounds`).
SOUND_BLOCKS = {"minecraft:block.stone.break": "minecraft:stone", "minecraft:block.glass.break": "minecraft:glass"}


def sound_block(row):
    sounds = {s["sound"] for s in row["states"]}
    # `DEFAULT` do `SoundType` é a pedra (`BlockBehaviour.Properties`); só o vidro estrutural troca o som (`BlockStructuralGlass`: `SoundType.GLASS`).
    assert len(sounds) == 1 and sounds <= set(SOUND_BLOCKS), (row["id"], sounds)
    return SOUND_BLOCKS[sounds.pop()]


def transmitter_kind(block):
    """A família de um transmissor (`mechanical_pipe`, `restrictive_transporter`...) e o tamanho; None se o bloco não for um."""
    for kind, (size, _) in TRANSMITTER_KINDS.items():
        if block.endswith("_" + kind):
            return kind
    if block in ("restrictive_transporter", "diversion_transporter"):
        return block
    return None


def transmitter_size(block):
    kind = transmitter_kind(block)
    return TRANSMITTER_KINDS[kind][0] if kind in TRANSMITTER_KINDS else "large"


def transmitter_tag(block):
    kind = transmitter_kind(block)
    if kind == "universal_cable":
        return "mekanism:energy_connects"
    # Os transportadores logísticos, o restritivo e o desviador se juntam entre si (`#mekanism:transmitters/items`).
    return f"mekanism:transmitters/{TRANSMITTER_KINDS[kind][1] if kind in TRANSMITTER_KINDS else 'items'}"


def transmitter_declaration(row):
    """Um transmissor: o bloco que conecta do Horizon (o chorus plant sem as regras dele, `connects_to` e `thickness`), com a dureza, a resistência, a cor e o som do original.
    O estado por lado do original (a conexão vem da block entity) e a rede são das Fases 3 e 4; `fluid_logged` espera (decisão 26)."""
    first = row["states"][0]
    block = row["id"].split(":")[1]
    assert not first["solid_render"] and row["hardness"] == 1.0 and row["resistance"] == 6.0 and not row["requires_tool"], block
    assert row["explosion_state"] == row["resistance"], block
    assert sound_block(row) == "minecraft:stone", block
    return {
        "template": CHORUS,
        "connects_to": "#" + transmitter_tag(block),
        "thickness": 6 if transmitter_size(block) == "small" else 8,
        "properties": {"hardness": row["hardness"], "explosion_resistance": row["explosion_state"], "requires_tool": False, "map_color": first["map_color"], "sounds": "minecraft:stone"},
    }


def block_declaration(row, template=TEMPLATE):
    block = row["id"].split(":")[1]
    if block in CABLES or block in TRANSMITTERS:
        return transmitter_declaration(row)
    first = row["states"][0]
    properties = {"hardness": row["hardness"], "requires_tool": row["requires_tool"], "map_color": first["map_color"], "sounds": sound_block(row)}
    # `Properties.strength` e o arquivo do Horizon aceitam 0 ou mais; a resistência -1 do `AttributeCustomResistance` (o cofre, o barril e a mesa de segurança) vai pela
    # tabela de linhas (`block_rows_table.rs`).
    if row["explosion_state"] >= 0:
        properties["explosion_resistance"] = row["explosion_state"]
        assert row["explosion_state"] == row["resistance"], row["id"]
    # Um bloco de forma menor que o cubo (`isSolidRender` falso) não esconde a face do vizinho. O Horizon esconde pela forma de oclusão da tabela `occlusion.nbt`, que um bloco de mod copia do
    # gabarito (o cubo cheio do `lodestone`), e pelo `can_occlude` de `render-definitions.json`; `occludes: false` põe o segundo falso (decisão 22): sem isso o chão sob a máquina perde a face de cima.
    if not row["states"][0]["solid_render"]:
        properties["occludes"] = False
    # `RenderShape.INVISIBLE` (o `bounding_block`: `BlockBounding.getRenderShape`) não desenha o bloco.
    if first["render_shape"] != "MODEL":
        properties["render_shape"] = first["render_shape"]
    static_light = [s["light"] for s in row["states"] if s["props"].get("active") in (None, "false") and s["props"].get("fluid_logged") in (None, "empty")]
    if row["id"].endswith("teleporter"):
        properties["light"] = max(static_light)
    declaration = {"template": template, "properties": properties}
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
    components = {}
    if row["item"]["max_stack"] != 64:
        components["minecraft:max_stack_size"] = row["item"]["max_stack"]
    # A raridade do item (`Rarity` do `Item.Properties`): o SPS e a bobina supercarregada são épicos, o resto comum.
    assert row["item"]["rarity"] in ("COMMON", "EPIC"), block
    if row["item"]["rarity"] == "EPIC":
        components["minecraft:rarity"] = "epic"
    if components:
        declaration["components"] = components
    # O `bounding_block` é só bloco (`BlockRegistryObject` sem item): o Horizon faz um item com o mesmo id, a menos que o arquivo diga que não.
    if row["item"]["id"] == "minecraft:air":
        declaration["item"] = False
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
    # Os transmissores do port não têm `fluid_logged` (o chorus plant do Horizon não aceita o estado do original, decisão 26).
    if row["id"].split(":")[1] in CABLES + TRANSMITTERS:
        return None
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
    assert render_shape in ("MODEL", "INVISIBLE") and not random_ticks
    assert can_occlude or not solid_render
    assert wet[:3] == dry[:3], (row["id"], dry, wet)
    active_light = max([s["light"] for s in states if s["props"].get("active") == "true" and s["props"].get("fluid_logged") in (None, "empty")] or [0])
    static_light = max([s["light"] for s in states if s["props"].get("active") in (None, "false") and s["props"].get("fluid_logged") in (None, "empty")] or [0])
    for s in states:
        want = max(static_light, active_light if s["props"].get("active") == "true" else 0, 15 if s["props"].get("fluid_logged") == "lava" else 0)
        assert s["light"] == want, (row["id"], s["props"], s["light"], want)
    # O original dá `canOcclude` verdadeiro a todo bloco menos o do `noOcclusion`, e o `Block.shouldRenderFace` do vanilla esconde a face do vizinho pela forma de oclusão. O mesher do Horizon
    # esconde pelo `can_occlude` sozinho (um bloco de forma menor com `can_occlude` verdadeiro deixa um buraco na face do vizinho: visto em jogo, a grama sob a máquina some). Então a
    # linha do port diz `can_occlude` igual ao `solid_render`: verdadeiro só no cubo cheio (decisão 22, H49).
    can_occlude = solid_render
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
                if key in ("model", "parent", "lit", "unlit") and isinstance(value, str) and ":" in value:
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


# O que o port escreve em vez de copiar, porque o Horizon não carrega o que o original usa (decisão 26): o modelo de transmissor é um OBJ próprio do Mekanism
# (`mekanism:special/transmitter`, `.obj.mek`), o do cubo de energia um carregador de modelo (`mekanism:energy_cube_sided`, `mekanism:energy_cube`) e o item do tanque de
# fluido e do cubo de energia um desenhista especial (`minecraft:special` com `mekanism:fluid_tank` e `mekanism:energy_cube`, que o Horizon troca pelo modelo base, e o base
# dos dois só tem `display`).
GENERATED_BLOCKSTATES = TRANSMITTERS + ENERGY_CUBES
GENERATED_ITEMS = TRANSMITTERS + ENERGY_CUBES + FLUID_TANKS
DIRECTIONS = (("north", {}), ("east", {"y": 90}), ("south", {"y": 180}), ("west", {"y": 270}), ("up", {"x": 270}), ("down", {"x": 90}))
# `RelativeSide.VALUES`: a ordem em que o `EnergyCubeModel` junta as partes.
RELATIVE_SIDES = ("front", "left", "right", "back", "top", "bottom")


def read_json(path):
    with open(path) as handle:
        return json.load(handle)


def transmitter_textures(block):
    """As texturas do centro e do braço de um transmissor: as do modelo do original (`transmitter/<tamanho>/<família>/<tier>.json`), nas variantes `opaque` como o cabo que o port
    já tinha (o braço recortado e o centro animado são da Fase 4)."""
    kind = transmitter_kind(block)
    size = transmitter_size(block)
    if kind in TRANSMITTER_KINDS:
        tier = block[: -len(kind) - 1]
        path = f"{RES}/assets/mekanism/models/block/transmitter/{size}/{kind}/{tier}.json"
    else:
        path = f"{RES}/assets/mekanism/models/block/transmitter/large/{block}.json"
    textures = read_json(path)["textures"]
    return textures["center_opaque"], textures["side_opaque"]


def box_faces(texture, faces):
    return {face: {"uv": [0, 0, 16, 16], "texture": texture} for face in faces}


def transmitter_assets(block):
    """O blockstate (as seis conexões do chorus plant), o modelo do centro e o do braço de um transmissor, e o item (o centro, como o do cabo). O corpo do centro é o cubo de 6 ou 8 pixels
    (`transmitter_small.obj.mek` e `transmitter_large.obj.mek`: 0,3125 a 0,6875 e 0,25 a 0,75) e o braço vai da borda até ele, com as quatro faces laterais (a da ponta é escondida)."""
    size = transmitter_size(block)
    center_texture, side_texture = transmitter_textures(block)
    lo, hi = (5, 11) if size == "small" else (4, 12)
    center = {
        "parent": "minecraft:block/block",
        "textures": {"particle": "#center", "center": center_texture},
        "elements": [{"from": [lo, lo, lo], "to": [hi, hi, hi], "faces": box_faces("#center", ("north", "south", "east", "west", "up", "down"))}],
    }
    # O braço do cabo mapeia a faixa vertical da textura em 13,33 de 16 para os 5 pixels de comprimento (v de 1,33 a 14,67); o do tamanho grande tem 4.
    v0, v1 = (1.33, 14.67) if size == "small" else (2.67, 13.33)
    arm_faces = {
        "up": {"uv": [0, v0, 16, v1], "texture": "#side"},
        "down": {"uv": [0, v0, 16, v1], "texture": "#side"},
        "east": {"uv": [0, v0, 16, v1], "rotation": 90, "texture": "#side"},
        "west": {"uv": [0, v0, 16, v1], "rotation": 90, "texture": "#side"},
    }
    side = {
        "parent": "minecraft:block/block",
        "textures": {"particle": "#side", "side": side_texture},
        "elements": [{"from": [lo, lo, 0], "to": [hi, hi, lo], "faces": arm_faces}],
    }
    multipart = [{"apply": {"model": f"mekanism:block/transmitter_port/{block}_center"}}]
    for name, rotation in DIRECTIONS:
        multipart.append({"when": {name: "true"}, "apply": {"model": f"mekanism:block/transmitter_port/{block}_side", **rotation}})
    return {
        f"assets/mekanism/blockstates/{block}.json": {"multipart": multipart},
        f"assets/mekanism/items/{block}.json": {"model": {"type": "minecraft:model", "model": f"mekanism:block/transmitter_port/{block}_center"}},
        f"assets/mekanism/models/block/transmitter_port/{block}_center.json": center,
        f"assets/mekanism/models/block/transmitter_port/{block}_side.json": side,
    }, {center_texture, side_texture}


def lit_led(element, shift):
    """O LED aceso (`EnergyCubeBaseGeometry.remapLedUVs`): as mesmas caixas com o uv deslocado e a luz do bloco em 15."""
    lit = json.loads(json.dumps(element))
    for face in lit["faces"].values():
        if "uv" in face:
            face["uv"] = [face["uv"][0] + shift[0], face["uv"][1] + shift[1], face["uv"][2] + shift[0], face["uv"][3] + shift[1]]
    lit["light_emission"] = 15
    return lit


def energy_cube_model(tier, item):
    """O modelo do cubo de energia sem os dados de lado: o `EnergyCubeModel` junta o quadro e, por lado, o LED (e a porta, se o lado não for `INACTIVE`). Sem block entity todo lado é
    `INACTIVE` (o bloco posto); o item sai com a configuração de lados do item (`ItemBlockEnergyCube.SIDE_CONFIG`: saída só na frente, `FRONT_OUT_EJECT`), o lado de saída aceso e com a porta."""
    base = read_json(f"{RES}/assets/mekanism/models/block/energy_cube/base.json")
    corner = read_json(f"{RES}/assets/mekanism/models/block/energy_cube/{tier}.json")
    assert corner["parent"] == "mekanism:block/energy_cube/base", tier
    shift = base["leds"]["lit_uv_shift"]
    elements = list(base["frame"])
    for side in RELATIVE_SIDES:
        if item and side == "front":
            elements += [lit_led(e, shift) for e in base["leds"][side]] + list(base["ports"][side])
        else:
            elements += list(base["leds"][side])
    return {"parent": "minecraft:block/block", "textures": {**base["textures"], **corner["textures"]}, "elements": elements}


def energy_cube_assets(block):
    tier = block[: -len("_energy_cube")]
    # O original lista os seis `facing` em ordem alfabética (`down`, `east`, `north`, `south`, `up`, `west`).
    states = {f"facing={f}": {"model": f"mekanism:block/energy_cube/{tier}_static", **dict(DIRECTIONS)[f]} for f in sorted(dict(DIRECTIONS))}
    display = read_json(f"{RES}/assets/mekanism/models/item/{block}.json")["display"]
    files = {
        f"assets/mekanism/blockstates/{block}.json": {"variants": states},
        f"assets/mekanism/items/{block}.json": {"model": {"type": "minecraft:model", "model": f"mekanism:item/{block}_port"}},
        f"assets/mekanism/models/block/energy_cube/{tier}_static.json": energy_cube_model(tier, False),
        f"assets/mekanism/models/block/energy_cube/{tier}_item.json": energy_cube_model(tier, True),
        f"assets/mekanism/models/item/{block}_port.json": {"parent": f"mekanism:block/energy_cube/{tier}_item", "display": display},
    }
    textures = set(files[f"assets/mekanism/models/block/energy_cube/{tier}_static.json"]["textures"].values())
    return files, textures


def fluid_tank_item(block):
    """O item do tanque de fluido sem fluido: o desenhista (`RenderFluidTankItem`) põe o modelo do estado padrão do bloco; o `display` é o do modelo base do item."""
    display = read_json(f"{RES}/assets/mekanism/models/item/{block}.json")["display"]
    return {
        f"assets/mekanism/items/{block}.json": {"model": {"type": "minecraft:model", "model": f"mekanism:item/{block}_port"}},
        f"assets/mekanism/models/item/{block}_port.json": {"parent": "mekanism:block/fluid_tank", "display": display},
    }


def generated_assets():
    """Os arquivos que o port escreve (JSON) e as texturas que eles citam."""
    files, textures = {}, set()
    for block in TRANSMITTERS:
        more, tex = transmitter_assets(block)
        files.update(more)
        textures |= tex
    for block in ENERGY_CUBES:
        more, tex = energy_cube_assets(block)
        files.update(more)
        textures |= tex
    for block in FLUID_TANKS:
        files.update(fluid_tank_item(block))
    return files, textures


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
        sources = []
        if block not in GENERATED_BLOCKSTATES:
            state = blockstate_source(block)
            files[f"assets/mekanism/blockstates/{block}.json"] = state
            sources.append(state)
        if block not in GENERATED_ITEMS and block != "bounding_block":
            item = f"{GEN}/assets/mekanism/items/{block}.json"
            files[f"assets/mekanism/items/{block}.json"] = item
            sources.append(item)
        for source in sources:
            more, _ = refs(source)
            for name in more:
                visit(name)
    # O `display` do item de cada tanque vem do modelo base (`models/item/<tanque>.json`); o modelo do estado do bloco já entra pelo blockstate.
    generated, extra = generated_assets()
    textures |= extra
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
    return files, generated


# Os blockstates que o port adapta: o `qio_redstone_adapter` tem, no estado ativo, uma variante `mekanism:special/qio_redstone_adapter` (`QIORedstoneAdapterModel`) que escolhe
# o modelo aceso ou apagado pelo dado do modelo da block entity (`EMITTING`) e não tem `model`: o Horizon recusa o arquivo inteiro ("missing model"). Sem block entity o
# adaptador não emite, então a variante ganha `model` com o modelo apagado (`unlit`), e o resto (`type`, `lit`, `unlit`) fica como o original o escreveu.
ADAPTED_BLOCKSTATES = ["assets/mekanism/blockstates/qio_redstone_adapter.json"]


def adapt_blockstate(text):
    data = json.loads(text)
    for variant in data["variants"].values():
        if "type" in variant and "model" not in variant:
            variant["model"] = variant["unlit"]
    return json.dumps(data, indent=2) + "\n"


def copy_assets():
    files, generated = assets_plan()
    for relative, value in sorted(generated.items()):
        write_json(os.path.join(CONTENT, relative), value, indent=2)
    copied = 0
    for relative, source in sorted(files.items()):
        target = os.path.join(CONTENT, relative)
        if relative in ADAPTED_BLOCKSTATES:
            os.makedirs(os.path.dirname(target), exist_ok=True)
            with open(target, "w") as handle:
                handle.write(adapt_blockstate(open(source).read()))
            continue
        if os.path.exists(target):
            if open(target, "rb").read() != open(source, "rb").read():
                raise SystemExit(f"{relative} já existe e difere do original")
            continue
        os.makedirs(os.path.dirname(target), exist_ok=True)
        shutil.copyfile(source, target)
        copied += 1
    print(f"assets: {len(files)} copiados ({copied} novos), {len(generated)} escritos")


def copy_loot():
    # O `bounding_block` não tem tabela de drop na datagen: não solta nada (o bloco de apoio some com a máquina que o pôs).
    blocks = [b for b in SCOPE if b != "bounding_block"]
    for block in blocks:
        shutil.copyfile(f"{GEN}/data/mekanism/loot_table/blocks/{block}.json", os.path.join(CONTENT, f"data/mekanism/loot_table/blocks/{block}.json"))
    print(f"loot_table: {len(blocks)}")


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
    redstone = ["industrial_alarm"] + [b for b in order if (by_id[b]["redstone_tab"] or redstone_transmitter(by_id[b])) and b in have and b != "industrial_alarm"]
    if "diversion_transporter" in have:
        redstone.append("diversion_transporter")
    return functional, redstone


def redstone_transmitter(row):
    """`BlockTransmitter` com `AttributeTier` cujo tier não é o do transportador (`MekanismCreativeTabs.java:107-112`): os cabos, os tubos, os canos e os condutores."""
    tiered = "AttributeTier" in row["attributes"] and not any(c.startswith("mekanism:tier/transporter=") for c in row["item"]["components"])
    return row["class"] in ("BlockSmallTransmitter", "BlockLargeTransmitter") and tiered


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
    # `addToDisplay(MekanismFluids.FLUIDS, ..)` vem depois dos blocos: os baldes dos fluidos do Mekanism, na ordem do registro (`rastros/fluids.jsonl`, frente `fluidos`).
    with open(os.path.join(HERE, "rastros/fluids.jsonl")) as handle:
        for line in handle:
            if line.strip():
                fluid = json.loads(line)
                if fluid["mod"] == "mekanism":
                    tab["items"].append(fluid["bucket"]["id"])
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
# Combiner cita. O balde vazio, o baú e o barril de madeira (Electric Pump, Osmium Compressor, Personal Chest e Barrel, Formulaic Assemblicator, Modification Station e Oredictionificator) também são do NeoForge, como as da
# decisão 19: o arquivo é o do jar, byte a byte.
NEOFORGE_TAGS = ["glass_panes", "glass_panes/colorless", "storage_blocks/diamond", "clumps/resin", "buckets/empty", "chests/wooden", "barrels/wooden"]
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
    missing = [b for b in ROWS_SCOPE if b not in by_id]
    if missing:
        raise SystemExit(f"o rastro não tem {missing}")
    facts = []
    for block in ROWS_SCOPE:
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
    print(f"blocos: {len(SCOPE)} e {len(CABLES)} cabos")
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
