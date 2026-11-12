#!/usr/bin/env python3
"""A forma e o gabarito do cubo de energia em jogo (`H_NAME=horizon`, mundo plano): o que o gabarito `beacon` (ou a classe posta pela injeção `open_menu_class`) traz de efeito, e o protótipo B2.

    h2_cubo_forma.py efeitos       menu, feixe (o cubo sobre o beacon e a pirâmide), efeitos de poção, entidade de bloco, reação ao pistão, instrumento do bloco de nota, luz
    h2_cubo_forma.py b2 DIR        o protótipo B2 (`cubob:cube`, uma propriedade de estado por lado): custo de remalha ao trocar estados e prints contra o cubo A
    h2_cubo_forma.py selecao DIR   o contorno de seleção do cubo (print e cena do vídeo)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim.
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_armor as a

BASE = (90, -60, 90)
SIDES = ["front", "left", "right", "back", "top", "bottom"]
failures = []


def at(dx=0, dy=0, dz=0):
    return "%d %d %d" % (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def cmd(text):
    return " | ".join(a.command(text))


def sky():
    a.ctl("close")
    a.command("gamemode creative")
    a.command("weather clear")
    a.command("time set noon")
    a.command("gamerule advance_time false")
    a.command("gamerule advance_weather false")


def restore():
    a.ctl("close")
    a.command("gamerule advance_time true")
    a.command("gamerule advance_weather true")


def step(n):
    a.command("tick step %d" % n)
    a.settle(0.4 + 0.02 * n)


def clear_area():
    a.command("fill %s %s minecraft:air" % (at(-6, -1, -6), at(6, 6, 6)))
    a.settle(0.6)


def report(label, value):
    print("%-34s %s" % (label, value))


def efeitos():
    sky()
    a.command("tp @s %s 180 15" % at(0.5, 0, 2.5))
    a.settle(1)
    for tier in ("basic", "creative"):
        clear_area()
        cube = "mekanism:%s_energy_cube" % tier
        # 1. o menu abre
        a.command("setblock %s %s" % (at(0, 0, 0), cube))
        a.settle(0.8)
        a.ctl("select", "6")
        opened = False
        for _ in range(4):
            a.ctl("interact", *at(0, 0, 0).split())
            a.settle(0.7)
            if '"screen":"container"' in a.ctl("screen").replace(" ", ""):
                opened = True
                break
        report("%s menu abre" % tier, opened)
        a.ctl("close")
        # 2. a entidade de bloco é a do mod, não a do beacon
        report("%s id da entidade" % tier, cmd("data get block %s id" % at(0, 0, 0)))
        # 3. luz: o beacon emite 15; o cubo do original, 0
        step(5)
        report("%s luz de bloco" % tier, a.ctl("light", *(at(0, 0, 0).split() * 2)).strip()[:160])
        # 4. o feixe: o cubo sobre o beacon real e a pirâmide de ferro de um nível
        a.command("fill %s %s minecraft:iron_block" % (at(-1, -1, 3), at(1, -1, 5)))
        a.command("setblock %s minecraft:beacon" % at(0, 0, 4))
        step(100)
        base_levels = cmd("data get block %s Levels" % at(0, 0, 4))
        a.command("setblock %s %s" % (at(0, 1, 4), cube))
        step(100)
        report("%s Levels do beacon (livre)" % tier, base_levels)
        report("%s Levels com o cubo acima" % tier, cmd("data get block %s Levels" % at(0, 0, 4)))
        report("%s id do bloco acima do beacon" % tier, cmd("data get block %s id" % at(0, 1, 4)))
        # 5. efeitos de poção
        report("%s efeitos no jogador" % tier, cmd("data get entity @s active_effects"))
        # 6. pistão: o cubo é IMMOVEABLE (BlockMekanism.getPistonPushReaction)
        clear_area()
        a.command("setblock %s %s" % (at(1, 0, 0), cube))
        a.command("setblock %s minecraft:piston[facing=east]" % at(0, 0, 0))
        a.command("setblock %s minecraft:redstone_block" % at(-1, 0, 0))
        step(20)
        report("%s cubo depois do pistão" % tier, a.ctl("block", *at(1, 0, 0).split()).strip()[:120])
        report("%s pistão" % tier, a.ctl("block", *at(0, 0, 0).split()).strip()[:120])
        # 7. bloco de nota em cima: o instrumento vem do bloco de baixo
        clear_area()
        a.command("setblock %s %s" % (at(0, 0, 0), cube))
        a.command("setblock %s minecraft:note_block" % at(0, 1, 0))
        step(5)
        report("%s bloco de nota em cima" % tier, a.ctl("block", *at(0, 1, 0).split()).strip()[:160])
        a.command("setblock %s minecraft:stone" % at(0, 0, 0))
        step(5)
        report("(controle) pedra embaixo", a.ctl("block", *at(0, 1, 0).split()).strip()[:160])
        clear_area()
    restore()


def props(ports, lits, facing="north"):
    parts = ["facing=" + facing]
    parts += ["port_%s=%s" % (n, "true" if n in ports else "false") for n in SIDES]
    parts += ["lit_%s=%s" % (n, "true" if n in lits else "false") for n in SIDES]
    return "[" + ",".join(parts) + "]"


def frame_stats():
    return a.ctl("stats").strip()


def b2(shots):
    """O custo de remalha do B2: 400 cubos `cubob:cube` mudando de estado dez vezes, contra o mesmo tempo parado; e prints do B2 e do A na mesma pose."""
    sky()
    a.command("tp @s %s 180 25" % at(0, 2, 14))
    a.settle(1.5)
    clear_area()
    a.command("fill %s %s minecraft:air" % (at(-12, 0, -12), at(12, 4, 12)))
    a.settle(0.6)
    report("estados do bloco", cmd("setblock %s cubob:cube%s" % (at(0, 0, 0), props([], []))))
    a.settle(0.6)
    area = (at(-10, 0, -10), at(9, 0, 9))
    a.command("fill %s %s cubob:cube%s" % (*area, props([], [])))
    a.settle(3)
    a.ctl("stats", "reset")
    a.settle(5)
    report("parado (20x20 cubos, 5 s)", frame_stats())
    a.ctl("stats", "reset")
    t0 = time.time()
    for i in range(10):
        on = SIDES if i % 2 == 0 else []
        a.command("fill %s %s cubob:cube%s" % (*area, props(on, on)))
        a.settle(0.4)
    report("10 trocas dos 400 cubos (%.1f s)" % (time.time() - t0), frame_stats())
    # prints: o B2 com a frente e a esquerda ativas (porta e LED), contra o A de config padrão
    a.command("fill %s %s minecraft:air" % (at(-12, 0, -12), at(12, 4, 12)))
    a.settle(0.6)
    a.command("tp @s %s 160 20" % at(0.5, 1, 3))
    a.command("setblock %s cubob:cube%s" % (at(-1, 0, 0), props(SIDES, ["front", "left", "top"])))
    a.command("setblock %s cubob:cube%s" % (at(1, 0, 0), props(["front", "left", "right"], [])))
    a.command("setblock %s mekanism:basic_energy_cube" % at(3, 0, 0))
    a.settle(2)
    a.ctl("screenshot", shots + "/b2_e_a.png")
    a.command("fill %s %s minecraft:air" % (at(-12, 0, -12), at(12, 4, 12)))
    restore()


def selecao():
    """A cena do vídeo: o contorno de seleção do cubo segue os lados habilitados. A tela de lados liga o de cima, o da esquerda e o de baixo, e o contorno ganha uma placa a cada vez
    (rode o `gravar-jogo rec` em paralelo, com uns 28 s)."""
    import h2_cubo_tela as ct
    ct.prepare(1500000)
    # guiScale 3 numa janela de 1920 por 1080: a tela de 176 por 166 está em (232, 97) em pixels de GUI.
    ct.tl.SIDE_TAB = (232 - 12, 97 + 19)
    ct.tl.WIN = (402, 112)  # a janela de lados do cubo abre à direita da tela
    a.command("tp @s 70.5 -60 59.5 180 25")  # o olhar na barra de cima do quadro (o centro do cubo é vazado)
    a.settle(4)
    for side in ("top", "left", "bottom"):
        ct.ab.open_screen()
        a.settle(1.2)
        ct.tl.click(ct.tl.SIDE_TAB)
        a.settle(1.2)
        ct.tl.click(ct.tl.at(ct.tl.WIN, ct.tl.SIDE[side]))
        a.settle(1.2)
        report("config depois de %s" % side, ct.config())
        a.ctl("close")
        a.settle(3.5)
    ct.finish()
    restore()


def main():
    step_name = sys.argv[1] if len(sys.argv) > 1 else "efeitos"
    shots = sys.argv[2] if len(sys.argv) > 2 else "/home/luka/.local/share/jogo-isolado/horizon/data/shots/cubo_forma"
    {"efeitos": efeitos, "selecao": selecao, "b2": lambda: b2(shots)}[step_name]()
    print("FALHAS" if failures else "OK", failures)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
