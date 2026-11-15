"""O padrão de aviso da aba de redstone vem do `warning.png` do original, ladrilhado e multiplicado pelo blend `DST_COLOR`: o código não embute mais os pixels."""
import json
import os
import re
import unittest

from png_minimo import ler_png

RAIZ = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SPRITES = os.path.join(RAIZ, "mods", "mekanism", "content", "assets", "mekanism", "textures", "gui", "sprites")
FONTE = os.path.join(RAIZ, "mods", "machinery_basis", "core", "src", "redstone_tab.rs")
COLA = os.path.join(RAIZ, "mods", "machinery_basis", "glue.rs")
MODULO = os.path.join(RAIZ, "mods", "machinery_basis", "multiply_blend.rs")


class PadraoDeAviso(unittest.TestCase):
    def test_o_warning_png_e_opaco_e_ladrilha_de_16_por_16(self):
        # Um padrão opaco: o `fs_main` do HUD descarta alfa menor que 0,004, e o original multiplica todo pixel do retângulo.
        largura, altura, pixels = ler_png(os.path.join(SPRITES, "warning.png"))
        self.assertEqual((largura, altura), (16, 16))
        self.assertEqual({p[3] for linha in pixels for p in linha}, {255})
        escala = json.load(open(os.path.join(SPRITES, "warning.png.mcmeta")))["gui"]["scaling"]
        self.assertEqual(escala, {"type": "tile", "width": 16, "height": 16})
        texto = open(FONTE, encoding="utf-8").read()
        self.assertIn("WARNING_PATTERN_TILE: [f32; 2] = [16.0, 16.0]", texto)

    def test_o_suporte_tem_cantos_transparentes_e_o_resto_opaco(self):
        # Os cantos transparentes do suporte são onde o original multiplica o mundo atrás da tela.
        largura, altura, pixels = ler_png(os.path.join(SPRITES, "holder_right.png"))
        self.assertEqual((largura, altura), (26, 9))
        alfas = {p[3] for linha in pixels for p in linha}
        self.assertEqual(alfas, {0, 255})

    def test_o_blend_e_dst_color_zero_e_a_cola_marca_o_retangulo_todo(self):
        modulo = open(MODULO, encoding="utf-8").read()
        self.assertRegex(modulo, r"src_factor: wgpu::BlendFactor::Dst,\s*dst_factor: wgpu::BlendFactor::Zero")
        self.assertIn("blend: Some(wgpu::BlendState { color: multiply, alpha: multiply })", modulo)
        cola = open(COLA, encoding="utf-8").read()
        self.assertRegex(cola, r"tiled_sprite\([^;]*holder, None\);\s*horizon_client::hud::machinery_multiply::mark_since")


if __name__ == "__main__":
    unittest.main()
