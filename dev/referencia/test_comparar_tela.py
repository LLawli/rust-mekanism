"""Testes de `comparar_tela.py`, sem o jogo: imagens sintéticas, e uma delas tem de falhar."""
import contextlib
import io
import os
import tempfile
import unittest

import cena_telas
import comparar_lote as cl
import comparar_tela as ct

LARGURA_JANELA = 640
ALTURA_JANELA = 480
ESCALA = 2


def imagem(cor=(120, 130, 140)):
    """Uma janela lisa, em RGBA."""
    return LARGURA_JANELA, ALTURA_JANELA, bytearray(bytes(cor) + b"\xff") * (LARGURA_JANELA * ALTURA_JANELA)


def pintar(img, x, y, cor=(255, 255, 255)):
    """Um pixel físico da imagem."""
    largura, _, rgba = img
    rgba[(y * largura + x) * 4 : (y * largura + x) * 4 + 3] = bytes(cor)


def copia(img):
    return img[0], img[1], bytearray(img[2])


def na_tela(x_gui, y_gui, tela_origem=None):
    """O pixel físico do canto de cima e da esquerda de um pixel de GUI da tela."""
    ox, oy = tela_origem or ct.origem_da_tela(LARGURA_JANELA, ALTURA_JANELA, ESCALA)
    return ox + x_gui * ESCALA, oy + y_gui * ESCALA


class CompararTela(unittest.TestCase):
    def test_a_tela_fica_no_centro_da_janela(self):
        # 640 por 480 com escala 2 é 320 por 240 de GUI: (320 - 176) / 2 e (240 - 166) / 2, em pixels.
        self.assertEqual(ct.origem_da_tela(640, 480, 2), (144, 74))
        # Uma janela que não divide pela escala arredonda para cima: 641 é 321 de GUI.
        self.assertEqual(ct.origem_da_tela(641, 480, 2), (144, 74))
        self.assertEqual(ct.origem_da_tela(1920, 1080, 3), (((640 - 176) // 2) * 3, ((360 - 166) // 2) * 3))

    def test_duas_imagens_iguais_dao_zero(self):
        a = imagem()
        resultado, _ = ct.comparar(a, copia(a), "infuser", ESCALA)
        self.assertEqual(resultado["diferentes"], 0)
        self.assertEqual(resultado["comparados"], 176 * 166 * ESCALA * ESCALA)

    def test_um_pixel_diferente_numa_barra_conta_no_elemento_dela(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(165, 20)  # dentro da barra de energia do infuser, que é 164,15 de 6 por 54
        pintar(b, x, y)
        resultado, _ = ct.comparar(a, b, "infuser", ESCALA)
        self.assertEqual(resultado["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["barra_energia"]["diferentes"], 1)
        # O mesmo pixel de GUI nas máquinas elétricas, onde a barra começa em 16, ainda é da barra.
        resultado, _ = ct.comparar(a, b, "electric", ESCALA)
        self.assertEqual(resultado["elementos"]["barra_energia"]["diferentes"], 1)

    def test_cada_pixel_cai_no_seu_elemento(self):
        a = imagem()
        casos = {
            ("infuser", 10, 20): "barra_quimico",
            ("infuser", 30, 60): "botao_dump",
            ("infuser", 75, 50): "progresso",
            ("infuser", 143, 36): "slot_energia",
            ("electric", 70, 40): "seta",
            ("electric", 90, 40): "progresso",
            ("electric", 120, 40): "slot_saida",
            ("generator", 60, 30): "medidor_lava",
            ("infuser", 20, 7): "titulo",
            ("infuser", 9, 74): "rotulo_inventario",
            ("infuser", 100, 100): "inventario",
            ("infuser", 100, 70): "fundo",
        }
        for (tela, gx, gy), nome in casos.items():
            b = copia(a)
            x, y = na_tela(gx, gy)
            pintar(b, x, y)
            resultado, _ = ct.comparar(a, b, tela, ESCALA)
            self.assertEqual(resultado["elementos"][nome]["diferentes"], 1, f"{tela} {gx},{gy} em {nome}")
            self.assertEqual(resultado["diferentes"], 1, f"{tela} {gx},{gy}")

    def test_as_abas_que_o_port_nao_tem_ficam_fora_da_conta(self):
        a = imagem()
        b = copia(a)
        # Com margem, a coluna da esquerda entra no recorte: uma aba de configuração em -20,30 não conta.
        x, y = na_tela(-20, 30)
        pintar(b, x, y)
        resultado, _ = ct.comparar(a, b, "infuser", ESCALA, margem=30)
        self.assertEqual(resultado["diferentes"], 0)
        self.assertGreater(resultado["mascarados"], 0)

    def test_a_aba_de_avisos_conta(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(-10, 120)  # dentro da aba de avisos das máquinas, que está na coluna das abas
        pintar(b, x, y)
        resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=30)
        self.assertEqual(resultado["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["aba_avisos"]["diferentes"], 1)
        # No gerador a aba de avisos fica à direita.
        x, y = na_tela(180, 120)
        c = copia(a)
        pintar(c, x, y)
        resultado, _ = ct.comparar(a, c, "generator", ESCALA, margem=30)
        self.assertEqual(resultado["elementos"]["aba_avisos"]["diferentes"], 1)

    def test_uma_mascara_a_mais_tira_o_elemento_da_conta(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(10, 20)
        pintar(b, x, y)
        resultado, _ = ct.comparar(a, b, "infuser", ESCALA, mascaras=[(7, 15, 6, 54)])
        self.assertEqual(resultado["diferentes"], 0)

    def test_duas_imagens_diferentes_falham_e_a_imagem_de_diferenca_mostra_onde(self):
        a = imagem((10, 10, 10))
        b = imagem((200, 10, 10))
        resultado, saida = ct.comparar(a, b, "infuser", ESCALA)
        self.assertEqual(resultado["diferentes"], resultado["comparados"])
        largura, altura, rgba = saida
        self.assertEqual((largura, altura), (176 * ESCALA, 166 * ESCALA))
        self.assertEqual(bytes(rgba[:4]), b"\xff\x00\x00\xff")
        with tempfile.TemporaryDirectory() as pasta:
            caminho = os.path.join(pasta, "diferenca.png")
            ct.gravar_png(caminho, largura, altura, rgba)
            lida = ct.ler_png(caminho)
            self.assertEqual(lida[:2], (largura, altura))
            self.assertEqual(bytes(lida[2]), bytes(rgba))

    def test_uma_regiao_fora_da_tela_entra_na_conta_e_o_recorte_cresce(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(190, -5)  # fora da tela e da coluna das abas, onde cai o tooltip de uma barra alta
        pintar(b, x, y)
        sem = ct.comparar(a, b, "infuser", ESCALA)[0]
        self.assertEqual(sem["diferentes"], 0)
        com, imagem_diferenca = ct.comparar(a, b, "infuser", ESCALA, regioes={"tooltip": (180, -10, 60, 30)})
        self.assertEqual(com["diferentes"], 1)
        self.assertEqual(com["elementos"]["tooltip"]["diferentes"], 1)
        self.assertGreater(imagem_diferenca[0], 176 * ESCALA)
        # Na coluna das abas que o port não tem, a região não conta.
        x, y = na_tela(190, 40)
        c = copia(a)
        pintar(c, x, y)
        coluna = ct.comparar(a, c, "infuser", ESCALA, regioes={"tooltip": (180, 30, 60, 30)})[0]
        self.assertEqual(coluna["diferentes"], 0)
        # Dentro da tela a região não rouba os pixels dos elementos.
        x, y = na_tela(165, 20)
        c = copia(a)
        pintar(c, x, y)
        resultado = ct.comparar(a, c, "infuser", ESCALA, regioes={"tooltip": (150, 10, 60, 30)})[0]
        self.assertEqual(resultado["elementos"]["barra_energia"]["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["tooltip"]["diferentes"], 0)

    def test_lado_a_lado_junta_o_original_o_port_e_a_diferenca(self):
        a = imagem((10, 20, 30))
        b = imagem((200, 20, 30))
        resultado, diferenca = ct.comparar(a, b, "infuser", ESCALA)
        largura, altura, rgba = ct.lado_a_lado(a, b, diferenca, resultado["recorte"])
        w = 176 * ESCALA
        self.assertEqual((largura, altura), (w * 3 + 16, 166 * ESCALA))
        self.assertEqual(bytes(rgba[:4]), bytes((10, 20, 30, 255)))
        self.assertEqual(bytes(rgba[(w + 8) * 4 : (w + 8) * 4 + 4]), bytes((200, 20, 30, 255)))
        self.assertEqual(bytes(rgba[(2 * (w + 8)) * 4 : (2 * (w + 8)) * 4 + 4]), b"\xff\x00\x00\xff")

    def test_com_as_abas_ocultas_no_original_a_coluna_delas_conta(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(-20, 30)
        pintar(b, x, y)
        resultado = ct.comparar(a, b, "infuser", ESCALA, margem=30, mascarar_abas=False)[0]
        self.assertEqual(resultado["diferentes"], 1)
        self.assertEqual(resultado["mascarados"], 0)

    def test_a_tolerancia_aceita_uma_diferenca_pequena(self):
        a = imagem((100, 100, 100))
        b = imagem((101, 100, 100))
        self.assertEqual(ct.comparar(a, b, "infuser", ESCALA)[0]["diferentes"], 176 * 166 * ESCALA * ESCALA)
        self.assertEqual(ct.comparar(a, b, "infuser", ESCALA, tolerancia=1)[0]["diferentes"], 0)

    def test_entrada_invalida_e_um_erro(self):
        a = imagem()
        pequena = (320, 240, bytearray(320 * 240 * 4))
        with self.assertRaises(ValueError):
            ct.comparar(a, pequena, "infuser", ESCALA)
        with self.assertRaises(ValueError):
            ct.comparar(a, a, "inexistente", ESCALA)
        # A tela não cabe: origem perto do canto.
        with self.assertRaises(ValueError):
            ct.comparar(a, a, "infuser", ESCALA, origem=(600, 400))
        with self.assertRaises(ValueError):
            ct.comparar(a, a, "infuser", 0)

    def test_o_codigo_de_saida_do_programa(self):
        a = imagem()
        b = copia(a)
        x, y = na_tela(100, 100)
        pintar(b, x, y)
        with tempfile.TemporaryDirectory() as pasta:
            igual = os.path.join(pasta, "a.png")
            outro = os.path.join(pasta, "b.png")
            ct.gravar_png(igual, *a)
            ct.gravar_png(outro, *b)
            saida = os.path.join(pasta, "diferenca.png")
            base = ["--tela", "infuser", "--escala", str(ESCALA)]
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(ct.principal([igual, igual, *base]), 0)
                self.assertEqual(ct.principal([igual, outro, *base, "--saida", saida]), 1)
                self.assertTrue(os.path.getsize(saida) > 0)
                # Arquivo que não existe: entrada inválida.
                self.assertEqual(ct.principal([igual, os.path.join(pasta, "nada.png"), *base]), 2)

    def test_le_png_de_paleta_e_de_tres_canais(self):
        # RGB de 8 bits sem alfa, como alguns prints saem: o alfa vira 255.
        import struct
        import zlib

        def bloco(tipo, corpo):
            return struct.pack(">I", len(corpo)) + tipo + corpo + struct.pack(">I", zlib.crc32(tipo + corpo))

        linhas = b"\x00" + bytes([1, 2, 3, 4, 5, 6]) + b"\x00" + bytes([7, 8, 9, 10, 11, 12])
        dados = (
            b"\x89PNG\r\n\x1a\n"
            + bloco(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
            + bloco(b"IDAT", zlib.compress(linhas))
            + bloco(b"IEND", b"")
        )
        with tempfile.TemporaryDirectory() as pasta:
            caminho = os.path.join(pasta, "rgb.png")
            with open(caminho, "wb") as arquivo:
                arquivo.write(dados)
            largura, altura, rgba = ct.ler_png(caminho)
        self.assertEqual((largura, altura), (2, 2))
        self.assertEqual(bytes(rgba), bytes([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]))


class Lote(unittest.TestCase):
    def test_o_tooltip_e_achado_pelo_roxo_perto_do_cursor(self):
        img = imagem((13, 13, 13))
        # Um tooltip roxo de 20 por 8 pixels de GUI a partir de 10,20 da tela.
        for gy in range(20, 28):
            for gx in range(190, 210):
                x, y = na_tela(gx, gy)
                for dx in range(ESCALA):
                    for dy in range(ESCALA):
                        pintar(img, x + dx, y + dy, (16, 0, 16))
        self.assertEqual(cl.caixa_do_tooltip(img, ESCALA, (167, 22)), (190, 20, 20, 8))
        self.assertIsNone(cl.caixa_do_tooltip(imagem((13, 13, 13)), ESCALA, (167, 22)))

    def test_a_uniao_de_duas_caixas(self):
        self.assertEqual(cl.uniao((10, 10, 5, 5), (12, 8, 10, 4)), (10, 8, 12, 7))
        self.assertEqual(cl.uniao(None, (1, 2, 3, 4)), (1, 2, 3, 4))

    def test_a_tabela_de_casos_tem_nomes_unicos_e_telas_conhecidas(self):
        nomes = [c["nome"] for c in cena_telas.casos()]
        self.assertEqual(len(nomes), len(set(nomes)))
        for c in cena_telas.casos():
            self.assertIn(cena_telas.MAQUINAS[c["maquina"]]["tela"], ct.TELAS)
            if "hover" in c["args"]:
                self.assertEqual(len(c["args"]["hover"]), 2)


if __name__ == "__main__":
    unittest.main()
