#!/usr/bin/env python3
"""A cena do vídeo de verdade da frente `tela-lados` (`H_NAME=horizon`, jogo aberto com `JOGO_PW_SOCK` e a captura `gravar-jogo rec` em paralelo):
um Crusher com um hopper em cima, a janela de lados arrastada para o lado para o slot de entrada aparecer, o lado de cima limpo (o hopper para) e
religado (volta a entrar), o ejetor, a aba de energia e a janela de transporte. Cada passo espera o tempo real que o jogo leva."""
import os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import cena_telas as c

ESCALA = 3
ESQ, TOPO = (-(-1920 // ESCALA) - 176) // 2, (-(-1080 // ESCALA) - 166) // 2


def ponto(x, y):
    return str(ESQ + x), str(TOPO + y)


def clica(x, y, botao=None):
    c.ctl("click-at", *ponto(x, y), *([botao] if botao else []))


def prepara():
    caso = [k for k in c.casos_config() if k["nome"] == "crusher_lados_padrao"][0]
    c.ctl("options-set", "guiScale", str(ESCALA))
    c.monta_port(caso, 3, ESCALA)
    c.comando("tick unfreeze")
    c.comando("setblock 3 -59 3 minecraft:hopper[facing=down]")
    c.ctl("tp", "3.0", "-60.0", "0.0")
    c.ctl("look", "0", "0")


def roteiro():
    c.ctl("interact", "3", "-60", "3")
    time.sleep(2.5)
    c.ctl("look", "0", "0")
    clica(-12, 19)                    # a aba de lados
    time.sleep(2.0)
    c.ctl("drag-at", *ponto(10 + 80, 15 + 8), *ponto(10 + 80 + 160, 15 + 8))  # a janela para a direita
    time.sleep(3.0)
    c.ctl("key", "shift", "down")
    clica(10 + 67 + 11 + 160, 15 + 46 + 11)   # shift: o alto vira NONE
    c.ctl("key", "shift", "up")
    time.sleep(1.5)
    c.comando('data merge block 3 -59 3 {Items:[{Slot:0b,id:"minecraft:cobblestone",count:64}]}')  # o hopper cheio, com o alto desligado
    time.sleep(5.0)
    clica(10 + 67 + 11 + 160, 15 + 46 + 11)   # esquerdo: volta a INPUT
    time.sleep(7.0)
    clica(10 + 136 + 7 + 160, 15 + 6 + 7)     # ejetar
    time.sleep(2.0)
    clica(10 + 160 - 12, 15 + 15 + 28)          # a aba de energia
    time.sleep(2.0)
    clica(10 + 136 + 7 + 160, 15 + 6 + 7)
    time.sleep(1.5)
    clica(-12, 47)                            # a janela de transporte
    time.sleep(3.0)
    c.ctl("close")


if __name__ == "__main__":
    {"preparar": prepara, "rodar": roteiro}[sys.argv[1]]()
