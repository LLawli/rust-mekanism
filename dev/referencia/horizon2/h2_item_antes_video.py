#!/usr/bin/env python3
"""O vídeo da frente `item-antes-do-bloco`: o convidado de LAN usa o cartão (copia agachado, cola de pé, copia no cubo e o cartão segue com os dados) e o Configurator em
`ROTATE` (gira sem abrir a tela) e em `ITEMS` (a tela abre). A gravação é de fora (`gravar-jogo`/`gst-launch` no nó do gamescope do convidado). Uso: H_NAME=horizon2 h2_item_antes_video.py"""
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_item_antes as t

R = "convidado"
t.setup()
# Para a câmera do convidado: o anfitrião sai da frente, o chat do convidado fica oculto (ele não manda comando) e o aviso do tutorial some.
t.d.guest("options-set", "chatVisibility", "2")
t.d.guest("options-set", "tutorialStep", "none")
t.hcmd("tp @s 92.5 -59 70.5 0 20")
time.sleep(2.0)
t.give(R, t.CARD)
time.sleep(1.0)
t.click(R, t.CHAMBER_A, "north", shift=True)
time.sleep(2.2)
t.click(R, t.CHAMBER_B, "north", shift=False)
time.sleep(2.2)
t.click(R, t.CUBE, "north", shift=True)
time.sleep(2.5)
t.give(R, f'{t.CONFIGURATOR}[{t.MODE}="rotate"]')
time.sleep(1.0)
for _ in range(3):
    t.click(R, t.CHAMBER_A, "north", shift=False)
    time.sleep(1.2)
t.click(R, t.CUBE, "north", shift=False)
time.sleep(1.5)
t.give(R, f'{t.CONFIGURATOR}[{t.MODE}="configurate_items"]')
time.sleep(1.0)
t.click(R, t.CHAMBER_A, "north", shift=False)
time.sleep(2.0)
t.who_ctl(R)("close")
time.sleep(1.0)
