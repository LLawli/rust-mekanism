import math, os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import som_plano_analise as sp

CENA = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cenas", "som_plano.txt")


def sinal(amplitude, inicio_s=1.0, duracao_s=12.0, freq=440.0):
    """Um seno de `duracao_s` que começa em `inicio_s`, precedido e seguido de silêncio, no formato que a análise lê (taxa de 48 kHz, escala de [-1, 1])."""
    taxa = sp.TAXA
    x = [0.0] * int((inicio_s + duracao_s + 1.0) * taxa)
    for i in range(int(duracao_s * taxa)):
        x[int(inicio_s * taxa) + i] = amplitude * math.sin(2 * math.pi * freq * i / taxa)
    return x


class AnaliseDoSomPlano(unittest.TestCase):
    def test_a_cena_tem_oito_trechos_na_ordem_da_gravacao(self):
        self.assertEqual(sp.nomes_da_cena(CENA), ["padrao", "base_0_5", "base_2", "silencio_depois"] * 2)

    def test_ganho_esperado_e_a_atenuacao_linear_do_motor(self):
        d = 3.2022
        # volume 1: 1 - d/16; volume 0,5: metade do ganho do volume 1 (o alcance continua 16); volume 2: o ganho limita a 1 mas o alcance dobra (32).
        self.assertAlmostEqual(sp.ganho_esperado(1.0, d), 1 - d / 16)
        self.assertAlmostEqual(sp.ganho_esperado(0.5, d), 0.5 * (1 - d / 16))
        self.assertAlmostEqual(sp.ganho_esperado(2.0, d), 1 - d / 32)
        self.assertGreater(sp.ganho_esperado(2.0, d), sp.ganho_esperado(1.0, d))
        # Além do alcance não se ouve nada.
        self.assertEqual(sp.ganho_esperado(1.0, 20.0), 0.0)

    def test_media_desvio_e_razao(self):
        m, d = sp.media_desvio([10.0, 12.0, 14.0])
        self.assertAlmostEqual(m, 12.0)
        self.assertAlmostEqual(d, 2.0)
        r, ic, df = sp.razao([100.0, 102.0, 98.0, 101.0, 99.0], [100.0, 102.0, 98.0, 101.0, 99.0])
        self.assertAlmostEqual(r, 1.0)
        self.assertGreater(ic, 0.0)
        # Um jogo 1 % mais alto, com a variação de 1,6 % de cada lado, fica dentro do intervalo; 10 % mais alto, não.
        r, ic, _ = sp.razao([100.0, 102.0, 98.0, 101.0, 99.0, 100.0], [101.0, 103.0, 99.0, 102.0, 100.0, 101.0])
        self.assertAlmostEqual(r, 1.01, places=3)
        self.assertLess(abs(r - 1), ic * 3)
        r, ic, _ = sp.razao([100.0, 102.0, 98.0, 101.0, 99.0, 100.0], [110.0, 112.0, 108.0, 111.0, 109.0, 110.0])
        self.assertGreater(abs(r - 1), ic)

    def test_as_medidas_de_rms_de_um_seno_de_amplitude_conhecida(self):
        amplitude = 0.05
        x = sinal(amplitude)
        trecho = (int(1.0 * sp.TAXA), int(13.0 * sp.TAXA))
        m = sp.medidas(x, trecho)
        esperado = amplitude / math.sqrt(2) * 32768
        for chave in ("corpo", "janela", "laco2"):
            self.assertAlmostEqual(m[chave] / esperado, 1.0, places=2, msg=chave)
        self.assertEqual(len(m["blocos"]), 6)
        for b in m["blocos"]:
            self.assertAlmostEqual(b / esperado, 1.0, places=2)
        # O começo fino cai na subida do seno, dentro de uma janela de 20 ms.
        self.assertAlmostEqual(m["inicio_s"], 1.0, delta=0.02)
        # Um sinal com o dobro da amplitude dá o dobro do RMS em todas as medidas (a mutação que erra o ganho não passa).
        m2 = sp.medidas(sinal(2 * amplitude), trecho)
        for chave in ("corpo", "janela", "laco2"):
            self.assertAlmostEqual(m2[chave] / m[chave], 2.0, places=3)

    def test_laco2_conta_voltas_inteiras_do_laco(self):
        """Um sinal modulado com o período do laço (4,075 s) só dá o RMS verdadeiro em voltas inteiras: com 1,5 volta o RMS é outro."""
        taxa, amplitude = sp.TAXA, 0.05
        x = [0.0] * int(15 * taxa)
        for i in range(int(12 * taxa)):
            t = i / taxa
            x[taxa + i] = amplitude * math.sin(2 * math.pi * 440 * t) * (1 + 0.5 * math.sin(2 * math.pi * t / sp.LACO_S))
        m = sp.medidas(x, (taxa, 13 * taxa))
        esperado = amplitude / math.sqrt(2) * math.sqrt(1 + 0.5 ** 2 / 2) * 32768
        self.assertAlmostEqual(m["laco2"] / esperado, 1.0, delta=0.002)

    def test_trecho_curto_nao_tem_duas_voltas(self):
        x = sinal(0.05, duracao_s=5.0)
        m = sp.medidas(x, (int(1.0 * sp.TAXA), int(6.0 * sp.TAXA)))
        self.assertIsNone(m["laco2"])
        self.assertIsNotNone(m["janela"])


if __name__ == "__main__":
    unittest.main()
