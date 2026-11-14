#!/usr/bin/env python3
"""O conteúdo animado dos transmissores no jogo (frente `cabo-conteudo`, Fase 4, decisões 212 a 214). Num mundo plano, quatro faixas lado a lado (z 0, 4, 8, 12, 16) vistas pela mesma câmera:

    H_NAME=horizon2 h2_cabo_conteudo.py montar       # as faixas: o cabo com um cubo de energia criativo, o cano de água e o de lava (por um caldeirão e `probe_push`), o tubo de hidrogênio e o condutor
    H_NAME=horizon2 h2_cabo_conteudo.py foto NOME    # um print da faixa que `FAIXA` diz (cabo, agua, lava, tubo, condutor), do mesmo ponto
    H_NAME=horizon2 h2_cabo_conteudo.py nivel FAIXA 0.5   # empurra a fração `0.5` da capacidade na rede da faixa (cano e tubo) e espera um tick
    H_NAME=horizon2 h2_cabo_conteudo.py video SAIDA.mp4 SEGUNDOS CONTAINER   # o vídeo com som: o cabo enche e esvazia, e o cano, o tubo e o condutor mostram o conteúdo (`enviar_fontes.sh ... sonda`)
    H_NAME=horizon2 h2_cabo_conteudo.py devolver     # devolve as gamerules e limpa a cena

A cena é a do vídeo e dos recortes: clima limpo e meio-dia, ciclos parados. O print vai para `SHOTS` no desktop e para `LOCAL` no notebook."""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h

BASE = (0, 150, 0)
GAMERULES = ("advance_time", "advance_weather")
FAIXAS = {"cabo": 0, "agua": 4, "lava": 8, "tubo": 12, "condutor": 16}
CAPACIDADE = {"agua": 4000, "lava": 4000, "tubo": 16000}
SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/cabo-conteudo/prints")
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/cabo-conteudo"))
LAVA = 'energy:0,machinery_basis:{v:1,tanks:[{resource:"minecraft:lava",amount:1000L}]}'


def cmd(text):
    try:
        return h.command(text)[0]
    except (KeyError, IndexError):
        return ""


def wait(n):
    h.ctl("wait", str(n))


def at(dx, dy=0, dz=0):
    return (BASE[0] + dx, BASE[1] + dy, BASE[2] + dz)


def txt(p):
    return "%d %d %d" % p


def ceu_limpo():
    cmd("weather clear")
    cmd("time set noon")
    for regra in GAMERULES:
        cmd(f"gamerule {regra} false")


def montar():
    ceu_limpo()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:stone" % (BASE[0] - 6, BASE[1] - 1, BASE[2] - 6, BASE[0] + 12, BASE[1] - 1, BASE[2] + 22))
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 6, BASE[1], BASE[2] - 6, BASE[0] + 12, BASE[1] + 8, BASE[2] + 22))
    wait(10)
    # o cabo: o cubo criativo à esquerda enche a rede (a escala sobe 0,02 por tick até 1)
    cmd("setblock %s mekanism:creative_energy_cube" % txt(at(-1, 0, FAIXAS["cabo"])))
    cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:1b},energy_amount:9223372036854775807L}" % txt(at(-1, 0, FAIXAS["cabo"])))
    for x in range(4):
        cmd("setblock %s mekanism:basic_universal_cable" % txt(at(x, 0, FAIXAS["cabo"])))
    for faixa in ("agua", "lava"):
        for x in range(4):
            cmd("setblock %s mekanism:basic_mechanical_pipe" % txt(at(x, 0, FAIXAS[faixa])))
    for x in range(4):
        cmd("setblock %s mekanism:basic_pressurized_tube" % txt(at(x, 0, FAIXAS["tubo"])))
    for x in range(4):
        cmd("setblock %s mekanism:basic_thermodynamic_conductor" % txt(at(x, 0, FAIXAS["condutor"])))
    cmd("setblock %s mekanismgenerators:heat_generator[facing=north]" % txt(at(3, 1, FAIXAS["condutor"])))
    wait(40)


def empurrar(faixa, fracao):
    p = at(0, 0, FAIXAS[faixa])
    quantidade = int(CAPACIDADE[faixa] * fracao)
    if faixa in ("agua", "lava"):
        fluido = "minecraft:water" if faixa == "agua" else "minecraft:lava"
        cmd('data merge block %s {probe_push:{fluid:"%s",amount:%d}}' % (txt(p), fluido, quantidade))
    else:
        cmd('data merge block %s {probe_push:{chem:"mekanism:hydrogen",amount:%d}}' % (txt(p), quantidade))
    wait(2)


def aquecer():
    cmd("data merge block %s {%s}" % (txt(at(3, 1, FAIXAS["condutor"])), LAVA))


def enquadrar(faixa):
    z = FAIXAS[faixa]
    cmd("tp @s %.2f %.2f %.2f" % (BASE[0] + 1.5, BASE[1] + 1.4, BASE[2] + z - 2.4))
    wait(12)
    h.ctl("look-at", "%.2f" % (BASE[0] + 1.5), "%.2f" % (BASE[1] + 0.5), "%.2f" % (BASE[2] + z + 0.5))
    wait(6)


def foto(nome, faixa):
    enquadrar(faixa)
    h.ctl("screenshot", f"/tmp/{nome}.png")
    subprocess.run(["ssh", "desktop", f"mkdir -p {SHOTS} && docker exec {os.environ['H_NAME']} cat /tmp/{nome}.png > {SHOTS}/{nome}.png"], check=True)
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{SHOTS}/{nome}.png", f"{LOCAL}/{nome}.png"], check=True)
    print(f"{LOCAL}/{nome}.png")


GRAVADOR = "~/.cache/rust-mekanism/gravar-jogo"
NOME_GRAVADOR = os.environ.get("GRAVADOR_NOME", "cabo-conteudo")


def pose(faixa, passo=0.0):
    """A câmera fixa da faixa (`/tp` com yaw e pitch, sem `look-at`, que faz a câmera derivar), a 2,4 blocos do eixo, olhando um pouco para baixo."""
    cmd("tp @s %.2f %.2f %.2f 0 20" % (BASE[0] + 1.5 + passo, BASE[1] + 1.4, BASE[2] + FAIXAS[faixa] - 2.4))


def esvaziar(faixa, bloco):
    """Tira os blocos da faixa um a um, do fim para o cubo: a rede perde a parte de cada um."""
    # O `setblock` não toca som: o som do bloco que sai e do que entra vem do `playsound` do mesmo evento do bloco de metal (o que o jogo toca ao quebrar e ao pôr).
    for x in (3, 2, 1, 0):
        cmd("setblock %s minecraft:air" % txt(at(x, 0, FAIXAS[faixa])))
        cmd("playsound minecraft:block.metal.break block @s %s 1 1" % txt(at(x, 0, FAIXAS[faixa])))
        time.sleep(0.7)
    for x in range(4):
        cmd("setblock %s %s" % (txt(at(x, 0, FAIXAS[faixa])), bloco))
        cmd("playsound minecraft:block.metal.place block @s %s 1 1" % txt(at(x, 0, FAIXAS[faixa])))


def video(out, segundos, container):
    """Roteiro de cerca de 45 s: o cabo enche com o cubo criativo e esvazia, o cano de água enche em quatro passos, o tubo de hidrogênio também, e o condutor esquenta com o Heat Generator."""
    montar()
    cmd("data merge block %s {component_config:{eject0:0b}}" % txt(at(-1, 0, FAIXAS["cabo"])))
    esvaziar("cabo", "mekanism:basic_universal_cable")
    cmd("tick unfreeze")
    pose("cabo")
    time.sleep(3.0)
    gravar = f"{GRAVADOR} rec {NOME_GRAVADOR} {out} {segundos}" + (f" {container}" if container else "")
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(2.5)
    cmd("data merge block %s {component_config:{eject0:1b}}" % txt(at(-1, 0, FAIXAS["cabo"])))
    time.sleep(6.0)
    cmd("data merge block %s {component_config:{eject0:0b}}" % txt(at(-1, 0, FAIXAS["cabo"])))
    esvaziar("cabo", "mekanism:basic_universal_cable")
    time.sleep(1.5)
    for faixa, n in (("agua", 4), ("tubo", 4)):
        pose(faixa)
        time.sleep(1.5)
        for passo in range(n):
            empurrar(faixa, 0.25)
            time.sleep(1.8)
        time.sleep(1.5)
    pose("lava")
    time.sleep(1.0)
    for passo in range(4):
        empurrar("lava", 0.25)
        time.sleep(1.2)
    pose("condutor")
    time.sleep(2.0)
    aquecer()
    time.sleep(8.0)
    proc.wait()


def devolver():
    cmd("tick unfreeze")
    cmd("fill %d %d %d %d %d %d minecraft:air" % (BASE[0] - 6, BASE[1] - 1, BASE[2] - 6, BASE[0] + 12, BASE[1] + 8, BASE[2] + 22))
    for regra in GAMERULES:
        cmd(f"gamerule {regra} true")


def main():
    what = sys.argv[1]
    if what == "montar":
        montar()
    elif what == "foto":
        foto(sys.argv[2], os.environ.get("FAIXA", "cabo"))
    elif what == "nivel":
        empurrar(sys.argv[2], float(sys.argv[3]))
    elif what == "aquecer":
        aquecer()
    elif what == "video":
        video(sys.argv[2], int(sys.argv[3]) if len(sys.argv) > 3 else 45, sys.argv[4] if len(sys.argv) > 4 else "")
    elif what == "devolver":
        devolver()


if __name__ == "__main__":
    main()
