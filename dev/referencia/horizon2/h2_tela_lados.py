#!/usr/bin/env python3
"""Em jogo (`H_NAME=horizon2`), as abas de configuração de lados e de transporte das quatro máquinas e as janelas que elas abrem
(`GuiSideConfigurationTab`, `GuiTransporterConfigTab`, `GuiSideConfiguration`, `GuiTransporterConfig`).

Etapas (uma por argumento; sem argumentos, todas):
  abas      a tela de cada máquina com as duas abas à esquerda, e o clique nelas com terra no cursor (no original não solta)
  lados     a janela de lados aberta em cada máquina (print), um clique esquerdo, um direito e um com shift num botão de lado, o ejetor, o
            botão de limpar, a aba do tipo: o `component_config` lido de volta depois de cada clique
  transp    a janela de transporte aberta (print), a cor de saída e a de entrada, o `Strict Input`
  video     o vídeo curto: mudar um lado e o ejetor e ver a máquina obedecer

O canto da tela de 176 por 166 está em (232, 97) com guiScale 3 numa janela de 1920 por 1080; a janela de lados abre em (10, 15) dentro dela.
Prints em SHOTS (padrão: a pasta `shots/tela_lados` do container).
"""
import json, os, re, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_abas as ab
import h2_armor as a

SHOTS = os.environ.get("SHOTS", "/home/luka/.local/share/jogo-isolado/horizon/data/shots/tela_lados")
LEFT, TOP = a.LEFT, a.TOP
# O mundo `spike` é plano: o chão fica em y = -61.
ab.SPOT = SPOT = (70, -60, 57)
ab.STAND = "70.5 -60 59.5 180 20"
SIDE_TAB = (LEFT - 12, TOP + 19)
TRANSPORTER_TAB = (LEFT - 12, TOP + 47)
WIN = (LEFT + 10, TOP + 15)
# Os botões de lado da janela de lados (22 por 22), pelo centro.
SIDE = {"bottom": (67, 92), "top": (67, 46), "front": (67, 69), "back": (44, 92), "left": (44, 69), "right": (90, 69)}
TRANSPORTER = {"bottom": (41, 80), "top": (41, 34), "front": (41, 57), "back": (18, 80), "left": (18, 57), "right": (64, 57)}


def shot(name):
    a.ctl("screenshot", f"{SHOTS}/{name}.png")


def at(win, point, size=22):
    return (win[0] + point[0] + size // 2, win[1] + point[1] + size // 2)


def click(point, button=None):
    a.ctl("click-at", str(point[0]), str(point[1]), *([button] if button else []))
    a.settle(0.6)


def config_nbt():
    """O `component_config` e o `component_ejector` do tile, como o jogo os guarda."""
    out = " ".join(a.command(f"data get block {SPOT[0]} {SPOT[1]} {SPOT[2]}"))
    config = re.search(r"component_config: \{[^}]*\}", out)
    ejector = re.search(r"component_ejector: \{[^}]*\}", out)
    return (config.group(0) if config else None, ejector.group(0) if ejector else None)


def ints(nbt, ordinal):
    """O `config<n>` (6 inteiros, na ordem frente, esquerda, direita, costas, alto, base)."""
    found = re.search(rf"config{ordinal}: \[I; ([^\]]*)\]", nbt or "")
    return [int(v) for v in found.group(1).split(",")] if found else None


def open_machine(machine):
    ab.place(machine, False)
    ab.open_screen()


def case_tabs(failures):
    for machine in ab.MACHINES:
        open_machine(machine)
        shot(f"abas_{machine}")
        ab.hold_dirt()
        for name, point in (("aba de lados", SIDE_TAB), ("aba de transporte", TRANSPORTER_TAB)):
            before = a.carried()
            click(point)
            # O clique abre a janela; o item continua no cursor (o original não o solta).
            a.expect(f"{machine}: clique na {name}", a.carried(), before, failures)
            shot(f"abas_{machine}_{name.split()[-1]}")
            click((WIN[0] + 140, WIN[1] + 10))
        a.ctl("close")


def case_sides(failures):
    for machine in ab.MACHINES:
        open_machine(machine)
        click(SIDE_TAB)
        shot(f"lados_{machine}_aberta")
        before = ints(config_nbt()[0], 3)
        front = at(WIN, SIDE["front"])
        click(front)
        left = ints(config_nbt()[0], 3)
        click(front, "right")
        back = ints(config_nbt()[0], 3)
        a.ctl("key", "shift", "down")
        click(front)
        a.ctl("key", "shift", "up")
        shifted = ints(config_nbt()[0], 3)
        print(f"{machine}: frente {before} -> esquerdo {left} -> direito {back} -> shift {shifted}")
        a.expect(f"{machine}: o clique esquerdo muda o lado", left != before, True, failures)
        a.expect(f"{machine}: o direito volta", back, before, failures)
        a.expect(f"{machine}: shift limpa o lado", shifted[0], 0, failures)
        shot(f"lados_{machine}_depois")
        click(at(WIN, (136, 6), 14))
        print(f"{machine}: ejetar -> {config_nbt()[0]}")
        shot(f"lados_{machine}_ejetar")
        click(at(WIN, (136, 95), 14))
        cleared = ints(config_nbt()[0], 3)
        print(f"{machine}: limpar -> {cleared}")
        a.ctl("close")


def case_transporter(failures):
    for machine in ab.MACHINES:
        open_machine(machine)
        click(TRANSPORTER_TAB)
        shot(f"transp_{machine}_aberta")
        click(at(WIN, (111, 48), 18))
        click(at(WIN, TRANSPORTER["front"]))
        click(at(WIN, TRANSPORTER["front"]), "right")
        click(at(WIN, TRANSPORTER["front"]))
        click(at(WIN, (136, 6), 14))
        print(f"{machine}: {config_nbt()[1]}")
        shot(f"transp_{machine}_depois")
        a.ctl("close")


def input_count():
    """Quantas pedras o slot de entrada do Crusher tem agora (`Items`)."""
    out = " ".join(a.command(f"data get block {SPOT[0]} {SPOT[1]} {SPOT[2]} Items"))
    found = re.search(r"Slot: 0b[^}]*count: (\d+)|count: (\d+)[^}]*Slot: 0b", out)
    return int(next(g for g in found.groups() if g)) if found else 0


def case_video(failures):
    """Os quadros do vídeo: um hopper em cima do Crusher, a janela de lados aberta, o lado de cima limpo (o hopper para) e religado (volta a entrar).
    Cada quadro tem a legenda em `video_legendas.txt` (`nome|texto`), e o `montar_video.sh` os junta."""
    frames = []
    a.ctl("close")
    a.command("gamemode creative")
    a.command(f"setblock {SPOT[0]} {SPOT[1] + 1} {SPOT[2]} minecraft:air")
    ab.place("crusher", False)
    top = (SPOT[0], SPOT[1] + 1, SPOT[2])
    a.command(f"setblock {top[0]} {top[1]} {top[2]} minecraft:hopper[facing=down]")
    a.settle(1)
    a.command(f'data merge block {top[0]} {top[1]} {top[2]} {{Items:[{{Slot:0b,id:"minecraft:cobblestone",count:30}}]}}')
    ab.open_screen()
    frames.append(("1", "Crusher aberto: a aba de lados e a de transporte ficam a esquerda"))
    shot("video_1")
    click(SIDE_TAB)
    a.settle(2.5)
    shot("video_2")
    frames.append(("2", f"A janela de lados: o alto esta em INPUT e o hopper enche a entrada (pedras: {input_count()})"))
    a.ctl("key", "shift", "down")
    click(at(WIN, SIDE["top"]))
    a.ctl("key", "shift", "up")
    shot("video_3")
    frames.append(("3", f"Shift-clique no alto: NONE. O hopper nao entrega mais (pedras: {input_count()})"))
    a.settle(4)
    shot("video_4")
    frames.append(("4", f"4 s depois o hopper segue parado: nada entra pelo lado desligado (pedras: {input_count()})"))
    click(at(WIN, SIDE["top"]))
    a.settle(4)
    shot("video_5")
    frames.append(("5", f"Clique esquerdo no alto: volta a INPUT e o hopper enche de novo (pedras: {input_count()})"))
    click(at(WIN, (136, 6), 14))
    a.settle(1)
    shot("video_6")
    frames.append(("6", "O botao de ejetar liga o Eject: on do tipo Items"))
    open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "video_legendas.txt"), "w").write("\n".join("|".join(f) for f in frames) + "\n")
    a.ctl("close")


def main():
    steps = sys.argv[1:] or ["abas", "lados", "transp"]
    os.makedirs(SHOTS, exist_ok=True)
    failures = []
    a.prepare = a.prepare
    for step in steps:
        {"abas": case_tabs, "lados": case_sides, "transp": case_transporter, "video": case_video}[step](failures)
    print("FALHAS" if failures else "OK", failures)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
