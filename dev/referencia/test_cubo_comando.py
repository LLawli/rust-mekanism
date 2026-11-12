"""O `#cubo` do roteiro `cenas/cubo.json` no port (`h2_blocos.comando_do_cubo`): o cubo criativo cheio guarda `Long.MAX_VALUE`, e o `data merge` o aceita."""
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "horizon2"))

import h2_blocos

LONG_MAX = 9223372036854775807


class ComandoDoCubo(unittest.TestCase):
    def test_o_criativo_cheio_nao_passa_de_long_max(self):
        comando = h2_blocos.comando_do_cubo("#cubo 64 -60 82 1 front=output left=input right=input back=input top=input bottom=input", ["setblock 64 -60 82 mekanism:creative_energy_cube[facing=north]"])
        # 2^63 (o valor de `float(1.0) * Long.MAX_VALUE`) estoura o `data merge` e o cubo fica sem a configuração de lados.
        self.assertIn(f"energy_amount:{LONG_MAX}L", comando)
        self.assertNotIn("9223372036854775808", comando)

    def test_o_basico_leva_a_fracao_da_capacidade_e_a_configuracao(self):
        comando = h2_blocos.comando_do_cubo("#cubo 60 -60 82 0.5 front=output left=input right=input back=input top=input bottom=input", ["setblock 60 -60 82 mekanism:basic_energy_cube[facing=north]"])
        self.assertEqual(comando, "data merge block 60 -60 82 {energy:2000000,component_config:{config0:[I;4,1,1,1,1,1]}}")


if __name__ == "__main__":
    unittest.main()
