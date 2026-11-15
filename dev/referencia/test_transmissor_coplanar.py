"""As faces coplanares do modelo dos transmissores são as do original (frente `transmissor-sobras`, decisão 239).

O braço `PUSH` e o `PULL` do OBJ do original (`transmitter_small.obj`, `transmitter_large.obj`) trazem, em cada lado, os quatro retângulos do anel da ponta sobrepostos nas quinas, dois anéis por braço
(no pequeno, 48 pares de faces coplanares que se sobrepõem em cada conjunto de seis lados; no grande, 24 no `PUSH` e 48 no `PULL`). Todas têm o mesmo material, e o `NORMAL` não tem nenhuma. O port usa o mesmo OBJ byte a byte, então a sobreposição
é do original: o topo do braço `PUSH` e `PULL` que parecia cintilar no port não vem de uma face a mais no modelo do port. O teste confere o OBJ do port contra a contagem acima e, se o checkout do
original existir, contra o arquivo dele.
"""
import itertools
import os
import unittest

AQUI = os.path.dirname(os.path.abspath(__file__))
MODELOS = os.path.abspath(os.path.join(AQUI, "..", "..", "mods", "mekanism", "content", "assets", "mekanism", "models", "block"))
ORIGINAL = "/mnt/dados/caches/rust-mekanism-ref/mekanism/src/main/resources/assets/mekanism/models/block"
LADOS = ["down", "up", "north", "south", "west", "east"]


def partes(caminho):
    vertices, grupos, atual, material = [], {}, None, None
    for linha in open(caminho):
        t = linha.split()
        if not t:
            continue
        if t[0] == "v":
            vertices.append(tuple(float(x) for x in t[1:4]))
        elif t[0] == "usemtl":
            material = t[1]
        elif t[0] in ("g", "o"):
            atual = t[1]
            grupos.setdefault(atual, [])
        elif t[0] == "f":
            grupos[atual].append(([vertices[int(x.split("/")[0]) - 1] for x in t[1:]], material))
    return grupos


def plano(quad):
    for eixo in range(3):
        valores = {round(p[eixo], 5) for p in quad}
        if len(valores) == 1:
            return eixo, valores.pop()
    return None


def retangulo(quad, eixo):
    outros = [i for i in range(3) if i != eixo]
    a = [p[outros[0]] for p in quad]
    b = [p[outros[1]] for p in quad]
    return min(a), max(a), min(b), max(b)


def sobrepoe(r, s):
    return min(r[1], s[1]) - max(r[0], s[0]) > 1e-6 and min(r[3], s[3]) - max(r[2], s[2]) > 1e-6


def sobreposicoes(grupos, tipo):
    faces = [(nome, quad, material) for nome in (lado + tipo for lado in LADOS) for quad, material in grupos[nome]]
    pares, materiais_diferentes = 0, 0
    for (_, q1, m1), (_, q2, m2) in itertools.combinations(faces, 2):
        p1, p2 = plano(q1), plano(q2)
        if p1 and p2 and p1 == p2 and sobrepoe(retangulo(q1, p1[0]), retangulo(q2, p2[0])):
            pares += 1
            materiais_diferentes += m1 != m2
    return pares, materiais_diferentes


class FacesCoplanares(unittest.TestCase):
    def confere(self, pasta):
        for nome, (push, pull) in (("transmitter_small.obj.mek", (48, 48)), ("transmitter_large.obj.mek", (24, 48))):
            grupos = partes(os.path.join(pasta, nome))
            self.assertEqual(sobreposicoes(grupos, "PUSH"), (push, 0), nome)
            self.assertEqual(sobreposicoes(grupos, "PULL"), (pull, 0), nome)
            self.assertEqual(sobreposicoes(grupos, "NORMAL"), (0, 0), nome)

    def test_the_arms_overlap_the_way_the_original_does(self):
        self.confere(MODELOS)

    @unittest.skipUnless(os.path.isdir(ORIGINAL), "sem o checkout do original")
    def test_the_obj_of_the_port_is_the_one_of_the_original(self):
        for nome in ("transmitter_small.obj.mek", "transmitter_large.obj.mek"):
            self.assertEqual(open(os.path.join(MODELOS, nome), "rb").read(), open(os.path.join(ORIGINAL, nome), "rb").read(), nome)
        self.confere(ORIGINAL)


if __name__ == "__main__":
    unittest.main()
