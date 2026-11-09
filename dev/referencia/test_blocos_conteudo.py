"""O vidro dos tanques de fluido (`blocos_conteudo.py`): cada textura de tier é a do original com o RGB multiplicado pela cor do tier (`rastros/estrutura_modelos.json`) e o alfa intacto."""
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from comparar_tela import ler_png

HERE = os.path.dirname(os.path.abspath(__file__))
TEXTURAS = os.path.join(HERE, "..", "..", "mods/mekanism/content/assets/mekanism/textures/block/models")


class VidroDoTanque(unittest.TestCase):
    def test_cada_tier_multiplica_o_vidro_pela_cor_dele(self):
        with open(os.path.join(HERE, "rastros/estrutura_modelos.json")) as arquivo:
            cores = json.load(arquivo)["tier_colors"]
        self.assertEqual(sorted(cores), ["advanced", "basic", "creative", "elite", "ultimate"])
        comparados = 0
        for estado in ("off", "on"):
            largura, altura, original = ler_png(os.path.join(TEXTURAS, f"fluid_tank_side_{estado}.png"))
            for tier, (r, g, b) in cores.items():
                w, h, cozida = ler_png(os.path.join(TEXTURAS, f"fluid_tank_side_{estado}_{tier}.png"))
                self.assertEqual((w, h), (largura, altura))
                for i in range(0, len(original), 4):
                    esperado = [(original[i] * r + 127) // 255, (original[i + 1] * g + 127) // 255, (original[i + 2] * b + 127) // 255, original[i + 3]]
                    self.assertEqual(list(cozida[i : i + 4]), esperado, (estado, tier, i // 4))
                    comparados += 1
        self.assertEqual(comparados, 2 * 5 * 256)


if __name__ == "__main__":
    unittest.main()
