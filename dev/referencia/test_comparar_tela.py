"""Testes de `comparar_tela.py`, sem o jogo: imagens sintéticas, e uma delas tem de falhar."""
import contextlib
import io
import os
import tempfile
import unittest
from unittest import mock

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
    def test_um_pixel_da_janela_de_upgrades_conta_no_filho_dela_e_nao_no_elemento_da_tela(self):
        a = imagem()
        b = copia(a)
        # (30, 40) em GUI cai na lista da janela, que cobre o slot de entrada da tela (63, 16) só em outra posição: use o fundo da lista,
        # fora do ícone da linha (-2, 36, 8, 8).
        px, py = na_tela(40, 40)
        pintar(b, px, py)
        resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=28, prioridade=ct.JANELA, mascarar_abas=False)
        self.assertEqual(resultado["elementos"]["janela_lista"]["diferentes"], 1)
        # Fora da janela o mesmo pixel é de outro elemento: sem a prioridade, não há filho da janela na conta.
        sem, _ = ct.comparar(a, b, "electric", ESCALA, margem=28, mascarar_abas=False)
        self.assertNotIn("janela_lista", sem["elementos"])

    def test_a_aba_de_upgrades_tem_o_retangulo_dela(self):
        a = imagem()
        b = copia(a)
        px, py = na_tela(ct.LARGURA + 3, 10)
        pintar(b, px, py)
        resultado, _ = ct.comparar(a, b, "infuser", ESCALA, margem=28, mascarar_abas=False)
        self.assertEqual(resultado["elementos"]["aba_upgrades"]["diferentes"], 1)

    def test_a_aba_de_xp_tem_o_retangulo_dela_e_conta(self):
        a = imagem()
        b = copia(a)
        # O botão de dentro (179, 94) a (197, 112), e a borda do suporte: tudo dentro de (176, 90, 26, 26).
        for gx, gy in ((188, 100), (177, 91), (199, 115)):
            c = copia(a)
            pintar(c, *na_tela(gx, gy))
            for tela in ("electric", "infuser"):
                resultado, _ = ct.comparar(a, c, tela, ESCALA, margem=28)
                self.assertEqual(resultado["elementos"]["aba_xp"]["diferentes"], 1, f"{tela} {gx},{gy}")
        # O canto transparente do suporte mostra o mundo atrás da tela: conta à parte, não como a aba.
        c = copia(a)
        pintar(c, *na_tela(201, 115))
        resultado, _ = ct.comparar(a, c, "electric", ESCALA, margem=28)
        self.assertEqual(resultado["elementos"]["canto_aba_xp"]["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["aba_xp"]["diferentes"], 0)
        # Fora do suporte a mesma coluna é de uma aba que o port não tem.
        c = copia(a)
        pintar(c, *na_tela(188, 130))
        resultado, _ = ct.comparar(a, c, "electric", ESCALA, margem=28)
        self.assertEqual(resultado["diferentes"], 0)
        # O gerador não tem a aba.
        resultado, _ = ct.comparar(a, a, "generator", ESCALA, margem=28)
        self.assertNotIn("aba_xp", resultado["elementos"])

    def test_a_aba_de_energia_e_a_de_calor_contam_como_elementos_do_mod(self):
        a = imagem()
        for tela in ("electric", "infuser", "generator"):
            b = copia(a)
            pintar(b, *na_tela(-10, 150))
            resultado, _ = ct.comparar(a, b, tela, ESCALA, margem=30)
            self.assertEqual(resultado["elementos"]["aba_energia"]["diferentes"], 1, tela)
        b = copia(a)
        pintar(b, *na_tela(-10, 120))
        resultado, _ = ct.comparar(a, b, "generator", ESCALA, margem=30)
        self.assertEqual(resultado["elementos"]["aba_calor"]["diferentes"], 1)
        # O gerador ainda tem os slots de armadura e a aba de upgrades à direita: elementos dele que o port tem de ganhar.
        for nome, ponto in (("slots_armadura", (-12, 50)), ("aba_upgrades", (190, 20))):
            c = copia(a)
            pintar(c, *na_tela(*ponto))
            resultado, _ = ct.comparar(a, c, "generator", ESCALA, margem=30)
            self.assertEqual(resultado["elementos"][nome]["diferentes"], 1, nome)
        # Nas máquinas a mesma região é a aba de avisos, que tem o lugar dela.
        resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=30)
        self.assertEqual(resultado["elementos"]["aba_avisos"]["diferentes"], 1)
        # Sem a máscara das abas, o que falta nelas não some.
        resultado, _ = ct.comparar(a, copia(a), "electric", ESCALA, margem=30)
        self.assertEqual(resultado["diferentes"], 0)

    def test_o_canto_transparente_da_aba_de_calor_conta_a_parte(self):
        a = imagem()
        cantos = ct.cantos_do_sprite(ct.ABA_CALOR, ct.CALOR_PNG)
        self.assertGreater(len(cantos), 0)
        gx, gy = sorted(cantos)[0]
        b = copia(a)
        pintar(b, *na_tela(gx, gy))
        resultado, _ = ct.comparar(a, b, "generator", ESCALA, margem=30)
        self.assertEqual(resultado["elementos"]["canto_aba_calor"]["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["aba_calor"]["diferentes"], 0)

    def test_o_canto_transparente_da_aba_de_energia_conta_a_parte(self):
        a = imagem()
        cantos = ct.cantos_do_sprite(ct.ABA_ENERGIA, ct.ENERGIA_PNG)
        self.assertGreater(len(cantos), 0)
        gx, gy = sorted(cantos)[0]
        b = copia(a)
        pintar(b, *na_tela(gx, gy))
        resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=30)
        self.assertEqual(resultado["elementos"]["canto_aba_energia"]["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["aba_energia"]["diferentes"], 0)

    def test_so_o_tooltip_tem_tolerancia_e_um_texto_diferente_passa_dela(self):
        a = imagem()
        regioes = {"tooltip": (200, 100, 20, 10)}
        leve = copia(a)
        pintar(leve, *na_tela(205, 105), cor=(130, 140, 150))  # 10 níveis: o mundo atrás por baixo do fundo
        resultado, _ = ct.comparar(a, leve, "electric", ESCALA, margem=30, regioes=regioes)
        self.assertEqual(resultado["elementos"]["tooltip"]["diferentes"], 0)
        texto = copia(a)
        pintar(texto, *na_tela(205, 105), cor=(255, 255, 255))  # um pixel de texto
        resultado, _ = ct.comparar(a, texto, "electric", ESCALA, margem=30, regioes=regioes)
        self.assertEqual(resultado["elementos"]["tooltip"]["diferentes"], 1)
        # Fora do tooltip, nenhuma tolerância.
        fora = copia(a)
        pintar(fora, *na_tela(80, 40), cor=(130, 140, 150))
        resultado, _ = ct.comparar(a, fora, "electric", ESCALA)
        self.assertEqual(resultado["diferentes"], 1)

    def test_os_casos_da_aba_de_energia_hover_caem_dentro_dela(self):
        for c in cena_telas.casos_energia():
            if "hover" in c["args"]:
                x, y = c["args"]["hover"]
                self.assertTrue(-26 <= x < 0 and 137 <= y < 163, c["nome"])

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

    def test_cada_elemento_guarda_a_maior_diferenca_por_canal(self):
        # O teto dos resíduos lê isto: o maior dos três canais, entre os pixels diferentes do elemento, e 0 sem diferença.
        a = imagem()
        b = copia(a)
        pintar(b, *na_tela(165, 20), cor=(125, 100, 141))
        pintar(b, *na_tela(165, 30), cor=(121, 131, 150))
        resultado, _ = ct.comparar(a, b, "infuser", ESCALA)
        self.assertEqual(resultado["elementos"]["barra_energia"]["maior"], 30)
        self.assertEqual(resultado["elementos"]["fundo"]["maior"], 0)

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

    def test_um_elemento_ausente_vira_mundo_atras(self):
        a = imagem()
        b = copia(a)
        pintar(b, *na_tela(188, 100))
        com, _ = ct.comparar(a, b, "electric", ESCALA, margem=28)
        self.assertEqual(com["elementos"]["aba_xp"]["diferentes"], 1)
        sem, _ = ct.comparar(a, b, "electric", ESCALA, margem=28, ausentes=["aba_xp"])
        self.assertNotIn("aba_xp", sem["elementos"])
        self.assertEqual(sem["elementos"]["mundo_atras"]["diferentes"], 1)

    def test_a_aba_de_xp_aparece_com_o_upgrade_instalado(self):
        por_nome = {c["nome"]: c for c in cena_telas.casos_xp()}
        visiveis = {n for n, c in por_nome.items() if cena_telas.xp_visivel(c)}
        self.assertIn("chamber_xp_com", visiveis)
        self.assertIn("chamber_xp_instalado", visiveis)
        for ausente in ("chamber_xp_sem", "chamber_xp_instalando", "chamber_xp_hover_sem", "chamber_nome_cor"):
            self.assertNotIn(ausente, visiveis)

    def test_os_casos_da_aba_de_xp_tem_nomes_unicos_e_cursor_no_lugar_certo(self):
        todos = cena_telas.todos_os_casos()
        nomes = [c["nome"] for c in todos]
        self.assertEqual(len(nomes), len(set(nomes)))
        por_nome = {c["nome"]: c for c in cena_telas.casos_xp()}
        # O botão é (179, 94, 18, 18) e o suporte (176, 90, 26, 26): o cursor do botão cai no botão, o da borda só no suporte.
        botao, suporte = (179, 94, 18, 18), (176, 90, 26, 26)
        dentro = lambda r, p: r[0] <= p[0] < r[0] + r[2] and r[1] <= p[1] < r[1] + r[3]
        self.assertTrue(dentro(botao, por_nome["chamber_xp_hover"]["args"]["hover"]))
        self.assertTrue(dentro(suporte, por_nome["chamber_xp_hover_borda"]["args"]["hover"]))
        self.assertFalse(dentro(botao, por_nome["chamber_xp_hover_borda"]["args"]["hover"]))
        # Atrás da janela de upgrades (-11, 15, 198, 94): dentro do botão e dentro da janela. Fora: dentro do botão e fora da janela.
        janela = (-11, 15, 198, 94)
        atras = por_nome["chamber_xp_janela_atras"]["args"]["hover"]
        fora = por_nome["chamber_xp_janela_fora"]["args"]["hover"]
        self.assertTrue(dentro(botao, atras) and dentro(janela, atras))
        self.assertTrue(dentro(botao, fora) and not dentro(janela, fora))

    def test_o_ciclo_do_port_cobre_o_da_animacao(self):
        # Quadros vezes `frametime` de cada tira: `base` 10 de 4, `bio` 8 de 30, `fungi` 4 de 40, `lava_still` 38 de 2.
        ciclo = {"infuser_tanque_carbono": 40, "infuser_tanque_bio_ciclo": 240, "infuser_tanque_fungi_ciclo": 160, "generator_lava_ciclo": 76}
        por_nome = {c["nome"]: c for c in cena_telas.casos()}
        for nome, esperado in ciclo.items():
            self.assertGreaterEqual(cena_telas.ciclos(por_nome[nome], "port"), esperado, nome)
            self.assertEqual(cena_telas.ciclos(por_nome[nome], "oficial"), 40, nome)

    def test_a_lava_do_ciclo_nao_queima(self):
        # Com energia abaixo da capacidade o gerador gasta a lava a cada passo e o medidor muda de nível em vez de animar.
        por_nome = {c["nome"]: c for c in cena_telas.casos()}
        self.assertEqual(por_nome["generator_lava_ciclo"]["args"]["energia"], "cap")

    def test_o_mundo_atras_so_vale_como_residuo_ate_o_limite(self):
        a = imagem()
        # Três níveis a mais (13 contra 10): o escurecimento do Horizon (H22), residual.
        leve = copia(a)
        pintar(leve, *na_tela(-20, 110), cor=(123, 133, 143))
        resultado, _ = ct.comparar(a, leve, "electric", ESCALA, margem=30, limite_mundo=4, ausentes=["aba_avisos"])
        self.assertEqual(resultado["elementos"]["mundo_atras"]["diferentes"], 1)
        self.assertNotIn("alem_mundo_atras", resultado["elementos"])
        self.assertEqual(resultado["maior_do_mundo"], 3)
        # Uma aba que o port desenha onde o original não tem nada, ou o texto do chat: passa do limite e conta de verdade.
        forte = copia(a)
        pintar(forte, *na_tela(-20, 110), cor=(220, 220, 220))
        resultado, _ = ct.comparar(a, forte, "electric", ESCALA, margem=30, limite_mundo=4, ausentes=["aba_avisos"])
        self.assertEqual(resultado["elementos"]["alem_mundo_atras"]["diferentes"], 1)
        # Sem `limite_mundo` a conta é a antiga: tudo no mesmo nome.
        resultado, _ = ct.comparar(a, forte, "electric", ESCALA, margem=30, ausentes=["aba_avisos"])
        self.assertNotIn("alem_mundo_atras", resultado["elementos"])
        # Os cantos transparentes do painel e a sombra da janela são o mundo também: passam do limite do mesmo jeito.
        canto = copia(a)
        pintar(canto, *na_tela(0, 0), cor=(220, 220, 220))
        resultado, _ = ct.comparar(a, canto, "electric", ESCALA, limite_mundo=4)
        self.assertEqual(resultado["elementos"]["alem_canto_cima_esquerda"]["diferentes"], 1)
        sombra = copia(a)
        pintar(sombra, *na_tela(-12, 14), cor=(220, 220, 220))
        resultado, _ = ct.comparar(a, sombra, "electric", ESCALA, margem=28, prioridade=ct.JANELA, limite_mundo=4, mascarar_abas=False)
        self.assertEqual(resultado["elementos"]["alem_janela_fundo"]["diferentes"], 1)
        # Com a aba de avisos no lugar (o original a desenha), o mesmo pixel é dela.
        resultado, _ = ct.comparar(a, forte, "electric", ESCALA, margem=30, limite_mundo=4)
        self.assertEqual(resultado["elementos"]["aba_avisos"]["diferentes"], 1)

    def test_o_residuo_so_vale_para_o_que_o_horizon_desenha_diferente(self):
        for nome in ("mundo_atras", "janela_fundo", "janela_aceitos", "canto_aba_xp", "janela_lista_icone_2", "barra_quimico_fase", "medidor_lava_fase",
                     "tooltip_canto_0", "tooltip_canto_3"):
            self.assertTrue(cl.residuo(nome), nome)
        for nome in ("alem_mundo_atras", "alem_canto_aba_xp", "janela_lista", "barra_quimico", "medidor_lava", "tooltip", "aba_avisos", "aba_calor"):
            self.assertFalse(cl.residuo(nome), nome)

    def test_o_residuo_com_teto_so_vale_ate_a_diferenca_medida(self):
        # Um ícone trocado na lista, um canto de tooltip de outra cor, a barra de químico ou a lava com outro nível ou outra cor: um pixel ou um nível
        # por canal acima do medido e deixa de ser resíduo.
        for prefixo, nome in (("janela_lista_icone", "janela_lista_icone_1"), ("tooltip_canto", "tooltip_canto_2"),
                              ("barra_quimico_fase", "barra_quimico_fase"), ("medidor_lava_fase", "medidor_lava_fase")):
            for escala in (2, 3):
                px, canal = cl.TETO_DO_RESIDUO[prefixo][escala]
                self.assertTrue(cl.residuo(nome, escala, px, canal), (nome, escala))
                self.assertFalse(cl.residuo(nome, escala, px + 1, canal), (nome, escala))
                self.assertFalse(cl.residuo(nome, escala, px, canal + 1), (nome, escala))
        # Os resíduos sem teto continuam valendo pelo nome, e um elemento da tela não vira resíduo com diferença pequena.
        self.assertTrue(cl.residuo("janela_fundo", 3, 10**6, 255))
        self.assertFalse(cl.residuo("janela_lista", 2, 1, 1))
        resid, reais = cl.residuos_e_reais({"janela_lista_icone_0": {"diferentes": 1, "maior": 255}, "janela_fundo": {"diferentes": 9, "maior": 3}}, 2)
        self.assertEqual((resid, reais), ({"janela_fundo": 9}, {"janela_lista_icone_0": 1}))

    def test_o_icone_de_cada_linha_da_lista_tem_o_retangulo_dele(self):
        a = imagem()
        for linha in range(4):
            # Os quatro cantos do ícone de 8 por 8 são dele, e o pixel de fora (acima, abaixo, à esquerda e à direita) é da lista.
            for dx, dy, dele in ((0, 0, True), (7, 0, True), (0, 7, True), (7, 7, True), (-1, 0, False), (8, 0, False), (0, -1, False), (0, 8, False)):
                b = copia(a)
                pintar(b, *na_tela(-2 + dx, 36 + 12 * linha + dy))
                resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=28, prioridade=ct.JANELA, mascarar_abas=False)
                self.assertEqual(resultado["elementos"][f"janela_lista_icone_{linha}"]["diferentes"], int(dele), (linha, dx, dy))
                self.assertEqual(resultado["elementos"]["janela_lista"]["diferentes"], int(not dele), (linha, dx, dy))
        # Ao lado do ícone, na mesma linha, a lista conta sem desculpa.
        b = copia(a)
        pintar(b, *na_tela(40, 38))
        resultado, _ = ct.comparar(a, b, "electric", ESCALA, margem=28, prioridade=ct.JANELA, mascarar_abas=False)
        self.assertEqual(resultado["elementos"]["janela_lista"]["diferentes"], 1)

    def test_as_abas_ausentes_vem_do_log_do_original(self):
        with tempfile.TemporaryDirectory() as pasta:
            caminho = os.path.join(pasta, "oficial.log")
            with open(caminho, "w") as arquivo:
                arquivo.write(
                    "[x] [Render thread/INFO] [m/]: cena: chamber_aba_upgrades fora GuiWarningTab x -26 y 109 largura 26 altura 26 visivel false\n"
                    "[x] [Render thread/INFO] [m/]: cena: chamber_aba_upgrades xp_tab visible false x 176 y 90 largura 26 altura 26\n"
                    "[x] [Render thread/INFO] [m/]: cena: chamber_aviso_saida fora GuiWarningTab x -26 y 109 largura 26 altura 26 visivel true\n"
                    "[x] [Render thread/INFO] [m/]: cena: chamber_aviso_saida xp_tab visible true x 176 y 90 largura 26 altura 26\n"
                    "[x] [Server thread/INFO] [m/]: cena: chamber_aviso_saida capacidade 1000\n"
                )
            visivel = cl.ler_visibilidade(caminho)
        self.assertEqual(visivel["chamber_aba_upgrades"], {"aba_avisos": False, "aba_xp": False})
        self.assertEqual(visivel["chamber_aviso_saida"], {"aba_avisos": True, "aba_xp": True})
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        self.assertEqual(cl.ausentes_do_caso(por_nome["chamber_aba_upgrades"], {"chamber_aba_upgrades": visivel["chamber_aba_upgrades"]}), ["aba_avisos", "aba_xp"])
        self.assertEqual(cl.ausentes_do_caso(por_nome["chamber_aviso_saida"], {"chamber_aviso_saida": visivel["chamber_aviso_saida"]}), [])
        # Sem o log (ou sem o caso nele) vale a regra por nome.
        self.assertEqual(cl.ler_visibilidade(os.path.join("/nao/existe", "oficial.log")), {})
        self.assertIn("aba_avisos", cl.ausentes_do_caso(por_nome["chamber_xp_com"], {}))

    def test_o_quimico_de_cada_tanque_parado_tem_um_ciclo_de_referencia_do_mesmo_nivel(self):
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        for (quimico, quantidade), nome in cl.CICLO_DO_QUIMICO.items():
            caso = por_nome[nome]
            self.assertEqual(caso["args"]["quimico"], f"{quimico}:{quantidade}")
            self.assertIn("ciclo", caso["args"])
        # Todo caso do Infuser com tanque animado e sem ciclo próprio tem referência: senão a barra dele não se explica por fase.
        for caso in por_nome.values():
            if "quimico" in caso["args"] and "ciclo" not in caso["args"]:
                self.assertIsNotNone(cl.ciclo_do_caso(caso), caso["nome"])
        # E o do Heat Generator com lava: o medidor dele é a mesma tira animada, do mesmo nível (`CICLO_DA_LAVA`).
        for (nivel, hover), nome in cl.CICLO_DA_LAVA.items():
            self.assertEqual(por_nome[nome]["args"]["lava"], str(nivel))
            self.assertEqual(por_nome[nome]["args"].get("hover") == cena_telas.HOVER["generator"]["medidor_lava"], hover, nome)
            self.assertIn("ciclo", por_nome[nome]["args"])
            self.assertEqual(por_nome[nome]["args"]["energia"], "cap", "a lava do ciclo não queima")
        for caso in por_nome.values():
            if "lava" in caso["args"] and "ciclo" not in caso["args"] and caso["args"].get("energia") == "cap":
                self.assertIsNotNone(cl.ciclo_do_caso(caso), caso["nome"])
        # Os casos de medidor parado têm a energia cheia: a lava queimando baixa o nível, e o print deixaria de ser o do ciclo.
        for nome in ("generator_lava_meio", "generator_lava_cheia", "generator_hover_barra", "generator_hover_medidor"):
            self.assertEqual(por_nome[nome]["args"]["energia"], "cap", nome)

    def test_o_port_tira_um_passo_a_menos_antes_do_tick_que_a_janela_de_upgrades_pede(self):
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        self.assertTrue(cena_telas.abre_a_janela(por_nome["chamber_aba_instalando"]))
        self.assertFalse(cena_telas.abre_a_janela(por_nome["chamber_aba_fechada"]))
        self.assertFalse(cena_telas.abre_a_janela(por_nome["chamber_xp_clique"]))
        # 10 passos do original são 9 do port mais o tick depois de abrir; sem a janela, os mesmos.
        self.assertEqual(cena_telas.passos_do_port(por_nome["chamber_aba_instalando"]), 9)
        self.assertEqual(cena_telas.passos_do_port(por_nome["chamber_aba_instalado"]), 29)
        self.assertEqual(cena_telas.passos_do_port(por_nome["chamber_aba_vazia"]), 1)
        self.assertEqual(cena_telas.passos_do_port(por_nome["chamber_operacao"]), 100)
        self.assertEqual(cena_telas.passos_do_port(por_nome["chamber_vazia"]), 2)

    @staticmethod
    def _barra(img, altura, cor, trocados=0):
        """A barra de químico do Infuser em escala 2 com `altura` linhas de GUI cheias de `cor`, de baixo para cima, e `trocados` pixels da última linha
        com o vermelho trocado (1 nível, dentro do teto). Devolve a própria imagem."""
        escala = 2
        rect = ct.elementos_da_tela("infuser")["barra_quimico"]
        ox, oy = ct.origem_da_tela(LARGURA_JANELA, ALTURA_JANELA, escala)
        _, _, rgba = img
        x0, y0 = ox + rect[0] * escala, oy + rect[1] * escala
        for linha in range(altura * escala):
            y = y0 + (rect[3] * escala - 1 - linha)
            for x in range(x0, x0 + rect[2] * escala):
                rgba[(y * LARGURA_JANELA + x) * 4 : (y * LARGURA_JANELA + x) * 4 + 3] = bytes(cor)
        for k in range(trocados):
            rgba[((y0 + rect[3] * escala - 1) * LARGURA_JANELA + x0 + k) * 4] ^= 0x01
        return img

    def _lote_sintetico(self, pasta, altura_original, altura_port, desvio_original, cor_original=(27, 193, 60)):
        """Prints sintéticos de um caso do Infuser com carbono: a barra de químico com `altura` linhas de GUI cheias, um quadro de ciclo por passo do
        port (cada um de uma cor), e o original com `desvio_original` pixels trocados em relação ao quadro 7 (ou com `cor_original`)."""
        barra = self._barra

        def grava(nome, img):
            ct.gravar_png(os.path.join(pasta, nome), *img)

        os.makedirs(os.path.join(pasta, "o"), exist_ok=True)
        os.makedirs(os.path.join(pasta, "p"), exist_ok=True)
        ciclo = next(c for c in cena_telas.todos_os_casos() if c["nome"] == cl.CICLO_DO_QUIMICO[("carbon", 500)])
        for k in range(cena_telas.ciclos(ciclo, "port")):
            quadro = imagem()
            barra(quadro, 27, (20 + k, 200 - k, 60))
            grava(f"p/port_{ciclo['nome']}_e2_{k}.png", quadro)
        caso = cena_telas.caso("infuser_x", "infuser", quimico="carbon:500")
        original = imagem()
        barra(original, altura_original, cor_original, desvio_original)
        port = imagem()
        barra(port, altura_port, (27, 193, 60))
        grava("o/oficial_infuser_x_e2.png", original)
        grava("p/port_infuser_x_e2.png", port)
        return caso, os.path.join(pasta, "o"), os.path.join(pasta, "p")

    def test_a_fase_do_quimico_so_fecha_com_o_mesmo_nivel_e_um_quadro_do_ciclo(self):
        with tempfile.TemporaryDirectory() as pasta:
            caso, o, p = self._lote_sintetico(pasta, 27, 27, 0)
            ok, texto, medida = cl.fase_do_quimico(caso, 2, o, p)
            self.assertTrue(ok, texto)
            self.assertEqual(medida, {"diferentes": 0, "maior": 0})
        with tempfile.TemporaryDirectory() as pasta:
            # O original com 3 pixels um nível fora do quadro do ciclo (a mistura H26 do sprite): passa, dentro do teto absoluto.
            caso, o, p = self._lote_sintetico(pasta, 27, 27, 3)
            ok, texto, medida = cl.fase_do_quimico(caso, 2, o, p)
            self.assertTrue(ok, texto)
            self.assertEqual(medida, {"diferentes": 3, "maior": 1})
        with tempfile.TemporaryDirectory() as pasta:
            # O tanque ocupa mais linhas no port do que no original: é nível, não fase.
            caso, o, p = self._lote_sintetico(pasta, 27, 30, 0)
            self.assertFalse(cl.fase_do_quimico(caso, 2, o, p)[0])
        with tempfile.TemporaryDirectory() as pasta:
            caso, o, p = self._lote_sintetico(pasta, 27, 27, 0)
            sem_ciclo = cena_telas.caso("infuser_sem", "infuser", quimico="bio:7")
            self.assertEqual(cl.fase_do_quimico(sem_ciclo, 2, o, p), (False, "sem ciclo de referência", None))
        with tempfile.TemporaryDirectory() as pasta:
            # O print do port tem de ser um quadro exato do ciclo do port: um pixel fora de todos e a fase não fecha, mesmo com a barra igual à do original.
            caso, o, p = self._lote_sintetico(pasta, 27, 27, 0)
            caminho = os.path.join(p, "port_infuser_x_e2.png")
            imagem_port = ct.ler_png(caminho)
            imagem_port[2][((ct.origem_da_tela(LARGURA_JANELA, ALTURA_JANELA, 2)[1] + 100) * LARGURA_JANELA + ct.origem_da_tela(LARGURA_JANELA, ALTURA_JANELA, 2)[0] + 16) * 4] ^= 0x40
            ct.gravar_png(caminho, *imagem_port)
            ok, texto, _ = cl.fase_do_quimico(caso, 2, o, p)
            self.assertFalse(ok, texto)

    def test_a_barra_de_quimico_errada_passa_do_teto_absoluto(self):
        """O teto absoluto (`TETO_DO_RESIDUO`, medido no lote: um nível por canal e os pixels do pior ciclo) segura a barra com outra cor ou outro nível no
        original, e não depende do ciclo que se compara."""
        px, canal = cl.TETO_DO_RESIDUO["barra_quimico_fase"][2]
        with tempfile.TemporaryDirectory() as pasta:
            # A cor: o quadro 7 do ciclo, com cada canal `canal + 1` acima. Em tudo o mais o original é o quadro.
            caso, o, p = self._lote_sintetico(pasta, 27, 27, 0, cor_original=(27 + canal + 1, 193 - canal - 1, 60 + canal + 1))
            ok, texto, medida = cl.fase_do_quimico(caso, 2, o, p)
            self.assertFalse(ok, texto)
            self.assertGreater(medida["maior"], canal)
        with tempfile.TemporaryDirectory() as pasta:
            # O nível: o original com linhas de GUI a menos que o ciclo do port, o bastante para passar do teto de pixels.
            linhas = px // (6 * 4) + 1
            caso, o, p = self._lote_sintetico(pasta, 27 - linhas, 27, 0)
            ok, texto, medida = cl.fase_do_quimico(caso, 2, o, p)
            self.assertFalse(ok, texto)
            self.assertGreater(medida["diferentes"], px)

    def test_no_ciclo_a_barra_de_quimico_errada_e_diferenca_de_verdade(self):
        """O ramo de ciclo do lote: o original e o port tiram o mesmo ciclo, e a barra do original com outra cor passa do teto e conta, mesmo casando
        cada print com o quadro mais próximo do port."""
        caso = cena_telas.caso("infuser_ciclo_x", "infuser", quimico="carbon:500", ciclo="2", ciclo_port="2")
        _, canal = cl.TETO_DO_RESIDUO["barra_quimico_fase"][2]
        for desvio, esperado in ((0, 0), (canal + 1, 1)):
            with tempfile.TemporaryDirectory() as pasta:
                o, p, saida = (os.path.join(pasta, x) for x in ("o", "p", "s"))
                os.makedirs(o)
                os.makedirs(p)
                for k in range(2):
                    cor = (40 + 30 * k, 120, 60)
                    ct.gravar_png(os.path.join(p, f"port_infuser_ciclo_x_e2_{k}.png"), *self._barra(imagem(), 27, cor))
                    errada = (cor[0] + desvio, cor[1], cor[2])
                    ct.gravar_png(os.path.join(o, f"oficial_infuser_ciclo_x_e2_{k}.png"), *self._barra(imagem(), 27, errada))
                with mock.patch.object(cena_telas, "todos_os_casos", return_value=[caso]), contextlib.redirect_stdout(io.StringIO()) as texto:
                    codigo = cl.principal([o, p, saida, "2"])
                self.assertEqual(codigo, esperado, texto.getvalue())

    def test_o_icone_da_lista_em_guiscale_3_so_vale_se_o_mesmo_estado_deu_o_icone_certo_em_2(self):
        """Em guiScale 3 o teto do ícone é largo (o item em escala cai entre pixels, H25): a prova de que o ícone é o certo é o mesmo estado em guiScale 2,
        com zero pixel. Sem ela, qualquer diferença no ícone é de verdade, e em guiScale 2 o teto é zero."""
        px, canal = cl.TETO_DO_RESIDUO["janela_lista_icone"][3]
        dados = {"janela_lista_icone_1": {"diferentes": px, "maior": canal}}
        self.assertEqual(cl.residuos_e_reais(dados, 3, icone_provado=True), ({"janela_lista_icone_1": px}, {}))
        self.assertEqual(cl.residuos_e_reais(dados, 3, icone_provado=False), ({}, {"janela_lista_icone_1": px}))
        self.assertEqual(cl.residuos_e_reais({"janela_lista_icone_1": {"diferentes": 1, "maior": 1}}, 2), ({}, {"janela_lista_icone_1": 1}))
        # O que não é ícone não depende da prova.
        self.assertEqual(cl.residuos_e_reais({"janela_fundo": {"diferentes": 9, "maior": 3}}, 3, icone_provado=False), ({"janela_fundo": 9}, {}))

    def test_a_particula_da_maquina_e_diferenca_conhecida_e_nao_residuo(self):
        self.assertFalse(cl.residuo("particulas_da_maquina"))
        self.assertFalse(cl.residuo("alem_mundo_atras"))
        dados = {"particulas_da_maquina": {"diferentes": 614, "maior": 18}, "janela_fundo": {"diferentes": 9, "maior": 1}}
        conhecidas = cl.conhecidas_de(dados)
        self.assertEqual(conhecidas, {"particulas_da_maquina": 614})
        self.assertEqual(list(dados), ["janela_fundo"])
        texto = cl.texto_das_conhecidas(conhecidas)
        self.assertIn("particulas_da_maquina 614 px", texto)
        self.assertIn("Machine.java:23", texto)
        self.assertEqual(cl.texto_das_conhecidas({}), "")

    def test_o_mundo_que_passa_do_limite_cai_nas_particulas_so_dentro_da_caixa_e_so_com_a_maquina_acesa(self):
        a = imagem()
        # Dois pixels do mundo atrás da tela (a margem à esquerda dela), bem mais claros no port: um dentro da caixa das partículas, outro fora.
        x_dentro, y = na_tela(-20, 110)
        x_fora = na_tela(-10, 110)[0]
        caixa = (x_dentro - 4, y - 4, 8, 8)
        dentro, fora = copia(a), copia(a)
        pintar(dentro, x_dentro, y, cor=(200, 200, 200))
        pintar(fora, x_fora, y, cor=(200, 200, 200))
        kw = dict(margem=30, limite_mundo=2, ausentes=["aba_avisos", "aba_xp"])
        r, _ = ct.comparar(a, dentro, "electric", ESCALA, particulas=caixa, **kw)
        self.assertEqual(r["elementos"].get("particulas_da_maquina", {}).get("diferentes"), 1, r["elementos"])
        self.assertNotIn("alem_mundo_atras", r["elementos"])
        r, _ = ct.comparar(a, fora, "electric", ESCALA, particulas=caixa, **kw)
        self.assertNotIn("particulas_da_maquina", r["elementos"])
        self.assertEqual(r["elementos"]["alem_mundo_atras"]["diferentes"], 1)
        # Sem a máquina acesa não há caixa: o mesmo pixel é diferença de verdade.
        r, _ = ct.comparar(a, dentro, "electric", ESCALA, **kw)
        self.assertEqual(r["elementos"]["alem_mundo_atras"]["diferentes"], 1)
        # E a caixa do lote cai sobre a face da frente da máquina nos prints de 1920 por 1080.
        x0, y0, w, h = cl.CAIXA_DAS_PARTICULAS
        self.assertTrue(x0 <= 705 and x0 + w >= 960 and y0 <= 660 and y0 + h >= 955)

    def test_o_limite_do_mundo_e_o_do_nivel_de_luz_do_caso(self):
        """O lote usa o limite do nível de luz do caso, não um só: com a tabela mudada, o mesmo pixel passa num caso e falha no outro."""
        parada = cena_telas.caso("m_aba_energia_parada", "chamber", energia="0")
        acesa = cena_telas.caso("m_aba_energia_acesa", "chamber", energia="cap", entrada="coal:2", passos="100")
        a, b = imagem(), imagem()
        pintar(b, *na_tela(-20, 110), cor=(124, 134, 144))  # o mundo atrás da tela: 4 níveis por canal
        with tempfile.TemporaryDirectory() as pasta:
            for caso in (parada, acesa):
                for lado, prefixo, img in (("o", "oficial_", a), ("p", "port_", b)):
                    os.makedirs(os.path.join(pasta, lado), exist_ok=True)
                    ct.gravar_png(os.path.join(pasta, lado, f"{prefixo}{caso['nome']}_e2.png"), *img)
            resultados = []
            for tabela in ({0: 4, 8: 3, 13: 4}, {0: 3, 8: 4, 13: 4}):
                with mock.patch.object(cena_telas, "todos_os_casos", return_value=[parada, acesa]), mock.patch.object(cl, "LIMITE_MUNDO_POR_LUZ", tabela), \
                        contextlib.redirect_stdout(io.StringIO()) as texto:
                    cl.principal([os.path.join(pasta, "o"), os.path.join(pasta, "p"), os.path.join(pasta, "s"), "2"])
                linhas = {l.split()[1]: l.split()[0] for l in texto.getvalue().splitlines() if l.startswith(("DIF", "ok ", "NE "))}
                resultados.append((linhas["m_aba_energia_parada_e2"], linhas["m_aba_energia_acesa_e2"]))
        # Dentro do limite o mundo atrás da tela é `NE` (não explicado), nunca `ok`; fora dele é `DIF`.
        self.assertEqual(resultados, [("NE", "DIF"), ("DIF", "NE")])

    def test_as_particulas_do_caso_anterior_tambem_contam(self):
        """O original roda os casos em sequência no mesmo lugar: um caso parado logo depois de um aceso ainda tem no ar as partículas dele."""
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        for nome in ("chamber_operacao", "crusher_aba_energia", "smelter_aba_energia", "generator_aba_energia"):
            self.assertTrue(cl.com_particulas(por_nome[nome]), nome)
        for nome in ("chamber_vazia", "chamber_aba_vazia", "chamber_cheia"):
            self.assertFalse(cl.com_particulas(por_nome[nome]), nome)
        # Um caso de fora da tabela não tem anterior.
        self.assertFalse(cl.com_particulas(cena_telas.caso("fora", "chamber", energia="0")))

    def test_o_lote_conta_a_particula_da_maquina_acesa_como_diferenca_conhecida_e_a_parada_nao_a_tem(self):
        parada = cena_telas.caso("m_aba_energia_parada", "chamber", energia="0")
        acesa = cena_telas.caso("m_aba_energia_acesa", "chamber", energia="cap", entrada="coal:2", passos="100")
        a, b = imagem(), imagem()
        x, y = na_tela(-20, 110)
        pintar(b, x, y, cor=(200, 200, 200))
        with tempfile.TemporaryDirectory() as pasta:
            for caso in (parada, acesa):
                for lado, prefixo, img in (("o", "oficial_", a), ("p", "port_", b)):
                    os.makedirs(os.path.join(pasta, lado), exist_ok=True)
                    ct.gravar_png(os.path.join(pasta, lado, f"{prefixo}{caso['nome']}_e2.png"), *img)
            with mock.patch.object(cena_telas, "todos_os_casos", return_value=[parada, acesa]), mock.patch.object(cl, "CAIXA_DAS_PARTICULAS", (x - 4, y - 4, 8, 8)), \
                    contextlib.redirect_stdout(io.StringIO()) as texto:
                codigo = cl.principal([os.path.join(pasta, "o"), os.path.join(pasta, "p"), os.path.join(pasta, "s"), "2"])
        linhas = {l.split()[1]: l for l in texto.getvalue().splitlines() if l.startswith(("DIF", "ok "))}
        self.assertTrue(linhas["m_aba_energia_parada_e2"].startswith("DIF"), linhas)
        self.assertTrue(linhas["m_aba_energia_acesa_e2"].startswith("ok "), linhas)
        self.assertIn("CONHECIDA, não portada: particulas_da_maquina 1 px", linhas["m_aba_energia_acesa_e2"])
        self.assertIn("Machine.java:23", linhas["m_aba_energia_acesa_e2"])
        self.assertIn("1 com diferença conhecida (não portada)", texto.getvalue())
        self.assertEqual(codigo, 1)  # a parada tem diferença de verdade

    def test_os_controles_da_luz_so_rodam_quando_pedidos_pelo_nome(self):
        """Os controles comparam o mundo sem tela e a luz, não a tela: ficam fora do lote e dos dois roteiros, a menos que `--so` os peça."""
        self.assertFalse([c["nome"] for c in cena_telas.todos_os_casos() if c["nome"].startswith("controle_")])
        self.assertFalse([c["nome"] for c in cena_telas.casos_do_roteiro() if c["nome"].startswith("controle_")])
        pedidos = cena_telas.casos_do_roteiro(["controle_luz_8", "chamber_vazia"])
        self.assertEqual(sorted(c["nome"] for c in pedidos), ["chamber_vazia", "controle_luz_8"])
        niveis = {c["nome"]: c["args"]["luz"] for c in cena_telas.casos_controle() if c["nome"].startswith("controle_luz_") and "_semtela" not in c["nome"]}
        self.assertEqual(niveis, {"controle_luz_4": "4", "controle_luz_8": "8", "controle_luz_12": "12", "controle_luz_15": "15"})
        for c in cena_telas.casos_controle():
            self.assertEqual("semtela" in c["args"], c["nome"].endswith("semtela"), c["nome"])

    def test_o_port_roda_com_as_opcoes_graficas_do_original_e_confere_cada_uma(self):
        """O Horizon nasce com outras opções (preset personalizado, sem vinheta, sem filtragem, distância 12) e o mundo atrás da tela sairia diferente por
        isso: o roteiro põe as do `options.txt` do original e falha se alguma não ficou."""
        esperadas = dict(cena_telas.OPCOES_DO_ORIGINAL)
        self.assertEqual(esperadas["graphicsPreset"], "fancy")
        self.assertEqual({k: esperadas[k] for k in ("gamma", "ao", "particles", "vignette", "textureFiltering", "renderDistance")},
                         {"gamma": "0.5", "ao": "true", "particles": "0", "vignette": "true", "textureFiltering": "1", "renderDistance": "16"})
        self.assertEqual(next(iter(esperadas)), "graphicsPreset", "o preset reescreve as outras opções: vai primeiro")
        chamadas = []

        def ctl_falso(*args, **kw):
            chamadas.append(args)
            if args[0] == "options-get":
                return '{"options": {' + ",".join(f'"{k}": {{"value": "{v}"}}' for k, v in lidas.items()) + "}}"
            return ""

        lidas = dict(esperadas)
        with mock.patch.object(cena_telas, "ctl", ctl_falso):
            cena_telas.iguala_as_opcoes()
        self.assertEqual([a for a in chamadas if a[0] == "options-set"], [("options-set", k, v) for k, v in esperadas.items()])
        lidas["vignette"] = "false"
        with mock.patch.object(cena_telas, "ctl", ctl_falso), self.assertRaisesRegex(RuntimeError, "vignette"):
            cena_telas.iguala_as_opcoes()

    def test_o_port_volta_a_vista_de_frente_depois_do_interact(self):
        """O `interact` vira o jogador para o bloco (inclinação 20), e o `CenaTelas` fica de frente: sem o `look 0 0` depois dele o mundo atrás da tela é
        outra vista. Sem tela (`semtela`) não há `interact`."""
        chamadas = []
        with mock.patch.object(cena_telas, "ctl", lambda *a, **k: chamadas.append(a) or ""), mock.patch.object(cena_telas, "comando", lambda *a: None):
            cena_telas.tira_port(cena_telas.caso("m", "chamber", energia="0"), 3, 2, "/x")
        nomes = [a[0] for a in chamadas]
        self.assertLess(nomes.index("interact"), len(nomes) - 1 - nomes[::-1].index("look"))
        self.assertLess(nomes.index("interact"), nomes.index("screenshot"))
        depois = nomes[nomes.index("interact") :]
        self.assertEqual(depois[depois.index("wait") + 1], "look")
        chamadas.clear()
        with mock.patch.object(cena_telas, "ctl", lambda *a, **k: chamadas.append(a) or ""):
            cena_telas.tira_port(cena_telas.caso("m", "chamber", semtela="1"), 3, 2, "/x")
        self.assertNotIn("interact", [a[0] for a in chamadas])
        self.assertEqual([a for a in chamadas if a[0] == "screenshot"], [("screenshot", "/x/port_m_e2.png")])

    def test_a_fase_do_medidor_de_lava_fecha_pelo_ciclo_do_mesmo_nivel_e_tem_teto(self):
        """O medidor de lava do Heat Generator é a mesma tira animada: o caso de lava sem ciclo se explica pelo ciclo do mesmo nível, com o teto
        absoluto de `medidor_lava_fase` (0 pixel e 0 níveis)."""
        rect = ct.elementos_da_tela("generator")["medidor_lava"]
        ox, oy = ct.origem_da_tela(LARGURA_JANELA, ALTURA_JANELA, 2)

        def medidor(img, cor, trocados=0):
            _, _, rgba = img
            for y in range(oy + rect[1] * 2, oy + (rect[1] + rect[3]) * 2):
                for x in range(ox + rect[0] * 2, ox + (rect[0] + rect[2]) * 2):
                    rgba[(y * LARGURA_JANELA + x) * 4 : (y * LARGURA_JANELA + x) * 4 + 3] = bytes(cor)
            for k in range(trocados):
                rgba[((oy + rect[1] * 2) * LARGURA_JANELA + ox + rect[0] * 2 + k) * 4] ^= 0x01
            return img

        ciclo = next(c for c in cena_telas.todos_os_casos() if c["nome"] == cl.CICLO_DA_LAVA[(500, False)])
        caso = cena_telas.caso("generator_lava_x", "generator", energia="0", lava="500")
        px, canal = cl.TETO_DO_RESIDUO["medidor_lava_fase"][2]
        # O teto da lava é 0 e 0 (o `lava_still` não tem mistura): nenhum pixel trocado passa.
        for cor_original, trocados, esperado in (((200, 90, 20), 0, True), ((200, 90, 20), 1, False), ((200, 90 + canal + 1, 20), 0, False)):
            with tempfile.TemporaryDirectory() as pasta:
                for lado in ("o", "p"):
                    os.makedirs(os.path.join(pasta, lado))
                for k in range(cena_telas.ciclos(ciclo, "port")):
                    ct.gravar_png(os.path.join(pasta, "p", f"port_{ciclo['nome']}_e2_{k}.png"), *medidor(imagem(), (150 + k, 90, 20)))
                ct.gravar_png(os.path.join(pasta, "p", "port_generator_lava_x_e2.png"), *medidor(imagem(), (200, 90, 20)))
                # O quadro 50 do ciclo é (200, 90, 20): o original é ele, com os pixels trocados ou a cor errada.
                ct.gravar_png(os.path.join(pasta, "o", "oficial_generator_lava_x_e2.png"), *medidor(imagem(), cor_original, trocados))
                ok, texto, medida = cl.fase_do_quimico(caso, 2, os.path.join(pasta, "o"), os.path.join(pasta, "p"))
            self.assertEqual(ok, esperado, texto)
        self.assertEqual(cl.animado_da_tela("generator"), "medidor_lava")
        self.assertEqual(cl.animado_da_tela("infuser"), "barra_quimico")

    def test_os_tetos_dos_residuos_sao_os_que_o_lote_mediu(self):
        """Os números do lote (docs/plano-paridade.md, "Tetos dos resíduos"): um teto mais largo esconderia uma barra, um ícone ou uma lava errados."""
        self.assertEqual(cl.TETO_DO_RESIDUO, {
            "janela_lista_icone": {2: (0, 0), 3: (107, 206)},
            "tooltip_canto": {2: (6, 72), 3: (21, 72)},
            "barra_quimico_fase": {2: (192, 1), 3: (342, 1)},
            "medidor_lava_fase": {2: (0, 0), 3: (0, 0)},
        })

    def test_o_ciclo_com_o_cursor_conta_o_tooltip_com_a_tolerancia_dele(self):
        """Um ciclo com `hover` (o medidor de lava com o tooltip) passa o retângulo do tooltip ao comparador, como os casos sem ciclo: o fundo de 94 % de opacidade
        deixa passar o mundo, e uma diferença de 10 níveis dentro da caixa não é diferença de verdade."""
        hover = cena_telas.HOVER["generator"]["medidor_lava"]
        caso = cena_telas.caso("generator_ciclo_h", "generator", energia="cap", lava="500", hover=hover, ciclo="2", ciclo_port="2")
        for desvio, codigo_esperado in ((10, 0), (40, 1)):
            with tempfile.TemporaryDirectory() as pasta:
                o, p, saida = (os.path.join(pasta, x) for x in ("o", "p", "s"))
                os.makedirs(o)
                os.makedirs(p)
                caixa_x, caixa_y = na_tela(hover[0] + 4, hover[1] + 4)
                for k in range(2):
                    port, original = imagem(), imagem()
                    for img in (port, original):
                        for y in range(caixa_y, caixa_y + 24):
                            for x in range(caixa_x, caixa_x + 60):
                                pintar(img, x, y, cor=(30, 0, 60))
                    pintar(original, *na_tela(hover[0] + 10, hover[1] + 8), cor=(30, 0 + desvio, 60))
                    ct.gravar_png(os.path.join(p, f"port_generator_ciclo_h_e2_{k}.png"), *port)
                    ct.gravar_png(os.path.join(o, f"oficial_generator_ciclo_h_e2_{k}.png"), *original)
                with mock.patch.object(cena_telas, "todos_os_casos", return_value=[caso]), contextlib.redirect_stdout(io.StringIO()) as texto:
                    codigo = cl.principal([o, p, saida, "2"])
            self.assertEqual(codigo, codigo_esperado, (desvio, texto.getvalue()))

    def test_o_limite_do_mundo_por_luz_e_o_que_se_mediu(self):
        """Os controles de luz 4, 8, 12 e 15 deram 1 por canal com o escurecimento refeito (`container_dim`): o limite de cada nível é esse, e um limite mais
        largo esconderia diferença (docs/plano-paridade.md, "Tetos dos resíduos")."""
        self.assertEqual(cl.LIMITE_MUNDO_POR_LUZ, {0: 1, 8: 1, 13: 1})

    def test_os_slots_de_armadura_sob_a_sombra_da_janela_contam_como_slots_e_nao_como_mundo(self):
        """Na janela de upgrades do Heat Generator a coluna de armadura (x -21 a -4) passa por baixo da sombra da janela (x a partir de -14): a diferença que
        o original desenha ali são os slots, e sai em `slots_armadura`, não em `alem_janela_fundo`."""
        gerador = cena_telas.caso("g_aba_x", "generator", energia="0")
        prioridade = cl.prioridade_do_caso(gerador, {}, ct.JANELA)
        chaves = list(prioridade)
        self.assertLess(chaves.index("slots_armadura"), chaves.index("janela_fundo"))
        self.assertGreater(chaves.index("slots_armadura"), chaves.index("janela_titulo"), "as filhas da janela vêm antes")
        a, b = imagem(), imagem()
        pintar(b, *na_tela(-10, 20), cor=(250, 250, 250))  # na coluna de armadura e na sombra da janela
        r, _ = ct.comparar(a, b, "generator", ESCALA, margem=28, prioridade=prioridade, mascarar_abas=False, limite_mundo=2)
        self.assertEqual(r["elementos"]["slots_armadura"]["diferentes"], 1)
        self.assertNotIn("alem_janela_fundo", r["elementos"])
        # O Infuser e as máquinas elétricas não têm a coluna: a janela segue como era.
        eletrica = cl.prioridade_do_caso(cena_telas.caso("c_aba_x", "chamber", energia="0"), {}, ct.JANELA)
        self.assertNotIn("slots_armadura", eletrica)
        self.assertEqual(list(eletrica), list(ct.JANELA))

    def test_o_que_nao_esta_portado_segue_dif_e_o_relatorio_diz_a_causa(self):
        """Os slots de armadura do Heat Generator: diferença de verdade, sem resíduo que a esconda, e o relatório diz que não estão portados."""
        self.assertFalse(cl.residuo("slots_armadura"))
        self.assertEqual(cl.marca_do_estado({"slots_armadura": 2608}, {}), "DIF")
        texto = cl.texto_dos_nao_portados({"slots_armadura": 2608, "medidor_lava": 5})
        self.assertIn("NÃO PORTADO: slots_armadura", texto)
        self.assertIn("armorSideBar(-20, 11, 0)", texto)
        self.assertNotIn("medidor_lava", texto)
        self.assertEqual(cl.texto_dos_nao_portados({"medidor_lava": 5}), "")
        # No lote, o estado sai `DIF` com a causa na linha, e o código de saída é 1.
        a, b = imagem(), imagem()
        gerador = cena_telas.caso("g_aba_energia_x", "generator", energia="0")
        x, y = na_tela(-21 + 8, 10 + 8)  # dentro do primeiro slot de armadura
        pintar(b, x, y, cor=(250, 250, 250))
        with tempfile.TemporaryDirectory() as pasta:
            for lado, prefixo, img in (("o", "oficial_", a), ("p", "port_", b)):
                os.makedirs(os.path.join(pasta, lado), exist_ok=True)
                ct.gravar_png(os.path.join(pasta, lado, f"{prefixo}{gerador['nome']}_e2.png"), *img)
            with mock.patch.object(cena_telas, "todos_os_casos", return_value=[gerador]), contextlib.redirect_stdout(io.StringIO()) as texto:
                codigo = cl.principal([os.path.join(pasta, "o"), os.path.join(pasta, "p"), os.path.join(pasta, "s"), "2"])
        linha = next(l for l in texto.getvalue().splitlines() if "g_aba_energia_x_e2" in l)
        self.assertTrue(linha.startswith("DIF"), linha)
        self.assertIn("NÃO PORTADO: slots_armadura", linha)
        self.assertEqual(codigo, 1)

    def test_o_mundo_atras_da_tela_sem_causa_provada_e_ne_e_nao_ok(self):
        self.assertEqual(cl.marca_do_estado({}, {}), "ok ")
        self.assertEqual(cl.marca_do_estado({}, {"mundo_atras": 5}), "NE ")
        self.assertEqual(cl.marca_do_estado({"barra_quimico": 1}, {"mundo_atras": 5}), "DIF")
        residuos = {"mundo_atras": 5, "canto_cima_esquerda": 12, "janela_fundo": 3, "janela_aceitos": 493, "barra_quimico_fase": 40, "tooltip_canto_0": 6}
        self.assertEqual(cl.nao_explicadas_de(residuos), {"mundo_atras": 5, "canto_cima_esquerda": 12, "janela_fundo": 3})
        self.assertEqual(cl.texto_das_nao_explicadas({}, {"maior_do_mundo": 0}), "")
        self.assertIn("NÃO EXPLICADA", cl.texto_das_nao_explicadas({"mundo_atras": 5}, {"maior_do_mundo": 1}))

    def test_a_maquina_ativa_so_com_receita_energia_saida_livre_e_passos(self):
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        for nome in ("chamber_operacao", "crusher_operacao", "infuser_operacao", "chamber_aba_energia_hover_ativa", "generator_queima",
                     "generator_aba_calor_hover_produzindo"):
            self.assertTrue(cena_telas.maquina_ativa(por_nome[nome]), nome)
        for nome in ("chamber_vazia", "chamber_aviso_energia", "chamber_aviso_saida", "chamber_cheia", "generator_vazia", "generator_aba_calor_hover",
                     "chamber_aba_energia_hover_cheia", "generator_lava_ciclo"):
            self.assertFalse(cena_telas.maquina_ativa(por_nome[nome]), nome)
        # O gerador com a lava mas com a energia cheia não queima (`generator_lava_ciclo`): só com energia que ainda cabe ele está aceso.
        self.assertTrue(cena_telas.maquina_ativa(cena_telas.caso("g", "generator", energia="120", lava="500", passos="100")))
        self.assertFalse(cena_telas.maquina_ativa(cena_telas.caso("g", "generator", energia="cap", lava="500", passos="100")))
        # Menos de 2 passos: a máquina ainda não operou, mesmo com receita, energia e saída livre.
        base = dict(energia="cap", entrada="coal:2")
        self.assertTrue(cena_telas.maquina_ativa(cena_telas.caso("m", "chamber", passos="2", **base)))
        self.assertFalse(cena_telas.maquina_ativa(cena_telas.caso("m", "chamber", passos="1", **base)))
        self.assertFalse(cena_telas.maquina_ativa(cena_telas.caso("m", "chamber", passos="0", **base)))

    def test_a_luz_do_bloco_aceso_vem_do_java(self):
        """`Attributes.ACTIVE_LIGHT` (8) nas quatro máquinas e `ACTIVE_MELT_LIGHT` (13) no Heat Generator; apagado emite 0."""
        for maquina in ("chamber", "crusher", "smelter", "infuser"):
            self.assertEqual(cena_telas.luz_do_caso(cena_telas.caso("m", maquina, energia="cap", entrada="coal:2", passos="100")), 8, maquina)
            self.assertEqual(cena_telas.luz_do_caso(cena_telas.caso("m", maquina, energia="0")), 0, maquina)
        self.assertEqual(cena_telas.luz_do_caso(cena_telas.caso("g", "generator", energia="120", lava="500", passos="100")), 13)
        self.assertEqual(cena_telas.luz_do_caso(cena_telas.caso("g", "generator", energia="0")), 0)
        # Todo nível de luz que um caso pode ter tem o limite do mundo dele.
        for c in cena_telas.todos_os_casos():
            self.assertIn(cena_telas.luz_do_caso(c), cl.LIMITE_MUNDO_POR_LUZ, c["nome"])

    def test_o_tooltip_cobre_o_que_esta_embaixo_com_a_tolerancia_dele_e_o_de_item_tem_os_cantos_a_parte(self):
        por_nome = {c["nome"]: c for c in cena_telas.todos_os_casos()}
        caixa = (20, 30, 40, 20)
        item = cl.prioridade_do_caso(por_nome["chamber_hover_slot_item"], {"tooltip": caixa}, ct.JANELA)
        self.assertEqual(list(item)[:5], ["tooltip_canto_0", "tooltip_canto_1", "tooltip_canto_2", "tooltip_canto_3", "tooltip"])
        self.assertEqual(item["tooltip_canto_3"], (20 + 40 - cl.CANTO_TOOLTIP, 30 + 20 - cl.CANTO_TOOLTIP, cl.CANTO_TOOLTIP, cl.CANTO_TOOLTIP))
        self.assertEqual(list(item)[5:], list(ct.JANELA))
        barra = cl.prioridade_do_caso(por_nome["chamber_hover_barra"], {"tooltip": caixa}, None)
        self.assertEqual(list(barra), ["tooltip"])
        self.assertEqual(cl.prioridade_do_caso(por_nome["chamber_vazia"], {}, None), {})
        # Sobre o inventário: uma diferença de 10 níveis (o mundo atrás do fundo de 94 %) passa, a de um texto não, e o canto conta à parte.
        a = imagem()
        leve, forte, canto = copia(a), copia(a), copia(a)
        pintar(leve, *na_tela(30, 40), cor=(130, 140, 150))
        pintar(forte, *na_tela(30, 40), cor=(255, 255, 255))
        pintar(canto, *na_tela(20, 30), cor=(255, 255, 255))
        for img, esperado in ((leve, 0), (forte, 1)):
            resultado, _ = ct.comparar(a, img, "electric", ESCALA, regioes={"tooltip": caixa}, prioridade=item, mascarar_abas=False)
            self.assertEqual(resultado["elementos"]["tooltip"]["diferentes"], esperado)
            self.assertEqual(resultado["elementos"].get("inventario", {"diferentes": 0})["diferentes"], 0)
        resultado, _ = ct.comparar(a, canto, "electric", ESCALA, regioes={"tooltip": caixa}, prioridade=item, mascarar_abas=False)
        self.assertEqual(resultado["elementos"]["tooltip_canto_0"]["diferentes"], 1)
        self.assertEqual(resultado["elementos"]["tooltip"]["diferentes"], 0)


if __name__ == "__main__":
    unittest.main()
