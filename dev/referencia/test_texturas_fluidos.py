"""As texturas dos fluidos são as do original: a de cada fluido tingido é a `liquid.png` (e a `liquid_flow.png`) com a cor do fluido multiplicada
no RGB, e a camada de líquido do balde é a `liquid.png` só nos pixels que a máscara `neoforge:item/mask/bucket_fluid` cobre.

O Horizon não tinge o modelo de um bloco (as fontes de cor do mesher são por nome de bloco), então a cor entra no pixel; o original tinge o vértice
com `FluidTintSources.constant` (`ClientRegistrationUtil.registerFluidModels`). A cor sai do rastro do Java (`rastros/fluids.jsonl`, campo
`type.color`), e o teste não lê o script que gerou os arquivos.
"""
import json
import os
import sys
import unittest

AQUI = os.path.dirname(os.path.abspath(__file__))
RAIZ = os.path.abspath(os.path.join(AQUI, "..", ".."))
sys.path.insert(0, AQUI)
from comparar_tela import ler_png  # noqa: E402

BRANCO = 0xFFFFFFFF
MASCARA = {3: range(4, 12), 4: range(3, 13), 5: range(5, 11)}


def conteudo(mod):
    return os.path.join(RAIZ, "mods", mod, "content", "assets", mod, "textures")


def rastro():
    caminho = os.path.join(AQUI, "rastros", "fluids.jsonl")
    with open(caminho, encoding="utf-8") as arquivo:
        return [json.loads(linha) for linha in arquivo if linha.strip()]


def tingido(rgba, cor):
    tr, tg, tb = (cor >> 16) & 255, (cor >> 8) & 255, cor & 255
    saida = bytearray(rgba)
    for i in range(0, len(saida), 4):
        saida[i] = (saida[i] * tr + 127) // 255
        saida[i + 1] = (saida[i + 1] * tg + 127) // 255
        saida[i + 2] = (saida[i + 2] * tb + 127) // 255
    return saida


class TexturasDosFluidos(unittest.TestCase):
    def test_cada_fluido_tingido_tem_as_texturas_do_original_com_a_cor(self):
        linhas = rastro()
        self.assertEqual(len(linhas), 22)
        _, _, liquido = ler_png(os.path.join(conteudo("mekanism"), "mek_liquid", "liquid.png"))
        _, _, corre = ler_png(os.path.join(conteudo("mekanism"), "mek_liquid", "liquid_flow.png"))
        tingidos = 0
        for linha in linhas:
            mod, nome = linha["mod"], linha["name"]
            cor = linha["type"]["color"] & 0xFFFFFFFF
            parada = linha["type"]["stillTexture"]
            if parada != "mekanism:mek_liquid/liquid":
                # O vapor tem a `steam.png` (idêntica à do líquido) e a água pesada usa a água do vanilla, que o port não traz.
                continue
            dir_mod = os.path.join(conteudo(mod), "mek_liquid")
            if cor == BRANCO:
                self.assertFalse(os.path.exists(os.path.join(dir_mod, f"{nome}.png")), nome)
                continue
            tingidos += 1
            for sufixo, original in (("", liquido), ("_flow", corre)):
                largura, altura, rgba = ler_png(os.path.join(dir_mod, f"{nome}{sufixo}.png"))
                self.assertEqual(rgba, tingido(original, cor), f"{nome}{sufixo}")
                self.assertTrue(os.path.exists(os.path.join(dir_mod, f"{nome}{sufixo}.png.mcmeta")))
        self.assertEqual(tingidos, 19)  # 22 menos o hidrogênio (branco), o vapor e a água pesada

    def test_o_vapor_usa_os_arquivos_do_original_sem_cor(self):
        base = os.path.join(conteudo("mekanism"), "mek_liquid")
        for nome, igual in (("steam", "liquid"), ("steam_flow", "liquid_flow")):
            with open(os.path.join(base, f"{nome}.png"), "rb") as a, open(os.path.join(base, f"{igual}.png"), "rb") as b:
                self.assertEqual(a.read(), b.read(), nome)

    def test_a_camada_de_liquido_do_balde_e_o_liquido_so_dentro_da_mascara(self):
        largura, altura, liquido = ler_png(os.path.join(conteudo("mekanism"), "mek_liquid", "liquid.png"))
        l2, a2, balde = ler_png(os.path.join(conteudo("mekanism"), "item", "bucket_fluid.png"))
        self.assertEqual((largura, altura), (l2, a2))
        cobertos = 0
        for quadro in range(altura // largura):
            for y in range(largura):
                for x in range(largura):
                    i = ((quadro * largura + y) * largura + x) * 4
                    if y in MASCARA and x in MASCARA[y]:
                        self.assertEqual(balde[i : i + 4], liquido[i : i + 4], (quadro, x, y))
                        cobertos += 1
                    else:
                        self.assertEqual(balde[i : i + 4], bytes(4), (quadro, x, y))
        self.assertEqual(cobertos, (altura // largura) * (8 + 10 + 6))

    def test_a_cor_do_balde_e_a_do_rastro(self):
        for linha in rastro():
            mod, nome = linha["mod"], linha["name"]
            caminho = os.path.join(RAIZ, "mods", mod, "content", "assets", mod, "items", f"{nome}_bucket.json")
            with open(caminho, encoding="utf-8") as arquivo:
                tints = json.load(arquivo)["model"]["tints"]
            self.assertEqual(tints[0]["value"], 0xFFFFFF)
            self.assertEqual(tints[1]["value"], linha["type"]["color"] & 0xFFFFFF, nome)


if __name__ == "__main__":
    unittest.main()
