#!/usr/bin/env python3
"""Gera o conteúdo dos 22 fluidos do Mekanism e do Mekanism Generators (bloco de líquido, balde, modelos, texturas, nome, tags).

O original registra cada fluido com `FluidDeferredRegister` (`MekanismFluids`, `GeneratorsFluids`): o fluido parado e o que corre, o
`LiquidBlock`, o `BucketItem` e o `FluidType`. O Horizon não tem fluido de mod no mundo (o motor só conhece água e lava, ver
`docs/injecoes-horizon.md`, H47), então o port os faz como bloco com a propriedade `level` do `LiquidBlock`, desenhado por modelo, com o
fluxo na cola (`machinery_basis`). Este script escreve só arquivos de conteúdo, a partir da lista abaixo (a tabela do Java) e da datagen
do original; o resultado é versionado, e `mods/mekanism/tests/fluids.rs` o compara com o rastro do Java e com a datagen.

    scripts/gerar_fluidos.py [--original /mnt/dados/caches/rust-mekanism-ref/mekanism]

Só biblioteca padrão do Python (lê e grava PNG sozinho, como `dev/referencia/comparar_tela.py`).
"""
import argparse
import json
import os
import shutil
import struct
import sys
import zlib

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(RAIZ, "dev", "referencia"))
from comparar_tela import ler_png  # noqa: E402

BRANCO = 0xFFFFFFFF
LUZ_MAXIMA = 15  # `Level.MAX_BRIGHTNESS`

# (mod, nome, cor ARGB do `FluidTypeRenderProperties.tint`, luz do `FluidType`, textura parada, textura que corre)
# Cada linha é uma chamada de `MekanismFluids` / `GeneratorsFluids`, e a cor e a luz de `registerLiquidChemical` vêm de
# `ChemicalConstants` / `GeneratorsChemicalConstants`. A textura é a do `FluidTypeRenderProperties`: a padrão é
# `mek_liquid/liquid`, o vapor tem a sua (`mek_liquid/steam`), e a água pesada usa a da água do vanilla.
LIQUIDO = ("mekanism:mek_liquid/liquid", "mekanism:mek_liquid/liquid_flow")
VAPOR = ("mekanism:mek_liquid/steam", "mekanism:mek_liquid/steam_flow")
AGUA = ("minecraft:block/water_still", "minecraft:block/water_flow")
FLUIDOS = [
    ("mekanism", "hydrogen", BRANCO, 0, LIQUIDO),
    ("mekanism", "oxygen", 0xFF6CE2FF, 0, LIQUIDO),
    ("mekanism", "chlorine", 0xFFCFE800, 0, LIQUIDO),
    ("mekanism", "sulfur_dioxide", 0xFFA99D90, 0, LIQUIDO),
    ("mekanism", "sulfur_trioxide", 0xFFCE6C6C, 0, LIQUIDO),
    ("mekanism", "sulfuric_acid", 0xFF82802B, 0, LIQUIDO),
    ("mekanism", "hydrogen_chloride", 0xFFA8F1E9, 0, LIQUIDO),
    ("mekanism", "hydrofluoric_acid", 0xFFC6C7BD, 0, LIQUIDO),
    ("mekanism", "uranium_oxide", 0xFFE1F573, LUZ_MAXIMA, LIQUIDO),
    ("mekanism", "uranium_hexafluoride", 0xFF809960, 0, LIQUIDO),
    ("mekanism", "ethene", 0xFFEACCF9, 0, LIQUIDO),
    ("mekanism", "sodium", 0xFFE9FEF4, 0, LIQUIDO),
    ("mekanism", "superheated_sodium", 0xFFD19469, LUZ_MAXIMA, LIQUIDO),
    ("mekanism", "brine", 0xFFFEEF9C, 0, LIQUIDO),
    ("mekanism", "lithium", 0xFFEBA400, 0, LIQUIDO),
    ("mekanism", "steam", BRANCO, 0, VAPOR),
    ("mekanism", "heavy_water", 0xFF0D1455, 0, AGUA),
    ("mekanism", "nutritional_paste", 0xFFEB6CA3, 0, LIQUIDO),
    ("mekanismgenerators", "bioethanol", 0xFFCEEB3D, LUZ_MAXIMA, LIQUIDO),
    ("mekanismgenerators", "deuterium", 0xFFFF3232, 0, LIQUIDO),
    ("mekanismgenerators", "fusion_fuel", 0xFF7E007D, 0, LIQUIDO),
    ("mekanismgenerators", "tritium", 0xFF64FF70, 0, LIQUIDO),
]

# `MapColor.MATERIAL_COLORS` (id, cor), de `MapColor.java`, para o `getClosestColor` do `FluidDeferredRegister`.
MAP_CORES = [
    (1, 8368696), (2, 16247203), (3, 13092807), (4, 16711680), (5, 10526975), (6, 10987431), (7, 31744), (8, 16777215),
    (9, 10791096), (10, 9923917), (11, 7368816), (12, 4210943), (13, 9402184), (14, 16776437), (15, 14188339),
    (16, 11685080), (17, 6724056), (18, 15066419), (19, 8375321), (20, 15892389), (21, 5000268), (22, 10066329),
    (23, 5013401), (24, 8339378), (25, 3361970), (26, 6704179), (27, 6717235), (28, 10040115), (29, 1644825),
    (30, 16445005), (31, 6085589), (32, 4882687), (33, 55610), (34, 8476209), (35, 7340544), (36, 13742497),
    (37, 10441252), (38, 9787244), (39, 7367818), (40, 12223780), (41, 6780213), (42, 10505550), (43, 3746083),
    (44, 8874850), (45, 5725276), (46, 8014168), (47, 4996700), (48, 4993571), (49, 5001770), (50, 9321518),
    (51, 2430480), (52, 12398641), (53, 9715553), (54, 6035741), (55, 1474182), (56, 3837580), (57, 5647422),
    (58, 1356933), (59, 6579300), (60, 14200723), (61, 8365974),
]

# A máscara `neoforge:item/mask/bucket_fluid` (16 por 16): as linhas e as colunas do líquido dentro do balde.
MASCARA_BALDE = {3: range(4, 12), 4: range(3, 13), 5: range(5, 11)}

ALTURAS = range(1, 10)  # nonos de bloco: 1/9 a 8/9 e o bloco cheio (9/9)
NIVEIS = range(16)  # `LiquidBlock.LEVEL`


def cor_mais_proxima(tint):
    """`FluidDeferredRegister.getClosestColor`: o id do `MapColor` mais próximo da cor, 0 (`NONE`) para o branco."""
    if tint == BRANCO:
        return 0
    r, g, b = (tint >> 16) & 255, (tint >> 8) & 255, tint & 255
    melhor, distancia_minima = 0, float("inf")
    for ident, cor in MAP_CORES:
        tr, tg, tb = (cor >> 16) & 255, (cor >> 8) & 255, cor & 255
        media = (r + tr) >> 1
        dr, dg, db = r - tr, g - tg, b - tb
        distancia = (((512 + media) * dr * dr) >> 8) + 4 * dg * dg + (((767 - media) * db * db) >> 8)
        if distancia < distancia_minima:
            melhor, distancia_minima = ident, distancia
    return melhor


def gravar_png(caminho, largura, altura, rgba):
    def bloco(tipo, corpo):
        return struct.pack(">I", len(corpo)) + tipo + corpo + struct.pack(">I", zlib.crc32(tipo + corpo) & 0xFFFFFFFF)

    linhas = b"".join(b"\x00" + bytes(rgba[y * largura * 4 : (y + 1) * largura * 4]) for y in range(altura))
    dados = (
        b"\x89PNG\r\n\x1a\n"
        + bloco(b"IHDR", struct.pack(">IIBBBBB", largura, altura, 8, 6, 0, 0, 0))
        + bloco(b"IDAT", zlib.compress(linhas, 9))
        + bloco(b"IEND", b"")
    )
    os.makedirs(os.path.dirname(caminho), exist_ok=True)
    with open(caminho, "wb") as arquivo:
        arquivo.write(dados)


def tingir(rgba, tint):
    """Multiplica o RGB de cada pixel pela cor, como o vértice tingido do original (o alfa fica)."""
    tr, tg, tb = (tint >> 16) & 255, (tint >> 8) & 255, tint & 255
    saida = bytearray(rgba)
    for i in range(0, len(saida), 4):
        saida[i] = (saida[i] * tr + 127) // 255
        saida[i + 1] = (saida[i + 1] * tg + 127) // 255
        saida[i + 2] = (saida[i + 2] * tb + 127) // 255
    return saida


def escrever(caminho, texto):
    os.makedirs(os.path.dirname(caminho), exist_ok=True)
    with open(caminho, "w", encoding="utf-8") as arquivo:
        arquivo.write(texto)


def json_texto(valor, fim=""):
    return json.dumps(valor, indent=2, ensure_ascii=False) + fim


def altura_do_estado(nivel, em_cima):
    """A altura, em nonos de bloco, de um estado do `LiquidBlock`: `FluidState.getHeight` (1 com o mesmo fluido acima, senão `getOwnHeight`)."""
    if em_cima:
        return 9
    if 1 <= nivel <= 7:
        return 8 - nivel
    return 8


def modelo_pai(k):
    """`mekanism:block/liquid_h<k>`: um cubo de k/9 de altura, com a textura que corre nos lados e a parada em cima e embaixo."""
    topo = 16 * k / 9
    topo = 16 if k == 9 else round(topo, 4)
    lado_v0 = round((16 - topo) / 2, 4)
    lado = {"uv": [0, lado_v0, 8, 8], "texture": "#flow"}
    cima = {"uv": [0, 0, 16, 16], "texture": "#still"}
    if k == 9:
        cima["cullface"] = "up"
    return {
        "textures": {"particle": "#still"},
        "elements": [
            {
                "from": [0, 0, 0],
                "to": [16, topo, 16],
                "faces": {
                    "down": {"uv": [0, 0, 16, 16], "texture": "#still", "cullface": "down"},
                    "up": cima,
                    "north": dict(lado, cullface="north"),
                    "south": dict(lado, cullface="south"),
                    "west": dict(lado, cullface="west"),
                    "east": dict(lado, cullface="east"),
                },
            }
        ],
    }


def estados_do_bloco(nome, mod):
    variantes = {}
    for nivel in NIVEIS:
        for em_cima in (False, True):
            chave = f"level={nivel},up={'true' if em_cima else 'false'}"
            variantes[chave] = {"model": f"{mod}:block/{nome}_h{altura_do_estado(nivel, em_cima)}"}
    return {"variants": variantes}


def definicao_do_bloco(tint):
    # O `LiquidBlock` do original não emite luz, nem o do fluido com `FluidType.lightLevel` 15 (rastro `fluids.jsonl`, `lightEmission_world`).
    propriedades = {
        "hardness": 100.0,
        "explosion_resistance": 100.0,
        "render_shape": "MODEL",
        "occludes": False,
    }
    mapa = cor_mais_proxima(tint)
    if mapa:
        propriedades["map_color"] = mapa
    return {
        "template": "minecraft:structure_void",
        "item": False,
        "properties": propriedades,
        "state_properties": {
            "level": {"type": "enum", "values": [str(n) for n in NIVEIS]},
            "up": {"type": "boolean"},
        },
        "shapes": {"default": []},
        "collision_shapes": {"default": []},
    }


def definicao_do_fluido(mod, nome, tint, luz):
    """O que o `FluidDeferredRegister` guarda no fluido, no tipo e no balde, para a cola (`machinery_basis_core::liquid_data`)."""
    return {
        "block": f"{mod}:{nome}",
        "bucket": f"{mod}:{nome}_bucket",
        "tick_delay": 5,
        "slope_find_distance": 4,
        "drop_off": 1,
        "can_convert_to_source": False,
        "light": luz,
        "color": tint & 0xFFFFFF,
        "fill_sound": "minecraft:item.bucket.fill",
        "empty_sound": "minecraft:item.bucket.empty",
        **({"drink_when_hungry": True} if nome == "nutritional_paste" else {}),
    }


# `ItemNutritionalPasteBucket`: `Consumables.defaultDrink()` sem partículas, `usingConvertsTo(Items.BUCKET)` e a `PasteBucketConsumption`, que dá
# `FluidType.BUCKET_VOLUME / nutritionalPasteMBPerFood` de fome (1000 / 50 = 20) e `nutritionalPasteSaturation` (0,8) da configuração. O `minecraft:food`
# com nutrição 0 faz o que o Horizon precisa dele (bebe só com fome, `canEat(false)`, e toca o som e o arroto como o `FoodProperties.onConsume` em que a
# `PasteBucketConsumption` se baseia), e o efeito de consumo `mekanism:paste` dá a fome e a saturação lidas da configuração (`consume_effect`, em
# `mods/mekanism/native/glue.rs`). O componente `mekanism:paste_consumption` é só o marcador desse comportamento no original. Os componentes saem do
# rastro (`bucket.defaultStack.components`).
PASTA_NUTRITIVA = {
    "minecraft:food": {"nutrition": 0, "saturation": 0.0},
    "minecraft:consumable": {
        "animation": "drink",
        "sound": "minecraft:entity.generic.drink",
        "has_consume_particles": False,
        "on_consume_effects": [{"type": "mekanism:paste"}],
    },
    "minecraft:use_remainder": {"id": "minecraft:bucket"},
}


def definicao_do_balde(nome):
    """`itemProperties.stacksTo(1).craftRemainder(Items.BUCKET)` (`FluidDeferredRegister`): o balde é um item simples, o uso é da cola."""
    definicao = {"template": "minecraft:iron_nugget", "max_stack_size": 1, "crafting_remainder": "minecraft:bucket"}
    if nome == "nutritional_paste":
        definicao["components"] = PASTA_NUTRITIVA
    return definicao


def textura_cozida(origem, destino, tint):
    largura, altura, rgba = ler_png(origem)
    gravar_png(destino, largura, altura, tingir(rgba, tint))
    shutil.copyfile(origem + ".mcmeta", destino + ".mcmeta")


def balde_mascarado(origem, destino):
    """Cada quadro do líquido, só nos pixels que a máscara do balde cobre: a camada de fluido do `DynamicFluidContainerModel`."""
    largura, altura, rgba = ler_png(origem)
    saida = bytearray(len(rgba))
    for quadro in range(altura // largura):
        for y, colunas in MASCARA_BALDE.items():
            for x in colunas:
                i = ((quadro * largura + y) * largura + x) * 4
                saida[i : i + 4] = rgba[i : i + 4]
    gravar_png(destino, largura, altura, saida)
    shutil.copyfile(origem + ".mcmeta", destino + ".mcmeta")


def acrescentar_lang(caminho, entradas):
    with open(caminho, encoding="utf-8") as arquivo:
        texto = arquivo.read()
    termina_em_nova_linha = texto.endswith("\n")
    dados = json.loads(texto)
    dados.update(entradas)
    escrever(caminho, json_texto(dados, "\n" if termina_em_nova_linha else ""))


def principal():
    analise = argparse.ArgumentParser()
    analise.add_argument("--original", default="/mnt/dados/caches/rust-mekanism-ref/mekanism")
    argumentos = analise.parse_args()
    gerado = os.path.join(argumentos.original, "src/datagen/generated")
    recursos = os.path.join(argumentos.original, "src/main/resources/assets/mekanism/textures/mek_liquid")
    mods = os.path.join(RAIZ, "mods")
    conteudo = {"mekanism": os.path.join(mods, "mekanism/content"), "mekanismgenerators": os.path.join(mods, "mekanismgenerators/content")}

    # Texturas do original, copiadas como estão (o port as tem na mesma pasta do atlas), e a camada de líquido do balde.
    destino_texturas = os.path.join(conteudo["mekanism"], "assets/mekanism/textures/mek_liquid")
    for nome in ("liquid", "liquid_flow", "steam", "steam_flow"):
        for sufixo in (".png", ".png.mcmeta"):
            os.makedirs(destino_texturas, exist_ok=True)
            shutil.copyfile(os.path.join(recursos, nome + sufixo), os.path.join(destino_texturas, nome + sufixo))
    balde_mascarado(
        os.path.join(recursos, "liquid.png"),
        os.path.join(conteudo["mekanism"], "assets/mekanism/textures/item/bucket_fluid.png"),
    )
    for k in ALTURAS:
        escrever(os.path.join(conteudo["mekanism"], f"assets/mekanism/models/block/liquid_h{k}.json"), json_texto(modelo_pai(k)))

    lang = {mod: {} for mod in conteudo}
    for mod, nome, tint, luz, (parada, corre) in FLUIDOS:
        base = conteudo[mod]
        escrever(os.path.join(base, f"data/{mod}/horizon/block/{nome}.json"), json_texto(definicao_do_bloco(tint)))
        escrever(os.path.join(base, f"data/{mod}/horizon/item/{nome}_bucket.json"), json_texto(definicao_do_balde(nome)))
        escrever(os.path.join(base, f"data/{mod}/machinery_basis/liquid/{nome}.json"), json_texto(definicao_do_fluido(mod, nome, tint, luz)))
        escrever(os.path.join(base, f"assets/{mod}/blockstates/{nome}.json"), json_texto(estados_do_bloco(nome, mod)))
        if tint != BRANCO and parada.startswith("mekanism:"):
            # O Horizon não tinge o modelo de um bloco: a cor entra no pixel.
            parada = f"{mod}:mek_liquid/{nome}"
            corre = f"{mod}:mek_liquid/{nome}_flow"
            textura_cozida(os.path.join(recursos, "liquid.png"), os.path.join(base, f"assets/{mod}/textures/mek_liquid/{nome}.png"), tint)
            textura_cozida(
                os.path.join(recursos, "liquid_flow.png"), os.path.join(base, f"assets/{mod}/textures/mek_liquid/{nome}_flow.png"), tint
            )
        for k in ALTURAS:
            escrever(
                os.path.join(base, f"assets/{mod}/models/block/{nome}_h{k}.json"),
                json_texto({"parent": f"mekanism:block/liquid_h{k}", "textures": {"still": parada, "flow": corre}}),
            )
        escrever(
            os.path.join(base, f"assets/{mod}/models/item/{nome}_bucket.json"),
            json_texto(
                {
                    "parent": "minecraft:item/generated",
                    "textures": {"layer0": "minecraft:item/bucket", "layer1": "mekanism:item/bucket_fluid"},
                }
            ),
        )
        escrever(
            os.path.join(base, f"assets/{mod}/items/{nome}_bucket.json"),
            json_texto(
                {
                    "model": {
                        "type": "minecraft:model",
                        "model": f"{mod}:item/{nome}_bucket",
                        "tints": [
                            {"type": "minecraft:constant", "value": 0xFFFFFF},
                            {"type": "minecraft:constant", "value": tint & 0xFFFFFF},
                        ],
                    }
                }
            ),
        )
        # A tag `c:buckets/<nome>` é o arquivo da datagen, byte a byte.
        origem_tag = os.path.join(gerado, mod, f"data/c/tags/item/buckets/{nome}.json")
        destino_tag = os.path.join(base, f"data/c/tags/item/buckets/{nome}.json")
        os.makedirs(os.path.dirname(destino_tag), exist_ok=True)
        shutil.copyfile(origem_tag, destino_tag)
        original = json.load(open(os.path.join(gerado, mod, f"assets/{mod}/lang/en_us.json"), encoding="utf-8"))
        lang[mod][f"block.{mod}.{nome}"] = original[f"block.{mod}.{nome}"]
        lang[mod][f"item.{mod}.{nome}_bucket"] = original[f"item.{mod}.{nome}_bucket"]

    for mod, entradas in lang.items():
        acrescentar_lang(os.path.join(conteudo[mod], f"assets/{mod}/lang/en_us.json"), entradas)

    # `minecraft:replaceable` lista os blocos de líquido (`BaseTagProvider`), e as duas tags de entidade `c:` que só citam o vanilla.
    for mod in conteudo:
        destino = os.path.join(conteudo[mod], "data/minecraft/tags/block")
        os.makedirs(destino, exist_ok=True)
        shutil.copyfile(os.path.join(gerado, mod, "data/minecraft/tags/block/replaceable.json"), os.path.join(destino, "replaceable.json"))
    destino = os.path.join(conteudo["mekanism"], "data/c/tags/entity_type")
    os.makedirs(destino, exist_ok=True)
    for nome in ("creepers", "endermen"):
        shutil.copyfile(os.path.join(gerado, "mekanism/data/c/tags/entity_type", nome + ".json"), os.path.join(destino, nome + ".json"))


if __name__ == "__main__":
    principal()
