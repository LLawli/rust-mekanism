#!/usr/bin/env python3
"""A cena do vídeo da janela do livro de receitas (frente `janela-livro`, 4.ª rodada), no port, pelo `horizon-ctl` de `cena_telas.py`.

    H_NAME=horizon h2_janela_livro_video.py preparar   monta a Enrichment Chamber com todas as receitas (o caso `chamber_livro_receitas`)
    H_NAME=horizon h2_janela_livro_video.py rodar      abre a tela e o livro, troca de aba, busca, liga e desliga o filtro e vira a página
    H_NAME=horizon h2_janela_livro_video.py preparar6  monta as quatro máquinas (6.ª rodada)
    H_NAME=horizon h2_janela_livro_video.py rodar6     a colocação pelo livro, o fantasma, o Enter, o shift e o overlay, nas quatro máquinas

`rodar` dura uns 30 s e roda em paralelo com `gravar-jogo rec`. A janela fica onde a posição guardada a deixa (veja `LIVRO_BUSCA` em `cena_telas.py`).
"""
import os
import sys
import time

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import cena_telas as c  # noqa: E402

ESCALA = 2
X = 3


def caso():
    return next(k for k in c.casos_do_roteiro(["chamber_livro_receitas"]) if k["nome"] == "chamber_livro_receitas")


def preparar():
    c.comando("clear @s")
    c.comando("difficulty peaceful")
    c.comando("time set noon")
    c.comando("weather clear")
    c.comando("gamerule advance_time false")
    c.comando("gamerule advance_weather false")
    c.ctl("options-set", "chatVisibility", "2")
    c.ctl("options-set", "guiScale", str(ESCALA))
    c.monta_port(caso(), X, ESCALA)


def clique(x, y, espera=1.2):
    gui_l, gui_a = -(-1920 // ESCALA), -(-1080 // ESCALA)
    c.ctl("click-at", str((gui_l - 176) // 2 + x), str((gui_a - 166) // 2 + y))
    time.sleep(espera)


def rodar():
    c.ctl("look", "0", "0")
    c.ctl("interact", str(X), "-60", "3")
    time.sleep(2)
    c.ctl("look", "0", "0")
    time.sleep(1)
    clique(*c.LIVRO_BOTAO, espera=2.5)
    # As abas, de cima para baixo (a bússola é a primeira, em (-30, 3) do livro): a de construção, a de eficiência e a de volta à busca.
    for aba in (1, 2, 0):
        clique(-15, 88 + 10 + 3 + 27 * aba + 13, espera=1.8)
    clique(*c.LIVRO_SETA, espera=1.5)
    clique(*c.LIVRO_SETA, espera=1.5)
    clique(*c.LIVRO_BUSCA, espera=0.8)
    for letra in "iron":
        c.ctl("type", letra)
        time.sleep(0.5)
    time.sleep(2)
    c.ctl("key", "backspace", "tap")
    time.sleep(0.6)
    c.ctl("key", "backspace", "tap")
    time.sleep(2)
    clique(*c.LIVRO_FILTRO, espera=2.5)
    clique(*c.LIVRO_FILTRO, espera=2.5)
    c.ctl("close")
    time.sleep(1)


# A 6.ª rodada: a colocação pelo livro nas quatro máquinas, o fantasma, o Enter, o shift e o overlay das variantes.
MAQUINAS6 = (("chamber", 3), ("crusher", 20), ("infuser", 37), ("smelter", 54))


def preparar6():
    """As quatro máquinas, cada uma na caixa dela, com todas as receitas no livro e o inventário vazio."""
    c.comando("clear @s")
    c.comando("difficulty peaceful")
    c.comando("time set noon")
    c.comando("weather clear")
    c.comando("gamerule advance_time false")
    c.comando("gamerule advance_weather false")
    c.ctl("options-set", "chatVisibility", "2")
    c.ctl("options-set", "guiScale", str(ESCALA))
    for maquina, x in MAQUINAS6:
        c.monta_port(c.caso(f"{maquina}_livro_video", maquina, energia="0", receitas="1"), x, ESCALA)
    c.comando("clear @s")


def abre(x):
    c.ctl("tp", f"{x}.0", "-60.0", "0.0")
    time.sleep(0.6)
    c.ctl("look", "0", "0")
    c.ctl("interact", str(x), "-60", "3")
    time.sleep(1.2)
    c.ctl("look", "0", "0")


def busca(texto):
    clique(*c.LIVRO_BUSCA, espera=0.3)
    c.ctl("type", texto)
    time.sleep(0.8)


def rodar6():
    """Uns 60 s, em paralelo com `gravar-jogo rec`: a Enrichment Chamber (o fantasma, o Enter, o shift e o overlay), o Crusher, o
    Metallurgic Infuser (o fantasma na barra de químico e a colocação com o tanque cheio) e o livro do Energized Smelter."""
    abre(3)
    clique(*c.LIVRO_BOTAO, espera=1.2)
    busca("charcoal dust")
    # Sem serragem: o fantasma.
    clique(*c.LIVRO_GRADE, espera=2.0)
    # A serragem chega, um clique no fundo tira o foco da busca e o Enter coloca 8; o shift coloca tudo.
    c.da_item("9:mekanism:sawdust:20")
    clique(*c.LIVRO_FUNDO, espera=0.5)
    c.ctl("key", "enter", "tap")
    time.sleep(1.8)
    c.ctl("key", "shift", "down")
    clique(*c.LIVRO_GRADE, espera=0.2)
    c.ctl("key", "shift", "up")
    time.sleep(1.6)
    # O overlay das variantes do pó de ferro, com 5 de ferro bruto: a fazível entra, outra vira fantasma.
    c.da_item("10:raw_iron:5")
    clique(*c.LIVRO_BUSCA, espera=0.2)
    for _ in "charcoal dust":
        c.ctl("key", "backspace", "tap")
    c.ctl("type", "iron dust")
    time.sleep(0.8)
    gui_l, gui_a = -(-1920 // ESCALA), -(-1080 // ESCALA)
    c.ctl("click-at", str((gui_l - 176) // 2 + c.LIVRO_GRADE[0]), str((gui_a - 166) // 2 + c.LIVRO_GRADE[1]), "right")
    time.sleep(1.5)
    clique(*c.LIVRO_OVERLAY_1, espera=1.8)
    clique(*c.LIVRO_OVERLAY_0, espera=1.8)
    c.ctl("close")
    time.sleep(0.5)
    # O Crusher: pedregulho em cascalho.
    c.comando("clear @s")
    c.da_item("9:cobblestone:10")
    abre(20)
    clique(*c.LIVRO_BOTAO, espera=1.0)
    busca("gravel")
    clique(*c.LIVRO_GRADE, espera=1.8)
    c.ctl("close")
    time.sleep(0.5)
    # O Infuser: sem químico no tanque, o fantasma no slot, na barra e na saída; com o tanque cheio, o cobre entra.
    c.comando("clear @s")
    c.da_item("9:copper_ingot:5")
    abre(37)
    clique(*c.LIVRO_BOTAO, espera=1.0)
    busca("infused alloy")
    clique(*c.LIVRO_GRADE, espera=2.0)
    c.comando('data merge block 37 -60 3 {chemical_tanks:[{container:0b,stored:{type:"mekanism:redstone",amount:100L}}]}')
    time.sleep(1.0)
    clique(*c.LIVRO_FUNDO, espera=0.4)
    c.ctl("key", "enter", "tap")
    time.sleep(1.8)
    c.ctl("close")
    time.sleep(0.5)
    # O Energized Smelter: o livro dele (as receitas do forno).
    abre(54)
    clique(*c.LIVRO_BOTAO, espera=2.0)
    c.ctl("close")
    time.sleep(0.8)


if __name__ == "__main__":
    {"preparar": preparar, "rodar": rodar, "preparar6": preparar6, "rodar6": rodar6}[sys.argv[1]]()
