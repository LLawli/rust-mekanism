#!/usr/bin/env python3
"""O conteúdo dos transmissores no convidado de LAN contra o do anfitrião (frente `conteudo-tubo-cano`, Fase 4, passo 5): dois clientes no mesmo container
(`abrir_dois.sh anfitriao MUNDO`, `horizon-ctl lan open 0 commands`, `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`), a cena de `h2_cabo_conteudo.py` (as cinco faixas),
e para cada faixa e nível o mesmo ponto de vista tirado primeiro pelo anfitrião e depois pelo convidado (o outro jogador fica longe, fora do quadro).

    H_NAME=horizon2 h2_conteudo_lan.py rodar DESTINO    # monta, enche (cabo, cano de água e de lava, tubo de hidrogênio, condutor) e tira os prints `anfitriao_<faixa>_<nivel>.png` e `convidado_<faixa>_<nivel>.png`
    H_NAME=horizon2 h2_conteudo_lan.py video SAIDA.mp4 SEGUNDOS CONTAINER # o vídeo com som do convidado (a cena já montada por `rodar`)
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
    return "%.2f %.2f %.2f 0 20" % (cc.BASE[0] + 1.5, cc.BASE[1], cc.BASE[2] + cc.FAIXAS[faixa] - 2.4)  # no chão nos dois: o anfitrião voa e o convidado cai, e os olhos ficariam em alturas diferentes


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


GRAVADOR = "~/.cache/rust-mekanism/transmissor-sobras/gravar-jogo"  # a cópia do `gravar-jogo` com `PICK` (qual nó do gamescope grava)


def video(saida, segundos, container):
    """O convidado de LAN vendo os braços e o conteúdo, com som (`gravar-jogo rec`: o container tem de ter nascido com `JOGO_PW_SOCK`, e o gravador pega o nó do gamescope que `PICK` diz, `tail` para o
    segundo cliente, o convidado). A cena já está montada (`rodar`); o convidado fica de frente para cada faixa e o anfitrião enche as redes."""
    nome = os.environ.get("GRAVADOR_NOME", "transmissor-sobras")
    host_cmd("data merge block %s {component_config:{eject0:0b}}" % cc.txt(cc.at(-1, 0, cc.FAIXAS["cabo"])))
    cc.esvaziar("cabo", "mekanism:basic_universal_cable")
    host_cmd("tp Convidado " + pose_texto("cabo"))
    host_cmd("tp @s %d %d %d" % LONGE)
    time.sleep(3.0)
    gravar = f"PICK={os.environ.get('PICK', 'tail')} {GRAVADOR} rec {nome} {saida} {segundos} {container}"
    proc = subprocess.Popen(["ssh", "desktop", gravar])
    time.sleep(2.5)
    host_cmd("data merge block %s {component_config:{eject0:1b}}" % cc.txt(cc.at(-1, 0, cc.FAIXAS["cabo"])))
    time.sleep(6.0)
    host_cmd("data merge block %s {component_config:{eject0:0b}}" % cc.txt(cc.at(-1, 0, cc.FAIXAS["cabo"])))
    time.sleep(2.0)
    for faixa, passos in (("agua", 4), ("tubo", 4), ("lava", 3)):
        host_cmd("tp Convidado " + pose_texto(faixa))
        time.sleep(2.0)
        for _ in range(passos):
            cc.empurrar(faixa, 0.25)
            time.sleep(1.6)
    host_cmd("tp Convidado " + pose_texto("condutor"))
    time.sleep(2.0)
    cc.aquecer()
    time.sleep(6.0)
    proc.wait()


def comparar(destino):
    """Os pixels diferentes de cada par (`magick compare -metric AE -fuzz 3%`) dentro da faixa dos blocos (linhas 330 a 760); a LAN abre dois clientes na mesma tela do container, então o recorte tira o céu."""
    for nome in sorted(f[len("anfitriao_"):-4] for f in os.listdir(destino) if f.startswith("anfitriao_")):
        a, b = f"{destino}/anfitriao_{nome}.png", f"{destino}/convidado_{nome}.png"
        for origem, copia in ((a, "/tmp/lan_a.png"), (b, "/tmp/lan_b.png")):
            subprocess.run(["magick", origem, "-crop", "1920x430+0+330", "+repage", copia], check=True)
        saida = subprocess.run(["magick", "compare", "-metric", "AE", "-fuzz", "3%", "/tmp/lan_a.png", "/tmp/lan_b.png", "null:"], capture_output=True, text=True)
        n = round(float((saida.stderr or "0").split()[0]))
        print(f"{nome}: {n} pixels diferentes de {1920 * 430} ({100.0 * n / (1920 * 430):.3f} %)")


if __name__ == "__main__":
    if sys.argv[1] == "rodar":
        rodar(sys.argv[2])
    elif sys.argv[1] == "video":
        video(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        comparar(sys.argv[2])
