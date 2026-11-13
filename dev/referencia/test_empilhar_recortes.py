import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import empilhar_recortes as e  # noqa: E402
from comparar_tela import gravar_png, ler_png  # noqa: E402


def imagem(largura, altura, cor):
    return (largura, altura, bytes(cor) * (largura * altura))


class EmpilharRecortes(unittest.TestCase):
    def test_o_recorte_amplia_cada_pixel(self):
        a = imagem(4, 4, (1, 2, 3, 255))
        b = bytearray(a[2])
        b[(1 * 4 + 2) * 4] = 200
        larg, alt, pixels = e.recortar((4, 4, bytes(b)), (2, 1, 2, 2), 3)
        self.assertEqual((larg, alt), (6, 6))
        # O pixel (2, 1) do original é o canto de cima à esquerda do recorte: 3 por 3 pixels com o canal alterado.
        for y in range(3):
            for x in range(3):
                self.assertEqual(pixels[(y * 6 + x) * 4], 200)
        self.assertEqual(pixels[(0 * 6 + 3) * 4], 1)

    def test_o_original_fica_em_cima_e_o_port_embaixo(self):
        original = imagem(4, 4, (10, 10, 10, 255))
        port = imagem(4, 4, (90, 90, 90, 255))
        larg, alt, pixels = e.empilhar(original, port, (0, 0, 2, 2), 2)
        self.assertEqual((larg, alt), (4, 4 + e.FAIXA + 4))
        self.assertEqual(pixels[0], 10)
        self.assertEqual(pixels[(4 * larg) * 4], 128)
        self.assertEqual(pixels[((4 + e.FAIXA) * larg) * 4], 90)

    def test_a_saida_conta_os_pixels_diferentes_e_grava_o_png(self):
        original = imagem(4, 4, (10, 10, 10, 255))
        b = bytearray(original[2])
        b[0] = 99
        with tempfile.TemporaryDirectory() as pasta:
            a_png, b_png, saida = (os.path.join(pasta, n) for n in ("a.png", "b.png", "s.png"))
            gravar_png(a_png, 4, 4, original[2])
            gravar_png(b_png, 4, 4, bytes(b))
            self.assertEqual(e.principal([a_png, b_png, "--regiao", "0,0,2,2", "--escala", "2", "--saida", saida]), 1)
            self.assertEqual(e.principal([a_png, a_png, "--regiao", "0,0,2,2", "--saida", saida]), 0)
            self.assertEqual(ler_png(saida)[:2], (8, 8 + e.FAIXA + 8))

    def test_regiao_invalida_e_recusada(self):
        with tempfile.TemporaryDirectory() as pasta:
            a_png = os.path.join(pasta, "a.png")
            gravar_png(a_png, 4, 4, imagem(4, 4, (0, 0, 0, 255))[2])
            self.assertEqual(e.principal([a_png, a_png, "--regiao", "0,0,9,9", "--saida", os.path.join(pasta, "s.png")]), 2)


if __name__ == "__main__":
    unittest.main()
