#!/usr/bin/env python3
"""Gera o conteúdo do Mekanism Generators sem função (frente `generators-conteudo`): os 23 blocos que o port ainda não tinha, os 3 itens simples, a aba criativa e o rastro dos arquivos.

Uso: generators_conteudo.py [CHECKOUT_DO_MEKANISM]

É o `blocos_conteudo.py` (os blocos do Mekanism) estendido ao Generators: lê `rastros/blocos_generators*.jsonl` (o que `testes/BlocosTraceTest.java`, o método `generators`, tirou
do original em jogo, no mesmo formato de `blocos.jsonl`), usa as mesmas funções de declaração de bloco, de posicionamento e de linha de jogo, e os arquivos do original (a
datagen, `src/datagen/generated/mekanismgenerators`, e os recursos, `src/generators/resources`). Escreve em `mods/mekanismgenerators`:

- `content/data/mekanismgenerators/horizon/{block,item}/<id>.json`: o bloco (o gabarito `minecraft:lodestone`, ou `minecraft:glass` nos dois blocos de vidro, as propriedades,
  as propriedades de estado, a forma de seleção e a de colisão) e os três itens;
- `content/data/mekanismgenerators/machinery_basis/placement/<id>.json`: como o bloco escolhe o `facing` ao ser posto;
- `content/assets/mekanismgenerators/{blockstates,items,models,textures}`: o estado de bloco, a definição do item, os modelos e as texturas, copiados do original;
- `content/data/mekanismgenerators/loot_table/blocks/<id>.json`: a tabela de drop da datagen;
- `content/assets/mekanismgenerators/lang/en_us.json`: os nomes e as chaves `description.*`;
- as tags de bloco e de item (as do Generators, as que ele acrescenta às do Mekanism e as do vanilla), as receitas de mesa e a de infusão do `hohlraum` que ficam completas;
- `content/data/mekanismgenerators/horizon/creative_tab/mekanismgenerators.json`, a aba do mod, e `native/creative_tabs_table.rs`, o que o original acrescenta às abas do vanilla;
- `native/block_rows_table.rs`: a tabela de linhas de jogo (luz do estado ativo, reação ao pistão, oclusão) que a injeção `block_rows` aplica;
- `dev/referencia/rastros/generators_datagen.json`: o hash dos arquivos copiados, as tags e as receitas da datagen e a ordem do registro, que os testes em Rust leem.

Idempotente: reescreve o que gera e confere o que copia (um arquivo que já existe e difere do original é erro).
"""
import json
import os
import re
import shutil
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import blocos_conteudo as bc  # noqa: E402  (lê sys.argv[1] como o checkout do original)

ROOT = bc.ROOT
ORIGINAL = bc.ORIGINAL
NS = "mekanismgenerators"
CONTENT = os.path.join(ROOT, "mods/mekanismgenerators/content")
MEKANISM_CONTENT = os.path.join(ROOT, "mods/mekanism/content")
NATIVE = os.path.join(ROOT, "mods/mekanismgenerators/native")
GEN = f"{ORIGINAL}/src/datagen/generated/mekanismgenerators"
RES = f"{ORIGINAL}/src/generators/resources"
OUT = os.path.join(HERE, "rastros")

# O `heat_generator` já existia, com o gabarito do forno (as quatro máquinas anteriores, Fase 5); o resto do registro de `GeneratorsBlocks` entra agora.
EXISTING = ["heat_generator"]
ITEMS = ["solar_panel", "hohlraum", "turbine_blade"]
FLUID_BUCKETS = ["bioethanol_bucket", "deuterium_bucket", "fusion_fuel_bucket", "tritium_bucket"]
# O original faz os dois blocos de vidro de `BlockTileGlass`: som, instrumento e `noOcclusion` do vidro. O gabarito do Horizon é o vidro do vanilla (`rule: same_block`).
GLASS = {"reactor_glass", "laser_focus_matrix"}
EXTRA_MODELS = [f"{NS}:block/wind_generator_blades"]
EXTRA_FILES = ["render/turbine.png"]
TEMPLATE_GLASS = "minecraft:glass"
PROPS_ITEM = {"template": "minecraft:iron_nugget"}


def ported(kind):
    folder = os.path.join(CONTENT, f"data/{NS}/horizon/{kind}")
    return {name[:-5] for name in os.listdir(folder)} if os.path.isdir(folder) else set()


def read_json(path):
    with open(path) as handle:
        return json.load(handle)


def fnv(data):
    h = 0xCBF29CE484222325
    for byte in data:
        h = ((h ^ byte) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}:{len(data)}"


def ryu(x):
    """Um `f64` como o `serde_json` o escreve (`ryu`): decimal de 1e-5 a 1e16, senão científico sem o zero à esquerda do expoente. `repr` do Python difere em 1e-5 a 1e-4 e no expoente."""
    r = repr(x)
    if "e" not in r:
        return r
    from decimal import Decimal

    mantissa, exponent = r.split("e")
    exponent = int(exponent)
    if -5 <= exponent < 16:
        text = format(Decimal(r), "f")
        return text if "." in text else text + ".0"
    return f"{mantissa}e{exponent}"


def canon(value):
    """O texto canônico de um JSON, o mesmo que `serde_json::to_string` dá a um `Value` (chaves em ordem, sem espaço): o que `tests/common/generators.rs` (`digest`) hasheia."""
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, ensure_ascii=False) + ":" + canon(v) for k, v in sorted(value.items())) + "}"
    if isinstance(value, list):
        return "[" + ",".join(canon(v) for v in value) + "]"
    if isinstance(value, float):
        return ryu(value)
    return json.dumps(value, ensure_ascii=False)


def digest(path):
    """O hash que os testes conferem: o JSON pelo texto canônico e o resto pelos bytes."""
    data = open(path, "rb").read()
    if path.endswith(".json"):
        data = canon(json.loads(data.decode())).encode()
    return fnv(data)


# --- blocos e itens ------------------------------------------------------------------------------------------------------------------------------------------


def block_rows():
    return bc.rows("blocos_generators")


def item_rows():
    out = {}
    with open(os.path.join(OUT, "blocos_generators_itens.jsonl")) as handle:
        for line in handle:
            row = json.loads(line)
            out[row["id"].split(":")[1]] = row
    return out


def write_blocks(by_id, scope):
    facts = []
    for block in scope:
        row = by_id[block]
        template = TEMPLATE_GLASS if block in GLASS else bc.TEMPLATE
        bc.write_json(os.path.join(CONTENT, f"data/{NS}/horizon/block/{block}.json"), bc.block_declaration(row, template))
        placement = bc.placement_declaration(row)
        target = os.path.join(CONTENT, f"data/{NS}/machinery_basis/placement/{block}.json")
        if placement:
            bc.write_json(target, placement)
        elif os.path.exists(target):
            os.remove(target)
        facts.append(bc.row_facts(row))
    return facts


def write_items(item_by_id):
    for item in ITEMS:
        row = item_by_id[item]
        assert row["rarity"] == "COMMON", item
        declaration = dict(PROPS_ITEM)
        if row["max_stack"] != 64:
            declaration["max_stack_size"] = row["max_stack"]
        bc.write_json(os.path.join(CONTENT, f"data/{NS}/horizon/item/{item}.json"), declaration)


def write_rows_table(facts, furnace=()):
    text = bc.rows_table(facts, furnace)
    text = text.replace(
        "// GERADO por `dev/referencia/blocos_conteudo.py` a partir de `rastros/blocos.jsonl` (`testes/BlocosTraceTest.java`, o original em jogo). Não edite à mão.",
        "// GERADO por `dev/referencia/generators_conteudo.py` a partir de `rastros/blocos_generators.jsonl` (`testes/BlocosTraceTest.java`, o original em jogo). Não edite à mão.",
    )
    text = text.replace("dos blocos de máquina e dos blocos avulsos com tile que o Horizon", "dos blocos do Mekanism Generators que o Horizon")
    text = text.replace("A linha da tabela de um bloco do Mekanism, pelo id.", "A linha da tabela de um bloco do Mekanism Generators, pelo id.")
    text = text.replace("A injeção `active_light` aplica a tabela", "A injeção `block_rows` aplica a tabela")
    with open(os.path.join(NATIVE, "block_rows_table.rs"), "w") as handle:
        handle.write(text)


# --- arquivos copiados --------------------------------------------------------------------------------------------------------------------------------------


def blockstate_source(block):
    for base in (f"{RES}/assets/{NS}/blockstates/{block}.json", f"{GEN}/assets/{NS}/blockstates/{block}.json"):
        if os.path.exists(base):
            return base
    raise SystemExit(f"{block}: sem blockstate no original")


def model_source(name):
    namespace, path = name.split(":")
    roots = (RES, GEN) if namespace == NS else (f"{ORIGINAL}/src/main/resources", f"{ORIGINAL}/src/datagen/generated/mekanism")
    for root in roots:
        candidate = f"{root}/assets/{namespace}/models/{path}.json"
        if os.path.exists(candidate):
            return candidate
    return None


def texture_source(name):
    namespace, path = name.split(":")
    root = RES if namespace == NS else f"{ORIGINAL}/src/main/resources"
    return f"{root}/assets/{namespace}/textures/{path}.png"


def assets_plan(scope):
    """Os arquivos de `assets/` que o conteúdo copia: por arquivo, o caminho no mod e a origem. Os modelos e as texturas do `mekanism:` entram só se o Mekanism do port não os tem."""
    files = {}
    models, textures = {}, set()

    def visit(name):
        if name in models or name.split(":")[0] not in (NS, "mekanism"):
            return
        if name.endswith(".obj"):
            # `neoforge:obj`: o `.obj`, o `.mtl` e as texturas que o `.mtl` cita (`map_Kd`).
            namespace, path = name.split(":")
            base = f"{RES}/assets/{namespace}/{path}" if namespace == NS else f"{ORIGINAL}/src/main/resources/assets/{namespace}/{path}"
            if not os.path.exists(base):
                raise SystemExit(f"{name} sem arquivo no original")
            models[name] = None
            files[(namespace, path)] = base
            mtl = base[:-4] + ".mtl"
            if os.path.exists(mtl):
                files[(namespace, path[:-4] + ".mtl")] = mtl
                for line in open(mtl):
                    if line.startswith("map_Kd") and not line.split()[1].startswith("#"):
                        raise SystemExit(f"{mtl} cita a textura {line.split()[1]}: trate")
            return
        source = model_source(name)
        if source is None:
            raise SystemExit(f"modelo {name} sem arquivo no original")
        models[name] = source
        more, tex = bc.refs(source)
        textures.update(tex)
        for item in more:
            visit(item)

    for block in scope:
        state = blockstate_source(block)
        files[(NS, f"blockstates/{block}.json")] = state
        item = f"{GEN}/assets/{NS}/items/{block}.json"
        files[(NS, f"items/{block}.json")] = item
        for source in (state, item):
            more, _ = bc.refs(source)
            for name in more:
                visit(name)
    # O que só o renderer do bloco de entidade lê (Fase 6): o modelo das pás do Wind Generator (`RenderWindGenerator`) e a textura da turbina (`ModelTurbine.TURBINE_TEXTURE`).
    for name in EXTRA_MODELS:
        visit(name)
    for relative in EXTRA_FILES:
        files[(NS, relative)] = f"{RES}/assets/{NS}/{relative}"
    for item_id in ITEMS:
        item = f"{GEN}/assets/{NS}/items/{item_id}.json"
        files[(NS, f"items/{item_id}.json")] = item
        more, _ = bc.refs(item)
        for name in more:
            visit(name)
    for name, source in models.items():
        if source is None:
            continue
        namespace, path = name.split(":")
        files[(namespace, f"models/{path}.json")] = source
    for texture in sorted(textures):
        namespace, path = texture.split(":")
        png = texture_source(texture)
        if not os.path.exists(png):
            raise SystemExit(f"textura {texture} sem arquivo no original")
        files[(namespace, f"textures/{path}.png")] = png
        if os.path.exists(png + ".mcmeta"):
            files[(namespace, f"textures/{path}.png.mcmeta")] = png + ".mcmeta"
    return files


def copy_assets(scope):
    plan = assets_plan(scope)
    copied = missing_other = 0
    placed = {}
    for (namespace, relative), source in sorted(plan.items()):
        if namespace == NS:
            target = os.path.join(CONTENT, "assets", namespace, relative)
        else:
            # Um arquivo do Mekanism: o do port, se já tem, tem de ser o do original; se não tem, o Generators não o leva (o Mekanism é dependência) e o gerador avisa.
            target = os.path.join(MEKANISM_CONTENT, "assets", namespace, relative)
            if not os.path.exists(target):
                missing_other += 1
                print(f"  falta no Mekanism: assets/{namespace}/{relative}")
                continue
            if open(target, "rb").read() != open(source, "rb").read():
                raise SystemExit(f"assets/{namespace}/{relative} no Mekanism difere do original")
            continue
        if os.path.exists(target):
            if open(target, "rb").read() != open(source, "rb").read():
                raise SystemExit(f"{relative} já existe e difere do original")
        else:
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copyfile(source, target)
            copied += 1
        placed[os.path.relpath(target, CONTENT)] = source
    print(f"assets: {len(plan)} arquivos, {copied} novos, {missing_other} do Mekanism que faltam")
    return placed


def copy_loot(scope):
    for block in scope:
        target = os.path.join(CONTENT, f"data/{NS}/loot_table/blocks/{block}.json")
        shutil.copyfile(f"{GEN}/data/{NS}/loot_table/blocks/{block}.json", target)
    print(f"loot_table: {len(scope)}")


def lang(by_id, item_by_id, all_blocks):
    path = os.path.join(CONTENT, f"assets/{NS}/lang/en_us.json")
    ours = read_json(path)
    original = read_json(f"{GEN}/assets/{NS}/lang/en_us.json")
    add = {}
    for block in all_blocks:
        add[f"block.{NS}.{block}"] = original[f"block.{NS}.{block}"]
        description = [c for c in by_id[block]["item"]["components"] if c.startswith("mekanism:description=")]
        assert len(description) == 1, block
        key = description[0].split("key='")[1].split("'")[0]
        add[key] = original[key]
    for item in ITEMS:
        add[f"item.{NS}.{item}"] = original[f"item.{NS}.{item}"]
    add[f"constants.{NS}.mod_name"] = original[f"constants.{NS}.mod_name"]
    out = dict(ours)
    for key, value in add.items():
        out[key] = value
    with open(path, "w") as handle:
        json.dump(out, handle, indent=2, ensure_ascii=False)
        handle.write("\n")
    print(f"lang: {len(add)} chaves")
    return add


# --- aba criativa -------------------------------------------------------------------------------------------------------------------------------------------


def creative(by_id, order):
    """A aba do mod (`GeneratorsCreativeTabs.GENERATORS`: os itens, depois os blocos, depois os baldes dos fluidos, a ordem de registro de cada registro) e as listas das abas do
    vanilla em `native/creative_tabs_table.rs` (`addToExistingTabs`: os geradores nos blocos funcionais, os blocos com comparador na redstone, os baldes nas ferramentas e o
    hohlraum e o painel solar nos ingredientes)."""
    tab = {"title": f"constants.{NS}.mod_name", "icon": f"{NS}:heat_generator",
           "items": [f"{NS}:{i}" for i in ITEMS + order + FLUID_BUCKETS]}
    bc.write_json(os.path.join(CONTENT, f"data/{NS}/horizon/creative_tab/{NS}.json"), tab)
    functional = ["heat_generator", "solar_generator", "advanced_solar_generator", "wind_generator", "bio_generator", "gas_burning_generator"]
    redstone = [b for b in order if by_id[b]["redstone_tab"]]
    ingredients = ["hohlraum", "solar_panel"]
    return {"minecraft:functional_blocks": functional, "minecraft:redstone_blocks": redstone, "minecraft:tools_and_utilities": FLUID_BUCKETS, "minecraft:ingredients": ingredients}, tab


def write_tabs_table(vanilla):
    lines = [
        "// O que o Mekanism Generators acrescenta às abas do vanilla (`GeneratorsCreativeTabs.addToExistingTabs`, o evento `BuildCreativeModeTabContentsEvent`): os itens ao fim de cada",
        "// aba, na ordem da lista dele. GERADO por `dev/referencia/generators_conteudo.py` (a ordem vem do original em jogo, `rastros/blocos_generators.jsonl`). O Horizon só deixa o mod",
        "// declarar a aba própria (`creative_tab/*.json`) e não conhece as abas do vanilla por mod: a injeção `creative_tabs` põe a lista no fim de cada aba do vanilla.",
        "",
        "/// Os itens do Generators que o original acrescenta a cada aba do vanilla, na ordem da lista do original.",
        f"pub const VANILLA_TABS: [(&str, &[&str]); {len(vanilla)}] = [",
    ]
    for tab, ids in vanilla.items():
        lines.append(f'    (\n        "{tab}",\n        &[')
        lines += [f'            "{NS}:{i}",' for i in ids]
        lines.append("        ],\n    ),")
    lines += [
        "];",
        "",
        "/// Acrescenta ao fim das `stacks` de uma aba do vanilla (`id`) o que o Generators põe nela, como a pilha que o Horizon escreve para um id. Devolve se mudou.",
        "pub fn edit_tab(id: &str, stacks: &mut Vec<String>) -> bool {",
        "    let Some((_, ids)) = VANILLA_TABS.iter().find(|(tab, _)| *tab == id) else {",
        "        return false;",
        "    };",
        "    stacks.extend(ids.iter().map(|id| format!(\"{{count:1,id:\\\"{id}\\\"}}\")));",
        "    true",
        "}",
        "",
    ]
    with open(os.path.join(NATIVE, "creative_tabs_table.rs"), "w") as handle:
        handle.write("\n".join(lines))


# --- tags -----------------------------------------------------------------------------------------------------------------------------------------------------


def datagen_tags():
    """Os arquivos de tag de bloco e de item da datagen do Generators (`c`, `minecraft`, `mekanism` e `mekanismgenerators`), por caminho relativo a `data/`."""
    tags = {}
    for namespace in ("c", "minecraft", "mekanism", NS):
        base = f"{GEN}/data/{namespace}/tags"
        for kind in ("item", "block"):
            folder = f"{base}/{kind}"
            if not os.path.isdir(folder):
                continue
            for dirpath, _, names in os.walk(folder):
                for name in names:
                    path = os.path.join(dirpath, name)
                    tags[f"{namespace}/tags/{os.path.relpath(path, base)}"] = [v if isinstance(v, str) else v["id"] for v in read_json(path)["values"]]
    return tags


def expand(tags, key, seen=()):
    """Os ids de uma tag da datagen com as tags aninhadas que a datagen também escreve resolvidas."""
    out = []
    kind = key.split("/")[2]
    for value in tags[key]:
        if value.startswith("#"):
            namespace, path = value[1:].split(":")
            nested = f"{namespace}/tags/{kind}/{path}.json"
            if nested in tags and nested not in seen:
                out.extend(expand(tags, nested, seen + (nested,)))
        else:
            out.append(value)
    return out


def sync_tags(own_blocks, own_items, mekanism_blocks, mekanism_items):
    """Cada tag da datagen que cita um bloco ou item do Generators vira arquivo no mod, com os ids que o port tem (o Mekanism e o vanilla contam como existentes), e o rastro diz o
    que a datagen lista e o port ainda não tem (`mekanism:pressure_disperser`, da frente `blocos-estrutura`). As tags do vanilla que o port achata ganham os ids novos."""
    tags = datagen_tags()
    all_blocks = own_blocks | mekanism_blocks
    all_items = own_items | mekanism_items
    report = {}
    written = 0
    for key, values in sorted(tags.items()):
        kind = key.split("/")[2]
        namespace = key.split("/")[0]
        mine = [v for v in values if v.startswith(f"{NS}:")]
        nested = [v for v in values if v.startswith(f"#{NS}:")]
        if not mine and not nested:
            continue
        have = all_blocks if kind == "block" else all_items
        kept, missing = [], []
        for value in values:
            if value.startswith("#"):
                ns, path = value[1:].split(":")
                nested_key = f"{ns}/tags/{kind}/{path}.json"
                # Uma tag aninhada vale se o Generators a escreve (a dele, `mekanismgenerators:`) ou se o Mekanism do port já a tem.
                if ns == NS or os.path.isfile(os.path.join(MEKANISM_CONTENT, "data", nested_key)):
                    kept.append(value)
                else:
                    missing.append(value)
            elif value in have or value.startswith("minecraft:"):
                kept.append(value)
            else:
                missing.append(value)
        target = os.path.join(CONTENT, "data", key)
        if namespace not in ("c", "minecraft", NS):
            # O Horizon não deixa um mod acrescentar a tag de outro mod (`data/mekanism/tags/..` num mod que não é o Mekanism: "a mod adds files in its own namespaces"), e dois mods
            # não dividem um namespace. A datagen do Generators acrescenta a `mekanism:structures` e a `mekanism:structures/common`; isso fica para o arquivo do Mekanism (frente
            # `blocos-estrutura`), e o rastro guarda o que a datagen lista.
            if os.path.exists(target):
                os.remove(target)
            report[key] = {"values": values, "missing": values, "pending": True}
            continue
        if namespace == "minecraft":
            # A tag que o port achata: o arquivo do Generators tem só os ids do Generators (as tags aninhadas do Generators resolvidas), e o do Mekanism os dele (o Horizon junta
            # as listas).
            kept = [v for v in dict.fromkeys(expand(tags, key)) if v.startswith(f"{NS}:") and v in have]
        if os.path.exists(target) and namespace != NS:
            current = read_json(target)["values"]
            merged = current + [v for v in kept if v not in current]
            if merged != current:
                bc.write_json(target, {"values": merged})
                written += 1
        else:
            bc.write_json(target, {"values": kept})
            written += 1
        report[key] = {"values": values, "missing": missing}
    print(f"tags: {written} escritas ou ampliadas")
    return report


NEOFORGE_JAR = bc.NEOFORGE_JAR
# As tags de item `c:` do NeoForge que as receitas do Generators citam e o port ainda não tinha (`#c:bars`, do `turbine_vent`): o arquivo é o do jar, byte a byte.
NEOFORGE_TAGS = ["bars", "bars/copper", "bars/iron"]


def extra_tags():
    import zipfile

    written = 0
    found = {}
    with zipfile.ZipFile(NEOFORGE_JAR) as jar:
        for name in NEOFORGE_TAGS:
            relative = f"data/c/tags/item/{name}.json"
            data = jar.read(relative)
            target = os.path.join(CONTENT, relative)
            if not os.path.exists(target):
                os.makedirs(os.path.dirname(target), exist_ok=True)
                with open(target, "wb") as handle:
                    handle.write(data)
                written += 1
            assert open(target, "rb").read() == data, relative
            found[relative] = fnv(canon(json.loads(data.decode())).encode())
    print(f"tags do NeoForge: {written} novas")
    return found


# --- receitas -----------------------------------------------------------------------------------------------------------------------------------------------


def recipe_refs(recipe):
    refs = []
    kind = recipe["type"]
    if kind == "minecraft:crafting_shaped":
        for v in recipe["key"].values():
            bc_refs(v, refs)
    elif kind == "minecraft:crafting_shapeless":
        bc_refs(recipe["ingredients"], refs)
    elif kind == "mekanism:metallurgic_infusing":
        bc_refs(recipe["item_input"]["ingredient"], refs)
        refs.append("##" + recipe["chemical_input"]["ingredient"])
    else:
        return None
    return refs


def bc_refs(x, out):
    if isinstance(x, str):
        out.append(x)
    elif isinstance(x, list):
        for y in x:
            bc_refs(y, out)
    elif isinstance(x, dict):
        if "values" in x:
            bc_refs(x["values"], out)
        elif "item" in x:
            bc_refs(x["item"], out)
        elif "tag" in x:
            out.append("#" + x["tag"])
        else:
            out.append("??" + json.dumps(x, sort_keys=True))


def sync_recipes(own, vanilla):
    """Cada receita de mesa da datagen do Generators, e a de infusão do `hohlraum`, que ficou completa com os itens, blocos e tags do port vira arquivo (cópia, no caminho da
    datagen: o id da receita é o do original). Completa: cada item e tag citada existe no port (os dois mods) ou no vanilla, e o resultado também."""
    vanilla_items, vanilla_tags = set(vanilla["items"]), set(vanilla["item_tags"])
    mekanism_items = {f"mekanism:{n[:-5]}" for kind in ("item", "block") for n in os.listdir(os.path.join(MEKANISM_CONTENT, f"data/mekanism/horizon/{kind}"))}
    items = own | mekanism_items

    def exists(entry):
        if entry.startswith("##"):
            tag = entry[2:]
            # A tag de químico (`#mekanism:carbon`): o port ainda não tem químicos, e a receita de infusão do hohlraum a cita como a do original.
            return True
        if entry.startswith("#"):
            namespace, rest = entry[1:].split(":")
            if namespace == "minecraft":
                return entry[1:] in vanilla_tags
            return any(os.path.isfile(os.path.join(root, f"data/{namespace}/tags/item/{rest}.json")) for root in (CONTENT, MEKANISM_CONTENT))
        if entry.startswith("minecraft:"):
            return entry in vanilla_items
        return entry in items

    out = {}
    for dirpath, _, names in sorted(os.walk(f"{GEN}/data/{NS}/recipe")):
        for name in sorted(names):
            path = os.path.join(dirpath, name)
            recipe = read_json(path)
            refs = recipe_refs(recipe)
            if refs is None:
                continue
            result = recipe["result"] if "result" in recipe else recipe["output"]
            rid = result["id"] if isinstance(result, dict) else result
            components = sorted(result.get("components", {})) if isinstance(result, dict) else []
            rel = os.path.relpath(path, f"{GEN}/data")
            out[rel] = {"type": recipe["type"], "refs": sorted(set(refs)), "result": rid, "result_components": components,
                        "complete": all(exists(r) for r in refs) and exists(rid) and not components}
    written = 0
    for rel, info in sorted(out.items()):
        target = os.path.join(CONTENT, "data", rel)
        if info["complete"]:
            if not os.path.exists(target):
                os.makedirs(os.path.dirname(target), exist_ok=True)
                shutil.copyfile(f"{GEN}/data/{rel}", target)
                written += 1
    # A receita do Heat Generator estava escrita à mão em `recipe/heat_generator.json` (o id `mekanismgenerators:heat_generator`); o original a chama `generator/heat`. É o mesmo
    # arquivo, e a cópia da datagen a substitui: duas receitas dariam duas entradas no livro de receitas.
    superseded = os.path.join(CONTENT, f"data/{NS}/recipe/heat_generator.json")
    if os.path.exists(superseded) and os.path.exists(os.path.join(CONTENT, f"data/{NS}/recipe/generator/heat.json")):
        os.remove(superseded)
    print(f"receitas: {written} novas, {sum(1 for i in out.values() if i['complete'])} completas de {len(out)}")
    return out


# --- rastro -------------------------------------------------------------------------------------------------------------------------------------------------


def main():
    by_id = block_rows()
    item_by_id = item_rows()
    order = list(by_id)
    assert order[0] == "heat_generator" and len(order) == 24, order
    scope = [b for b in order if b not in EXISTING]
    facts = write_blocks(by_id, scope)
    write_items(item_by_id)
    # O `heat_generator` segue com o gabarito do forno: só a reação ao pistão e a oclusão do original entram (a segunda tabela, `FURNACE`).
    write_rows_table(facts, [bc.row_facts(by_id[block]) for block in EXISTING])
    print(f"blocos: {len(scope)}, itens: {len(ITEMS)}")
    placed = copy_assets(scope)
    copy_loot(scope)
    add = lang(by_id, item_by_id, order)
    vanilla, tab = creative(by_id, order)
    write_tabs_table(vanilla)
    own_blocks = {f"{NS}:{b}" for b in ported("block")}
    own_items = own_blocks | {f"{NS}:{i}" for i in ported("item")}
    mekanism_blocks = {f"mekanism:{n[:-5]}" for n in os.listdir(os.path.join(MEKANISM_CONTENT, "data/mekanism/horizon/block"))}
    mekanism_items = {f"mekanism:{n[:-5]}" for n in os.listdir(os.path.join(MEKANISM_CONTENT, "data/mekanism/horizon/item"))} | mekanism_blocks
    tags = sync_tags(own_blocks, own_items, mekanism_blocks, mekanism_items)
    vanilla_ids = read_json(os.path.join(OUT, "horizon_vanilla_ids.json"))
    neoforge = extra_tags()
    recipes = sync_recipes(own_items, vanilla_ids)
    # O rastro: o hash dos arquivos que o port copia do original, as tags e as receitas da datagen, a lang e a ordem do registro.
    manifest = {}
    for rel, source in sorted(placed.items()):
        target = os.path.join(CONTENT, rel)
        if digest(target) == digest(source):
            manifest[rel] = digest(source)
    for key in tags:
        target = os.path.join(CONTENT, "data", key)
        original = f"{GEN}/data/{key}"
        if os.path.exists(target) and digest(target) == digest(original):
            manifest[os.path.relpath(target, CONTENT)] = digest(original)
    for rel, info in recipes.items():
        target = os.path.join(CONTENT, "data", rel)
        if os.path.exists(target):
            assert digest(target) == digest(f"{GEN}/data/{rel}"), rel
            manifest[os.path.relpath(target, CONTENT)] = digest(target)
    for block in scope:
        rel = f"data/{NS}/loot_table/blocks/{block}.json"
        manifest[rel] = digest(f"{GEN}/{rel}")
    rastro = {
        "files": manifest,
        "neoforge_tags": neoforge,
        "tags": tags,
        "recipes": recipes,
        "lang": add,
        "order": {"blocks": order, "items": ITEMS, "buckets": FLUID_BUCKETS},
        "creative": {"tab": tab, "vanilla": vanilla},
    }
    with open(os.path.join(OUT, "generators_datagen.json"), "w") as handle:
        json.dump(rastro, handle, indent=1, sort_keys=True, ensure_ascii=False)
        handle.write("\n")
    print(f"generators_datagen.json: {len(manifest)} arquivos")


if __name__ == "__main__":
    main()
