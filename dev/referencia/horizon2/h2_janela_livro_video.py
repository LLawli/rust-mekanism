#!/usr/bin/env python3
"""A cena do vídeo da janela do livro de receitas (frente `janela-livro`, 4.ª rodada), no port, pelo `horizon-ctl` de `cena_telas.py`.

    H_NAME=horizon h2_janela_livro_video.py preparar   monta a Enrichment Chamber com todas as receitas (o caso `chamber_livro_receitas`)
    H_NAME=horizon h2_janela_livro_video.py rodar      abre a tela e o livro, troca de aba, busca, liga e desliga o filtro e vira a página

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


if __name__ == "__main__":
    {"preparar": preparar, "rodar": rodar}[sys.argv[1]]()
