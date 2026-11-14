#!/usr/bin/env python3
"""O conteúdo dos transmissores no convidado de LAN contra o do anfitrião (frente `conteudo-tubo-cano`, Fase 4, passo 5): dois clientes no mesmo container
(`abrir_dois.sh anfitriao MUNDO`, `horizon-ctl lan open 0 commands`, `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`), a cena de `h2_cabo_conteudo.py` (as cinco faixas),
e para cada faixa e nível o mesmo ponto de vista tirado primeiro pelo anfitrião e depois pelo convidado (o outro jogador fica longe, fora do quadro).

    H_NAME=horizon2 h2_conteudo_lan.py rodar DESTINO    # monta, enche (cabo, cano de água e de lava, tubo de hidrogênio, condutor) e tira os prints `anfitriao_<faixa>_<nivel>.png` e `convidado_<faixa>_<nivel>.png`
    H_NAME=horizon2 h2_conteudo_lan.py comparar DESTINO # conta os pixels diferentes de cada par (o recorte da faixa dos blocos)

O convidado desenha pelo `ClientTransmitters` dele (a escala que o servidor manda, `PacketNetworkScale`); um pixel de diferença é o transporte."""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_dois as d
import h2_cabo_conteudo as cc

LONGE = (cc.BASE[0] + 60, cc.BASE[1], cc.BASE[2] + 60)
NIVEIS = {"cabo": (1.0,), "agua": (0.0, 0.5, 1.0), "lava": (0.5, 1.0), "tubo": (0.0, 0.5, 1.0), "condutor": (1.0,)}


def host_cmd(texto):
    return d.host("command", texto)


def pose_texto(faixa):
    return "%.2f %.2f %.2f 0 20" % (cc.BASE[0] + 1.5, cc.BASE[1] + 1.4, cc.BASE[2] + cc.FAIXAS[faixa] - 2.4)


def tirar(quem, destino, nome):
    caminho = f"/tmp/{nome}.png"
    (d.host if quem == "anfitriao" else d.guest)("screenshot", caminho)
    subprocess.run(["ssh", "desktop", f"mkdir -p {destino} && docker exec {os.environ['H_NAME']} cat {caminho} > {destino}/{quem}_{nome}.png"], check=True)


def rodar(destino):
    cc.montar()
    host_cmd("data merge block %s {component_config:{eject0:1b}}" % cc.txt(cc.at(-1, 0, cc.FAIXAS["cabo"])))
    cc.aquecer()
    host_cmd("gamemode creative Convidado")
    for faixa, niveis in NIVEIS.items():
        for nivel in niveis:
            if faixa in ("agua", "lava", "tubo"):
                cc.empurrar(faixa, nivel)
            time.sleep(3.0)
            nome = f"{faixa}_{int(nivel * 100)}"
            for quem in ("anfitriao", "convidado"):
                if quem == "anfitriao":
                    host_cmd("tp @s " + pose_texto(faixa))
                    host_cmd("tp Convidado %d %d %d" % LONGE)
                else:
                    host_cmd("tp Convidado " + pose_texto(faixa))
                    host_cmd("tp @s %d %d %d" % LONGE)
                time.sleep(2.5)
                tirar(quem, destino, nome)
            print("par", nome)


def comparar(destino):
    from PIL import Image, ImageChops
    for nome in sorted(f[len("anfitriao_"):-4] for f in os.listdir(destino) if f.startswith("anfitriao_")):
        a = Image.open(f"{destino}/anfitriao_{nome}.png").convert("RGB")
        b = Image.open(f"{destino}/convidado_{nome}.png").convert("RGB")
        diff = ImageChops.difference(a, b).convert("L").point(lambda v: 255 if v > 8 else 0)
        n = sum(1 for p in diff.getdata() if p)
        print(f"{nome}: {n} pixels diferentes de {a.size[0] * a.size[1]} ({100.0 * n / (a.size[0] * a.size[1]):.3f} %)")


if __name__ == "__main__":
    if sys.argv[1] == "rodar":
        rodar(sys.argv[2])
    else:
        comparar(sys.argv[2])
