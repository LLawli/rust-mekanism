"""O padrão de aviso da aba de redstone que o código embute (`WARNING_ROWS`, `STRIPE_LIGHT`, `STRIPE_DARK`) é o do `warning.png` do original, pixel a pixel."""
import os
import re
import unittest

from png_minimo import ler_png

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SPRITES = os.path.join(RAIZ, "mods", "mekanism", "content", "assets", "mekanism", "textures", "gui", "sprites")
FONTE = os.path.join(RAIZ, "mods", "machinery_basis", "core", "src", "redstone_tab.rs")


def constante(texto, nome):
    achado = re.search(r"pub const " + nome + r": [^=]+= ([^;]+);", texto, re.S)
    assert achado, nome
    return achado.group(1)


class PadraoDeAviso(unittest.TestCase):
    def test_o_padrao_do_codigo_e_o_do_warning_png(self):
        largura, altura, pixels = ler_png(os.path.join(SPRITES, "warning.png"))
        self.assertEqual((largura, altura), (16, 16))
        texto = open(FONTE, encoding="utf-8").read()
        claro = int(constante(texto, "STRIPE_LIGHT").replace("_", ""), 16)
        escuro = int(constante(texto, "STRIPE_DARK").replace("_", ""), 16)
        linhas = [int(v.replace("_", ""), 16) for v in re.findall(r"0x[0-9a-f_]+", constante(texto, "WARNING_ROWS"))]
        self.assertEqual(len(linhas), 16)
        argb = lambda p: (p[3] << 24) | (p[0] << 16) | (p[1] << 8) | p[2]
        for y in range(16):
            for x in range(16):
                esperado = claro if (linhas[y] >> x) & 1 else escuro
                self.assertEqual(argb(pixels[y][x]), esperado, f"({x}, {y})")

    def test_o_suporte_tem_cantos_transparentes_e_o_resto_opaco(self):
        # O aviso multiplica o suporte pixel a pixel; o que é transparente fica como está (o original multiplicaria o mundo atrás).
        largura, altura, pixels = ler_png(os.path.join(SPRITES, "holder_right.png"))
        self.assertEqual((largura, altura), (26, 9))
        alfas = {p[3] for linha in pixels for p in linha}
        self.assertEqual(alfas, {0, 255})


if __name__ == "__main__":
    unittest.main()
