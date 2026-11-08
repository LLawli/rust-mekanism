#!/usr/bin/env python3
"""Prints do som do port, pela legenda do jogo (`showSubtitles:true` no options.txt): a máquina acesa com o som tocando mostra a linha da legenda, e os casos sem som
(silêncio no máximo, `enableMachine` desligado, máquina parada) não. O jogo é o `horizon` (H_NAME=horizon), e os prints ficam em SAIDA no desktop (um caminho que o
container e o desktop enxergam igual, como `.../horizon/data/shots/som`).

Uso: H_NAME=horizon h2_som_prints.py SAIDA
"""
import os, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_som_medida as m

h = m.h
OUT = sys.argv[1]


def shot(name):
    h.ctl("look-at-block", *map(str, m.POS))
    time.sleep(0.4)
    h.ctl("screenshot", f"{OUT}/{name}.png")


def scene(base=1.0, enable=True, muffling=0):
    m.write_config(base, enable)
    time.sleep(2.5)
    m.place("chamber")
    for _ in range(muffling):
        m.add_muffling()
    time.sleep(1.5)
    h.ctl("close")
    m.stand(m.feet({"jogador": (3.0, -0.5, 0.0)}))
    time.sleep(1.0)


def main():
    m.ssh(f"mkdir -p {OUT}")
    m.silenciar()
    h.command("gamemode creative")
    h.command("tick unfreeze")
    # 1. Tocando, e depois de parar (as legendas levam alguns segundos para sumir).
    scene()
    shot("port_antes_de_ligar")
    m.power("chamber", True)
    time.sleep(3.0)
    shot("port_tocando")
    m.power("chamber", False)
    time.sleep(8.0)
    shot("port_parada")
    # 2. Aceso e calado: o upgrade de silêncio no máximo.
    scene(muffling=1)
    m.power("chamber", True)
    time.sleep(3.0)
    shot("port_silencio_no_maximo_aceso")
    m.power("chamber", False)
    time.sleep(2.0)
    # 3. Aceso com `enableMachine` desligado.
    scene(enable=False)
    m.power("chamber", True)
    time.sleep(3.0)
    shot("port_maquinas_desligadas_aceso")
    m.power("chamber", False)
    time.sleep(2.0)
    m.write_config(1.0, True)
    h.command(f"setblock {' '.join(map(str, m.POS))} minecraft:air")


if __name__ == "__main__":
    main()
