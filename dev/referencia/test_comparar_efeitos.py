import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import comparar_efeitos as c  # noqa: E402
from comparar_tela import gravar_png, ler_png  # noqa: E402


def imagem(largura, altura, cor):
    return (largura, altura, bytes(cor) * (largura * altura))


class CompararEfeitos(unittest.TestCase):
    def test_imagens_iguais_nao_diferem(self):
        a = imagem(8, 4, (10, 20, 30, 255))
        diferentes, total, _, _ = c.comparar(a, a)
        self.assertEqual((diferentes, total), (0, 32))

    def test_conta_os_pixels_diferentes_e_respeita_a_regiao(self):
        a = imagem(8, 4, (10, 20, 30, 255))
        b = bytearray(a[2])
        b[(1 * 8 + 3) * 4 + 1] += 5
        b[(3 * 8 + 7) * 4] += 1
        port = (8, 4, bytes(b))
        self.assertEqual(c.comparar(a, port)[0], 2)
        self.assertEqual(c.comparar(a, port, (0, 0, 4, 2))[0], 1)
        self.assertEqual(c.comparar(a, port, tolerancia=1)[0], 1)
        self.assertEqual(c.comparar(a, port, tolerancia=5)[0], 0)

    def test_tamanhos_diferentes_sao_recusados(self):
        with self.assertRaises(ValueError):
            c.comparar(imagem(8, 4, (0, 0, 0, 255)), imagem(4, 4, (0, 0, 0, 255)))

    def test_o_lado_a_lado_tem_tres_vezes_a_largura(self):
        a = imagem(4, 2, (10, 20, 30, 255))
        b = bytearray(a[2])
        b[0] += 9
        port = (4, 2, bytes(b))
        _, _, mascara, recorte = c.comparar(a, port)
        with tempfile.TemporaryDirectory() as pasta:
            caminho = os.path.join(pasta, "lado.png")
            c.lado_a_lado(a, port, mascara, recorte, caminho)
            largura, altura, pixels = ler_png(caminho)
        self.assertEqual((largura, altura), (12, 2))
        # O primeiro pixel da diferença é vermelho.
        self.assertEqual(tuple(pixels[8 * 4 : 8 * 4 + 4]), (255, 0, 0, 255))

    def test_a_saida_da_linha_de_comando(self):
        with tempfile.TemporaryDirectory() as pasta:
            igual = os.path.join(pasta, "a.png")
            gravar_png(igual, 4, 2, bytes((1, 2, 3, 255)) * 8)
            self.assertEqual(c.principal([igual, igual]), 0)
            outro = os.path.join(pasta, "b.png")
            gravar_png(outro, 4, 2, bytes((9, 2, 3, 255)) * 8)
            self.assertEqual(c.principal([igual, outro]), 1)
            self.assertEqual(c.principal([igual, igual, "--regiao", "0,0,9,9"]), 2)


if __name__ == "__main__":
    unittest.main()
