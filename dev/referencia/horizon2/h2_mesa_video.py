#!/usr/bin/env python3
"""O vídeo da Security Desk no port (frente `mesa-seguranca`, 2.ª rodada): o dono abre a mesa, põe dois nomes na lista (o Enter e o botão de confirmar), reabre para os
ver (a tela do original só se atualiza ao reabrir), rola a lista, tira um nome, troca o modo, liga a sobreposição e passa o mouse pelos botões. Um cliente só, com o som da
categoria `ui` ligado (o clique dos botões), gravado pelo `gravar-jogo rec` em paralelo.

Uso: H_NAME=horizon h2_mesa_video.py X  (a mesa em (X, -60, 3), o jogador já diante dela e o jogo no guiScale 3)"""
import json, os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
LEFT, TOP = (640 - 176) // 2, (360 - 230) // 2


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=120).stdout


def click(x, y, *extra):
    ctl("click-at", str(LEFT + x), str(TOP + y), *extra)


def hover(x, y):
    ctl("scroll-at", str(LEFT + x), str(TOP + y), "0")


def open_desk(x):
    for _ in range(4):
        ctl("interact", str(x), "-60", "3")
        time.sleep(0.8)
        if '"screen":"container"' in ctl("screen").replace(" ", ""):
            return
    raise SystemExit("a mesa não abriu")


def type_text(text):
    for ch in text:
        ctl("type", ch)
        time.sleep(0.08)


def main():
    x = int(sys.argv[1])
    time.sleep(1.5)
    open_desk(x)
    hover(5, 5)
    time.sleep(1.0)
    click(60, 73)
    type_text("Steve_99")
    time.sleep(0.5)
    ctl("key", "enter", "tap")
    time.sleep(0.8)
    type_text("Alex")
    time.sleep(0.4)
    click(128, 73)
    time.sleep(1.0)
    ctl("close")
    time.sleep(0.8)
    open_desk(x)
    time.sleep(1.2)
    for _ in range(3):
        ctl("scroll-at", str(LEFT + 60), str(TOP + 30), "-1")
        time.sleep(0.35)
    time.sleep(0.6)
    click(40, 39)
    time.sleep(0.8)
    click(70, 91)
    time.sleep(1.0)
    hover(115, 121)
    time.sleep(1.0)
    click(115, 121)
    time.sleep(0.8)
    hover(154, 67)
    time.sleep(1.0)
    click(154, 67)
    time.sleep(0.8)
    ctl("close")
    time.sleep(0.8)
    open_desk(x)
    hover(154, 67)
    time.sleep(1.5)
    hover(33, 121)
    time.sleep(1.5)
    hover(5, 5)
    time.sleep(1.0)
    ctl("close")
    print(json.dumps({"ok": True}))


if __name__ == "__main__":
    main()
