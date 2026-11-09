import itertools, os, sys, unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import particulas_taxa as pt


class TaxaDasParticulas(unittest.TestCase):
    def test_a_chance_por_eixo_e_a_do_sorteio_de_duas_nextint(self):
        """`nextInt(r) - nextInt(r)` cai em `d` com chance `(r - |d|) / r^2`: contado por enumeração, para um `r` pequeno."""
        r = 6
        contagem = {}
        for a, b in itertools.product(range(r), repeat=2):
            contagem[a - b] = contagem.get(a - b, 0) + 1
        for d in range(-r - 1, r + 2):
            self.assertAlmostEqual(pt.chance_por_eixo(d, r), contagem.get(d, 0) / (r * r))
        self.assertAlmostEqual(sum(pt.chance_por_eixo(d, r) for d in range(-r, r + 1)), 1.0)

    def test_a_esperanca_a_tres_blocos_e_a_soma_dos_dois_alcances(self):
        esperado = 667 * ((1 / 16) * (1 / 16) * (13 / 256) + (1 / 32) * (1 / 32) * (29 / 1024))
        self.assertAlmostEqual(pt.esperanca_por_tick((0, 0, 3)), esperado)
        self.assertAlmostEqual(pt.esperanca_por_tick((0, 0, 3)), pt.esperanca_por_tick((0, 0, -3)))
        # Mais longe que 32 blocos o sorteio não chega.
        self.assertEqual(pt.esperanca_por_tick((40, 0, 0)), 0.0)


if __name__ == "__main__":
    unittest.main()
