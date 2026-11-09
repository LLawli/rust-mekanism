#!/usr/bin/env python3
"""Gera os rastros da frente `itens-craft` (itens e blocos de craft e de recurso) a partir da datagen e do código do original.

Uso: craft_rastros.py [CHECKOUT_DO_MEKANISM] [CHECKOUT_DO_HORIZON_PIN]

O que sai, em `dev/referencia/rastros/`:
- `craft_datagen.json`: o hash (FNV-1a 64 e tamanho) de cada arquivo de `mods/mekanism/content` que o port copia do original (a datagen, `src/datagen/generated/mekanism`,
  ou os `src/main/resources` dele), para `tests/craft_content.rs` conferir byte a byte (JSON pelo texto canônico: chaves em ordem, sem espaço);
- `craft_tags.json`: os valores de cada tag `c:`, `minecraft:` e `mekanism:` da datagen que cita um id da frente;
- `craft_lang.json`: o texto da datagen para cada chave do `en_us.json` do port;
- `craft_worldgen.json`: os arquivos de worldgen da datagen (`worldgen/feature`, `placed_feature`, `neoforge/biome_modifier`) dos minérios e do sal;
- `craft_recipes.json`: as receitas `minecraft:*` da datagen que a frente leva (todos os ingredientes e o resultado existem) e as que ficam de fora, com o motivo;
- `craft_creative_order.json`: a ordem de registro de `MekanismItems` e de `MekanismBlocks`, que é a ordem da aba criativa.
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
    # `MekanismBlocks`: o bloco `static` (blocos de recurso por `PrimaryResource`, minérios por `OreType`) vem antes dos campos, que seguem a ordem do arquivo.
    blocks = ["block_osmium", "block_raw_osmium", "block_tin", "block_raw_tin", "block_lead", "block_raw_lead", "block_uranium", "block_raw_uranium"]
    for ore in ("tin", "osmium", "uranium", "fluorite", "lead"):
        blocks += [f"{ore}_ore", f"deepslate_{ore}_ore"]
    blocks += ["block_charcoal", "block_bronze", "block_steel", "block_fluorite", "block_refined_obsidian", "block_refined_glowstone", "teleporter_frame",
               "steel_casing", "enrichment_chamber", "crusher", "metallurgic_infuser", "energized_smelter", "basic_universal_cable", "advanced_universal_cable",
               "elite_universal_cable", "ultimate_universal_cable", "block_salt", "block_bio_fuel"]
    json.dump({"items": order, "blocks": blocks}, open(f"{OUT}/craft_creative_order.json", "w"), indent=0)
    print("craft_creative_order.json:", len(order), len(blocks))


def main():
    tags_rastro()
    recipes_rastro()
    lang_rastro()
    worldgen_rastro()
    creative_order()
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
