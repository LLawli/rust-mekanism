#!/usr/bin/env python3
"""Gera os rastros da frente `itens-craft` (itens e blocos de craft e de recurso) a partir da datagen e do código do original.

Uso: craft_rastros.py [CHECKOUT_DO_MEKANISM] [CHECKOUT_DO_HORIZON_PIN]

O que sai, em `dev/referencia/rastros/`:
- `craft_datagen.json`: o hash (FNV-1a 64 e tamanho) de cada arquivo de `mods/mekanism/content` que o port copia do original (a datagen, `src/datagen/generated/mekanism`,
  ou os `src/main/resources` dele), para `tests/craft_content.rs` conferir byte a byte (JSON pelo texto canônico: chaves em ordem, sem espaço);
- `craft_tags.json`: os valores de cada tag `c:`, `minecraft:` e `mekanism:` da datagen que cita um id da frente (e `craft_tags_expanded.json`, com as tags aninhadas resolvidas);
- `blocos.jsonl` não sai daqui: é o original em jogo (`testes/BlocosTraceTest.java`), de onde vêm a ordem de registro dos blocos e as listas das abas do vanilla;
- `craft_lang.json`: o texto da datagen para cada chave do `en_us.json` do port;
- `craft_worldgen.json`: os arquivos de worldgen da datagen (`worldgen/feature`, `placed_feature`, `neoforge/biome_modifier`) dos minérios e do sal;
- `craft_recipes.json`: as receitas `minecraft:*` da datagen que a frente leva (todos os ingredientes e o resultado existem) e as que ficam de fora, com o motivo;
- `craft_creative_order.json`: a ordem de registro de `MekanismItems` e de `MekanismBlocks`, que é a ordem da aba criativa;
- `craft_neoforge_tags.json` (frente `itens-equipamento`): os valores das tags `c:` do NeoForge (`NEOFORGE_TAGS`) que receitas da datagen citam e que o Horizon não traz;
- `estrutura_modelos.json` (frente `blocos-estrutura`): do original, o que o port reescreve porque o Horizon não carrega o modelo dele: as texturas do modelo de cada transmissor, o modelo base do cubo de energia (quadro, LEDs, portas, o deslocamento do LED aceso) com as texturas de cada tier, e o `display` do item do cubo de energia e do tanque de fluido;
- `equipment_items.json` (frente `itens-equipamento`): o registro de cada item de equipamento lido do Java (pilha, raridade, resistência ao fogo, durabilidade, encantável, recarga de uso, slot e ativo do `Equippable`, atributos de armadura) e os números de armadura da configuração de inicialização e do `ArmorMaterials.NETHERITE`.
O Rust não lê o checkout do original: lê estes arquivos.
"""
import json
import os
import re
import sys

ORIGINAL = sys.argv[1] if len(sys.argv) > 1 else "/mnt/dados/caches/rust-mekanism-ref/mekanism"
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
CONTENT = os.path.join(ROOT, "mods/mekanism/content")
OUT = os.path.join(HERE, "rastros")
GEN = f"{ORIGINAL}/src/datagen/generated/mekanism"
RES = f"{ORIGINAL}/src/main/resources/assets/mekanism"
JAVA = f"{ORIGINAL}/src/main/java/mekanism/common/registries"
NEOFORGE = sys.argv[3] if len(sys.argv) > 3 else "/mnt/dados/caches/rust-mekanism-ref/mc-sources/nf"


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


def walk(root):
    for dp, _, files in os.walk(root):
        for f in files:
            yield os.path.join(dp, f)


def original_for(rel: str):
    """O arquivo do original que corresponde a `rel` (relativo a `mods/mekanism/content`), na datagen ou nos recursos."""
    candidates = [f"{GEN}/{rel}"]
    if rel.startswith("assets/mekanism/"):
        candidates.append(f"{RES}/{rel[len('assets/mekanism/'):]}")
    return next((c for c in candidates if os.path.exists(c)), None)


# Arquivos do port que NÃO são cópia do original (adaptados ao Horizon ou escritos para ele): ficam fora do hash.
ADAPTED_PREFIXES = (
    "data/mekanism/horizon/",
    "data/mekanism/worldgen/",
    "data/mekanism/neoforge/",
    "data/mekanism/machinery_basis/",
    "data/mekanism/mekanism/",  # químicos e upgrades: outras frentes
    "assets/mekanism/lang/",
    "assets/mekanism/sounds.json",
)


SCOPE_BLOCKS = [
    "block_bio_fuel", "block_bronze", "block_charcoal", "block_fluorite", "block_lead", "block_osmium", "block_raw_lead", "block_raw_osmium", "block_raw_tin",
    "block_raw_uranium", "block_refined_glowstone", "block_refined_obsidian", "block_salt", "block_steel", "block_tin", "block_uranium", "deepslate_fluorite_ore",
    "deepslate_lead_ore", "deepslate_osmium_ore", "deepslate_tin_ore", "deepslate_uranium_ore", "fluorite_ore", "lead_ore", "osmium_ore", "steel_casing",
    "teleporter_frame", "tin_ore", "uranium_ore",
]


def horizon_ids(kind):
    folder = f"{CONTENT}/data/mekanism/horizon/{kind}"
    return sorted(f[:-5] for f in os.listdir(folder))


def tags_rastro():
    """Os valores de cada tag da datagen (c, minecraft, mekanism) que cita um item ou bloco da frente."""
    ids = {f"mekanism:{i}" for i in horizon_ids("item") + horizon_ids("block")}
    out = {}
    for namespace in ("c", "minecraft", "mekanism"):
        base = f"{GEN}/data/{namespace}/tags"
        if not os.path.isdir(base):
            continue
        for path in sorted(walk(base)):
            rel = os.path.relpath(path, base)
            kind = rel.split("/")[0]
            if kind not in ("item", "block"):
                continue
            values = [v if isinstance(v, str) else v["id"] for v in json.load(open(path))["values"]]
            if any(v in ids for v in values):
                out[f"{namespace}/tags/{rel}"] = values
    json.dump(out, open(f"{OUT}/craft_tags.json", "w"), indent=0, sort_keys=True)
    print("craft_tags.json:", len(out))
    # Os mesmos valores com as tags aninhadas do próprio original resolvidas (`#mekanism:factory_supported` vira os ids dela): as tags do vanilla que o port achata
    # (`mineable/pickaxe`) são conferidas por esta lista, não pela direta.
    def expand(key, seen=()):
        values = []
        for v in json.load(open(f"{GEN}/data/{key}"))["values"]:
            v = v if isinstance(v, str) else v["id"]
            if v.startswith("#"):
                namespace, rest = v[1:].split(":")
                kind = key.split("/")[2]
                nested = f"{namespace}/tags/{kind}/{rest}.json"
                if os.path.exists(f"{GEN}/data/{nested}") and nested not in seen:
                    values.extend(expand(nested, seen + (nested,)))
                    continue
            values.append(v)
        return values

    json.dump({key: expand(key, (key,)) for key in out}, open(f"{OUT}/craft_tags_expanded.json", "w"), indent=0, sort_keys=True)
    print("craft_tags_expanded.json:", len(out))


def worldgen_rastro():
    out = {}
    for sub in ("worldgen/feature", "worldgen/placed_feature", "neoforge/biome_modifier"):
        for path in sorted(walk(f"{GEN}/data/mekanism/{sub}")):
            name = os.path.basename(path)
            if "retrogen" in name:
                continue
            out[f"{sub}/{name}"] = json.load(open(path))
    json.dump(out, open(f"{OUT}/craft_worldgen.json", "w"), separators=(",", ":"), sort_keys=True)
    print("craft_worldgen.json:", len(out), os.path.getsize(f"{OUT}/craft_worldgen.json"))


def ingredient_refs(x, out):
    if isinstance(x, str):
        out.append(x)
    elif isinstance(x, list):
        for y in x:
            ingredient_refs(y, out)
    elif isinstance(x, dict):
        if "values" in x:
            ingredient_refs(x["values"], out)
        elif "item" in x:
            ingredient_refs(x["item"], out)
        elif "tag" in x:
            out.append("#" + x["tag"])
        else:
            out.append("??" + json.dumps(x, sort_keys=True))


def recipes_rastro():
    """As receitas de tipo do vanilla da datagen (mesa de trabalho e forno): o que citam e o que dão. O teste confere que o port tem toda a que ficou completa."""
    out = {}
    for path in sorted(walk(f"{GEN}/data/mekanism/recipe")):
        recipe = json.load(open(path))
        kind = recipe["type"]
        refs = []
        if kind == "minecraft:crafting_shaped":
            for v in recipe["key"].values():
                ingredient_refs(v, refs)
        elif kind == "minecraft:crafting_shapeless":
            ingredient_refs(recipe["ingredients"], refs)
        elif kind in ("minecraft:smelting", "minecraft:blasting", "minecraft:smoking", "minecraft:campfire_cooking"):
            ingredient_refs(recipe["ingredient"], refs)
        else:
            continue
        result = recipe["result"]
        rid = result["id"] if isinstance(result, dict) else result
        components = sorted(result.get("components", {})) if isinstance(result, dict) else []
        rel = os.path.relpath(path, f"{GEN}/data")
        out[rel] = {"type": kind, "refs": sorted(set(refs)), "result": rid, "result_components": components}
    json.dump(out, open(f"{OUT}/craft_recipes.json", "w"), indent=0, sort_keys=True)
    print("craft_recipes.json:", len(out))


def lang_rastro():
    """O texto que a datagen do original dá a cada chave de `lang/en_us.json` que o port tem (o port tem um subconjunto, nos textos do original)."""
    original = json.load(open(f"{GEN}/assets/mekanism/lang/en_us.json"))
    ours = json.load(open(f"{CONTENT}/assets/mekanism/lang/en_us.json"))
    out = {}
    for key in ours:
        if key not in original:
            raise SystemExit(f"a chave {key} do port não está no original")
        out[key] = original[key]
    json.dump(out, open(f"{OUT}/craft_lang.json", "w"), indent=0, sort_keys=True, ensure_ascii=False)
    print("craft_lang.json:", len(out))


def creative_order():
    """A ordem de registro de `MekanismItems` (os campos na ordem do arquivo, o bloco `static` no lugar dele) e a de `MekanismBlocks`."""
    text = open(f"{JAVA}/MekanismItems.java").read()
    body = text[text.index("public static final ItemRegistryObject<ItemRobit> ROBIT"):]
    statements = re.split(r"\n\s*(?=public static final|static \{)", body)
    primary = ["IRON", "GOLD", "OSMIUM", "COPPER", "TIN", "LEAD", "URANIUM"]
    types = [("shard", "SHARD"), ("crystal", "CRYSTAL"), ("dust", "DUST"), ("dirty_dust", "DIRTY_DUST"), ("clump", "CLUMP"), ("ingot", "INGOT"), ("raw", "RAW"),
             ("nugget", "NUGGET"), ("enriched", "ENRICHED")]
    prefix_of = {kind: prefix for prefix, kind in types}
    misc = {"REFINED_OBSIDIAN": "refined_obsidian", "LAPIS_LAZULI": "lapis_lazuli"}
    order = []
    for s in statements:
        if s.startswith("static {"):
            for prefix, kind in types:
                for p in primary:
                    vanilla = p in ("IRON", "GOLD", "COPPER")
                    if kind != "ENRICHED" and (not vanilla or kind not in ("INGOT", "RAW", "NUGGET")):
                        order.append(f"{prefix}_{p.lower()}")
            continue
        header = s.split("=")[0]
        if "REFINED_GLOWSTONE_INGOT" in header:
            order.append("ingot_refined_glowstone")
        elif m := re.search(r'ITEMS\.(?:register|registerItem|registerSimple)\("([a-z_0-9]+)"', s):
            order.append(m.group(1))
        elif m := re.search(r'registerQIODrive\("([a-z_]+)"', s):
            order.append("qio_drive_" + m.group(1))
        elif m := re.search(r"registerInstaller\(.*?BaseTier\.([A-Z]+)\)", s):
            order.append(m.group(1).lower() + "_tier_installer")
        elif m := re.search(r"registerCircuit\(BaseTier\.([A-Z]+)\)", s):
            order.append(m.group(1).lower() + "_control_circuit")
        elif m := re.search(r"registerAlloy\(AlloyTier\.([A-Z]+)", s):
            order.append("alloy_" + m.group(1).lower())
        elif m := re.search(r"registerResource\(ResourceType\.([A-Z_]+), (?:Misc|Primary)Resource\.([A-Z_]+)\)", s):
            order.append(f"{prefix_of[m.group(1)]}_{misc.get(m.group(2), m.group(2).lower())}")
        elif s.startswith("public static final"):
            raise SystemExit("registro de item não entendido: " + s[:90])
    # `MekanismBlocks`: a ordem de registro que o original em jogo deu (`rastros/blocos.jsonl`, `testes/BlocosTraceTest.java`: `MekanismBlocks.BLOCKS.getPrimaryEntries()`), que é a da
    # aba criativa (`addToDisplay` sobre `BLOCKS.getSecondaryEntries()`): o bloco `static` (fábricas, blocos de recurso por `PrimaryResource`, minérios por `OreType`) vem antes dos campos.
    blocks = []
    with open(f"{OUT}/blocos.jsonl") as handle:
        for line in handle:
            blocks.append(json.loads(line)["id"].split(":")[1])
    json.dump({"items": order, "blocks": blocks}, open(f"{OUT}/craft_creative_order.json", "w"), indent=0)
    print("craft_creative_order.json:", len(order), len(blocks))


# As tags `c:` de item que o NeoForge define (o original as tem, o Horizon não) e que receitas da datagen citam: as dos tintes e dos painéis de vidro das receitas do traje
# de hazmat e do conta-gotas (`hazmat_*`, `gauge_dropper`), com as tags aninhadas delas.
NEOFORGE_TAGS = ["dyes/black", "dyes/orange", "glass_panes", "glass_panes/colorless"]
# E as das receitas de mesa dos blocos de máquina (frente `blocos-maquinas`): o diamante, o resíduo de resina, o balde vazio, o baú e o barril de madeira.
NEOFORGE_TAGS += ["storage_blocks/diamond", "clumps/resin", "buckets/empty", "chests/wooden", "barrels/wooden"]
# E as das receitas do disperso de pressão e dos dois transportadores especiais (frente `blocos-estrutura`): as barras, com as de cobre e de ferro (a aninhada `#minecraft:bars` o Horizon tem).
NEOFORGE_TAGS += ["bars", "bars/copper", "bars/iron"]


def neoforge_tags_rastro():
    out = {}
    for name in NEOFORGE_TAGS:
        out[f"c/tags/item/{name}.json"] = json.load(open(f"{NEOFORGE}/data/c/tags/item/{name}.json"))["values"]
    json.dump(out, open(f"{OUT}/craft_neoforge_tags.json", "w"), indent=0, sort_keys=True)
    print("craft_neoforge_tags.json:", len(out))


EQUIPMENT_IDS = [
    "atomic_disassembler", "canteen", "configuration_card", "configurator", "crafting_formula", "dictionary", "dosimeter", "electric_bow", "energy_tablet", "flamethrower",
    "free_runners", "free_runners_armored", "gauge_dropper", "geiger_counter", "hazmat_mask", "hazmat_gown", "hazmat_pants", "hazmat_boots", "hdpe_elytra", "jetpack",
    "jetpack_armored", "mekasuit_helmet", "mekasuit_bodyarmor", "mekasuit_pants", "mekasuit_boots", "meka_tool", "network_reader", "portable_qio_dashboard",
    "portable_teleporter", "robit", "scuba_mask", "scuba_tank", "seismic_reader", "basic_tier_installer", "advanced_tier_installer", "elite_tier_installer",
    "ultimate_tier_installer",
]
ITEM_DIR = f"{ORIGINAL}/src/main/java/mekanism/common/item"


def balanced(text, start):
    """O texto entre o parêntese que abre em `start` e o que o fecha."""
    depth = 0
    for i in range(start, len(text)):
        if text[i] == "(":
            depth += 1
        elif text[i] == ")":
            depth -= 1
            if depth == 0:
                return text[start + 1 : i]
    raise SystemExit("parêntese sem par")


def java_class(name):
    for dp, _, files in os.walk(ITEM_DIR):
        if f"{name}.java" in files:
            return open(os.path.join(dp, f"{name}.java")).read()
    return None


def constructor_chain(cls):
    """O corpo do `super(...)` do construtor de `cls` e, subindo, o das classes de item de que ela herda (`ItemChemicalArmor` dá a pilha e a raridade do jetpack)."""
    out = []
    while cls:
        text = java_class(cls)
        if text is None:
            break
        m = re.search(rf"\b{cls}\([^)]*\)\s*\{{\s*super\(", text)
        if m:
            out.append(balanced(text, m.end() - 1))
        sup = re.search(rf"class {cls}(?:<[^>]*>)?\s+extends\s+(\w+)", text)
        cls = sup.group(1) if sup else None
    return out


def equipment_rastro():
    """Para cada item de equipamento, o que `MekanismItems` e o construtor da classe dele dão às propriedades (`Item.Properties`), lido do texto do Java."""
    registry = open(f"{JAVA}/MekanismItems.java").read()
    statements = re.split(r";\n\s*(?://[^\n]*\n\s*)*(?=public static final|static \{|private static)", registry)
    startup = open(f"{ORIGINAL}/src/main/java/mekanism/common/config/MekanismStartupConfig.java").read()

    def default(name):
        m = re.search(rf'{name} = Cached\w+Value\.wrap\(this, [^;]*?defineInRange\("(\w+)", ([^;]*?)\)\);', startup, re.S)
        if not m:
            raise SystemExit(f"configuração {name} não achada")
        return m.group(2).split(",")[0].strip()

    items = {}
    for ident in EQUIPMENT_IDS:
        if ident.endswith("_tier_installer"):
            tier = ident.split("_")[0].upper()
            reg = next(s for s in statements if f"{tier}_TIER_INSTALLER" in s.split("=")[0])
            cls = "ItemTierInstaller"
            chain = constructor_chain(cls)
            entry = {"class": cls, "tier": tier.lower()}
        else:
            reg = next(s for s in statements if re.search(rf'"{ident}"', s) and "ITEMS." in s)
            cls = (re.search(r"new (Item\w+)\(", reg) or re.search(r"(Item\w+)::new", reg) or re.search(r"registerItem\(\"%s\", (Item\w+)" % ident, reg) or [None, None])[1]
            chain = constructor_chain(cls) if cls else []
            entry = {"class": cls}
        text = reg + " " + " ".join(chain)
        arg_type = re.search(r"ArmorType\.(\w+)", reg)
        stack = re.findall(r"stacksTo\((\d+)\)", text)
        rarity = re.findall(r"Rarity\.(\w+)", text)
        entry["stack"] = int(stack[-1]) if stack else None
        entry["rarity"] = rarity[-1].lower() if rarity else None
        entry["fire_resistant"] = "fireResistant" in text
        entry["enchantable"] = int(m.group(1)) if (m := re.search(r"\.enchantable\((\d+)\)", text)) else None
        entry["use_cooldown"] = float(m.group(1)) if (m := re.search(r"useCooldown\(([\d.]+)F\)", text)) else None
        entry["durability"] = int(m.group(1)) if (m := re.search(r"\.durability\((\d+)\)", text)) else None
        slot = re.search(r"Equippable\.builder\(EquipmentSlot\.(\w+)\)", text) or re.search(r"\.equippable\(EquipmentSlot\.(\w+)\)", text)
        if slot:
            entry["slot"] = slot.group(1).lower()
        elif "Equippable.builder(armorType.getSlot())" in text and arg_type:
            entry["slot"] = {"HELMET": "head", "CHESTPLATE": "chest", "LEGGINGS": "legs", "BOOTS": "feet"}[arg_type.group(1)]
        asset = re.search(r"setAsset\(MekanismEquipmentAssets\.(\w+)\)", text)
        if asset:
            entry["asset"] = asset.group(1).lower()
        elif "setAsset(markerAsset)" in text or "setAsset(MekanismEquipmentAssets" in text:
            entry["asset"] = re.search(r"MekanismEquipmentAssets\.(\w+)", reg).group(1).lower()
        if m := re.search(r"attributes\(MekanismArmorMaterials\.(\w+)\(", text):
            entry["attributes"] = m.group(1)
        entry["damage_on_hurt"] = not re.search(r"setDamageOnHurt\(false\)", text)
        entry["mob_visibility"] = None
        if m := re.search(r"new MobVisibility\([^,]*,\s*([\d.]+)F\)", text):
            entry["mob_visibility"] = float(m.group(1))
        repair = re.search(r"\.repairable\((\w+)\.value\(\)\)", text)
        entry["repairable"] = repair.group(1).lower() if repair else None
        entry["glider"] = "DataComponents.GLIDER" in text
        entry["equip_sound"] = re.search(r"setEquipSound\(SoundEvents\.(\w+)\)", text).group(1).lower() if re.search(r"setEquipSound", text) else None
        items[ident] = entry
    nether = open(next(os.path.join(dp, "ArmorMaterials.java") for dp, _, fs in os.walk(MC) if "ArmorMaterials.java" in fs)).read()
    m = re.search(r"ArmorMaterial NETHERITE = new ArmorMaterial\(\s*\d+, makeDefense\((\d+), (\d+), (\d+), (\d+), \d+\), \d+, [\w.]+, ([\d.]+)F, ([\d.]+)F", nether)
    boots, legs, chest, helmet, toughness, knockback = m.groups()
    armor = {
        "netherite": {"boots": int(boots), "leggings": int(legs), "chestplate": int(chest), "helmet": int(helmet), "toughness": float(toughness), "knockback": float(knockback)},
        "armored_free_runners": {"armor": int(default("armoredFreeRunnerArmor")), "toughness": float(default("armoredFreeRunnerToughness")), "knockback": float(default("armoredFreeRunnerKnockbackResistance"))},
        "armored_jetpack": {"armor": int(default("armoredJetpackArmor")), "toughness": float(default("armoredJetpackToughness")), "knockback": float(default("armoredJetpackKnockbackResistance"))},
        "mekasuit_uses_netherite_defaults": all(f"ArmorMaterials.NETHERITE.{x}" in startup for x in ("defense().get(ArmorType.HELMET)", "toughness()", "knockbackResistance()")),
    }
    # As receitas da datagen que dão um item de equipamento, com o tipo: o teste confere as de mesa (completas ou com a falta registrada) e os tipos que o port não lê.
    recipes = {}
    for path in sorted(walk(f"{GEN}/data/mekanism/recipe")):
        recipe = json.load(open(path))
        result = recipe.get("result")
        rid = result["id"] if isinstance(result, dict) else result
        if isinstance(rid, str) and rid.startswith("mekanism:") and rid[9:] in EQUIPMENT_IDS:
            recipes[os.path.relpath(path, f"{GEN}/data")] = recipe["type"]
    json.dump({"items": items, "armor": armor, "recipes": recipes}, open(f"{OUT}/equipment_items.json", "w"), indent=0, sort_keys=True)
    print("equipment_items.json:", len(items))


MC = "/mnt/dados/caches/rust-mekanism-ref/mc-sources/mc"


def estrutura_modelos():
    """O que o port reescreve em vez de copiar (`blocos_conteudo.py`, decisão 26), lido do original: `tests/blocos_content.rs` confere os arquivos do port contra isto."""
    tiers = "basic advanced elite ultimate".split()
    kinds = {"universal_cable": "small", "pressurized_tube": "small", "thermodynamic_conductor": "small", "mechanical_pipe": "large", "logistical_transporter": "large"}
    transmitters = {}
    for kind, size in kinds.items():
        for tier in tiers:
            model = json.load(open(f"{RES}/models/block/transmitter/{size}/{kind}/{tier}.json"))
            transmitters[f"{tier}_{kind}"] = {"size": size, "center": model["textures"]["center_opaque"], "side": model["textures"]["side_opaque"]}
    for kind in ("restrictive_transporter", "diversion_transporter"):
        model = json.load(open(f"{RES}/models/block/transmitter/large/{kind}.json"))
        transmitters[kind] = {"size": "large", "center": model["textures"]["center_opaque"], "side": model["textures"]["side_opaque"]}
    base = json.load(open(f"{RES}/models/block/energy_cube/base.json"))
    cube = {"textures": base["textures"], "frame": base["frame"], "leds": base["leds"], "ports": base["ports"], "tiers": {}, "display": {}}
    for tier in tiers + ["creative"]:
        cube["tiers"][tier] = json.load(open(f"{RES}/models/block/energy_cube/{tier}.json"))
        cube["display"][tier] = json.load(open(f"{RES}/models/item/{tier}_energy_cube.json"))["display"]
    tanks = {tier: json.load(open(f"{RES}/models/item/{tier}_fluid_tank.json"))["display"] for tier in tiers + ["creative"]}
    json.dump({"transmitters": transmitters, "energy_cube": cube, "fluid_tank_display": tanks}, open(f"{OUT}/estrutura_modelos.json", "w"), indent=0, sort_keys=True)
    print("estrutura_modelos.json:", len(transmitters))


def main():
    equipment_rastro()
    neoforge_tags_rastro()
    tags_rastro()
    recipes_rastro()
    lang_rastro()
    worldgen_rastro()
    creative_order()
    estrutura_modelos()
    manifest = {}
    for path in sorted(walk(CONTENT)):
        rel = os.path.relpath(path, CONTENT)
        if rel == "horizon-mod.toml" or rel.startswith(ADAPTED_PREFIXES):
            continue
        if rel.startswith("data/c/tags/") or rel.startswith("data/minecraft/tags/"):
            continue  # as tags têm o rastro próprio (copiadas ou adaptadas)
        original = original_for(rel)
        if original is None:
            continue
        if digest(path) != digest(original):
            continue  # o port tem outro conteúdo neste caminho (adaptado antes); o teste do assunto o cobre
        manifest[rel] = digest(original)
    json.dump(manifest, open(f"{OUT}/craft_datagen.json", "w"), indent=0, sort_keys=True)
    print("craft_datagen.json:", len(manifest))


if __name__ == "__main__":
    main()
