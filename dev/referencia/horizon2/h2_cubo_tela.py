#!/usr/bin/env python3
"""A tela do cubo de energia em jogo (`H_NAME=horizon2`, mundo plano `spike`, guiScale 4 numa janela de 1920 por 1080: o canto da tela de 176 por 166 está em (152, 52) em pixels de GUI).

    h2_cubo_tela.py telas DIR      abre o Basic Energy Cube vazio e com 1.500.000 FE, a aba de lados, o lado de cima em saída (lido de volta no `component_config`) e a aba de transporte; prints em DIR
    h2_cubo_tela.py slots          o slot de descarga e o de carga recusam terra (a regra de `EnergyInventorySlot`) e o de descarga aceita redstone
    h2_cubo_tela.py video          a cena do vídeo (rode o `gravar-jogo rec` em paralelo)

Clima limpo e meio-dia com os ciclos parados; as gamerules voltam ao fim.
"""
import json, os, re, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import h2_abas as ab
import h2_armor as a
import h2_tela_lados as tl

SPOT = (70, -60, 57)
LEFT, TOP = 152, 52
tl.SPOT = ab.SPOT = SPOT
tl.SIDE_TAB = (LEFT - 12, TOP + 19)
tl.TRANSPORTER_TAB = (LEFT - 12, TOP + 47)
tl.WIN = (LEFT + 10, TOP + 15)
failures = []


def expect(label, got, want):
    ok = got == want
    print(("OK   " if ok else "DIFF ") + label + ("" if ok else f"  esperado {want}  obtido {got}"))
    if not ok:
        failures.append(label)


def config():
    out = " ".join(a.command("data get block %d %d %d" % SPOT))
    found = re.search(r"component_config: \{[^}]*\}", out)
    return found.group(0) if found else ""


def prepare(energy=None):
    a.ctl("close")
    a.command("gamemode creative")
    a.command("tp @s 70.5 -60 59.5 180 20")
    a.settle(0.8)
    a.command("weather clear")
    a.command("time set noon")
    a.command("gamerule advance_time false")
    a.command("gamerule advance_weather false")
    a.command("setblock %d %d %d minecraft:air" % SPOT)
    a.settle(0.4)
    a.command("setblock %d %d %d mekanism:basic_energy_cube" % SPOT)
    a.settle(0.8)
    if energy is not None:
        a.command("data modify block %d %d %d energy set value %d" % (*SPOT, energy))
        a.settle(0.5)


def finish():
    a.ctl("close")
    a.command("setblock %d %d %d minecraft:air" % SPOT)
    a.command("gamerule advance_time true")
    a.command("gamerule advance_weather true")


def telas(shots):
    prepare()
    ab.open_screen()
    a.settle(0.8)
    a.ctl("screenshot", f"{shots}/vazia.png")
    a.ctl("close")
    a.command("data modify block %d %d %d energy set value 1500000" % SPOT)
    a.settle(0.5)
    ab.open_screen()
    a.settle(0.8)
    a.ctl("screenshot", f"{shots}/energia_parcial.png")
    before = config()
    tl.click(tl.SIDE_TAB)
    a.ctl("screenshot", f"{shots}/lados.png")
    tl.click(tl.at(tl.WIN, tl.SIDE["top"]))
    # `RelativeSide` na ordem frente, esquerda, direita, costas, alto, base; `DataType` 1 INPUT e 4 OUTPUT.
    expect("o lado de cima passa de entrada para saída", (before, config()), (before, before.replace("1, 1, 1, 1, 1]", "1, 1, 1, 4, 1]", 1)))
    a.ctl("screenshot", f"{shots}/lado_cima_saida.png")
    tl.click(tl.at(tl.WIN, (136, 6), 14))
    tl.click(tl.TRANSPORTER_TAB)
    a.ctl("screenshot", f"{shots}/transporte.png")
    finish()


def slots():
    prepare()
    a.command("clear @s")
    a.command("give @s minecraft:dirt 8")
    a.command("give @s minecraft:redstone 8")
    a.settle(0.5)
    ab.open_screen()
    a.settle(0.6)

    def state():
        screen = json.loads(a.ctl("screen"))
        return {s["slot"]: s["item"] for s in screen["slots"] if s["slot"] in (300, 301)}, screen.get("carried")

    for target in (300, 301):
        a.ctl("click-slot", "0")
        a.settle(0.3)
        a.ctl("click-slot", str(target))
        a.settle(0.5)
        held, carried = state()
        expect(f"a terra não entra no slot {target}", (held[target], (carried or {}).get("item")), (None, "minecraft:dirt"))
        a.ctl("click-slot", "0")
        a.settle(0.3)
    finish()


def video():
    """A cena do vídeo. Com `GRAVAR` (o nome do gravador de `gravar-jogo`, no desktop) grava o arquivo `cubo_tela.mp4` de dentro desta cena: o chat fica escondido durante a
    gravação (o chat oculto também impede comandos, então tudo o que precisa de comando vem antes e o `finish` depois)."""
    import subprocess
    prepare(1500000)
    gravador = os.environ.get("GRAVAR")
    rec = None
    if gravador:
        a.ctl("options-set", "chatVisibility", "2")
        rec = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {gravador} cubo_tela.mp4 22"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        a.settle(3.0)
    ab.open_screen()
    a.settle(2.0)
    tl.click(tl.SIDE_TAB)
    a.settle(2.0)
    tl.click(tl.at(tl.WIN, tl.SIDE["top"]))
    a.settle(3.0)
    tl.click(tl.at(tl.WIN, tl.SIDE["top"]))
    a.settle(2.0)
    a.ctl("close")
    a.settle(1.0)
    if rec:
        rec.wait()
        a.ctl("options-set", "chatVisibility", "0")
    finish()


def main():
    step = sys.argv[1] if len(sys.argv) > 1 else "telas"
    shots = sys.argv[2] if len(sys.argv) > 2 else "/home/luka/.local/share/jogo-isolado/horizon/data/shots/cubo_tela"
    {"telas": lambda: telas(shots), "slots": slots, "video": video}[step]()
    print("FALHAS" if failures else "OK", failures)
    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
