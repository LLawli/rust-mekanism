import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comparar_item as c  # noqa: E402
from comparar_tela import gravar_png  # noqa: E402


def png(pasta, nome, largura, altura, cor):
    gravar_png(os.path.join(pasta, nome), largura, altura, bytes(cor) * (largura * altura))


class CompararItem(unittest.TestCase):
    def test_o_prefixo_escolhe_o_recorte(self):
        self.assertEqual(c.regiao_de("tooltip_crusher"), c.REGIOES["tooltip"])
        self.assertEqual(c.regiao_de("tela_depois_crusher"), c.REGIOES["tela"])
        self.assertIsNone(c.regiao_de("quebrada_crusher"))

    def test_a_caixa_cobre_so_os_pixels_marcados(self):
        mascara = bytearray(6 * 4)
        mascara[1 * 6 + 2] = 1
        mascara[2 * 6 + 4] = 1
        self.assertEqual(c.caixa_das_diferencas(mascara, 6, 4, (10, 20)), (12, 21, 3, 2))
        self.assertIsNone(c.caixa_das_diferencas(bytearray(24), 6, 4, (0, 0)))

    def test_os_tiros_comuns_e_os_de_um_lado_so(self):
        with tempfile.TemporaryDirectory() as original, tempfile.TemporaryDirectory() as port:
            png(original, "oficial_a.png", 2, 2, (1, 2, 3, 255))
            png(original, "oficial_b.png", 2, 2, (1, 2, 3, 255))
            png(port, "porte_a.png", 2, 2, (1, 2, 3, 255))
            png(port, "porte_c.png", 2, 2, (1, 2, 3, 255))
            self.assertEqual(c.tiros(original, port), (["a"], ["b"], ["c"]))


if __name__ == "__main__":
    unittest.main()
