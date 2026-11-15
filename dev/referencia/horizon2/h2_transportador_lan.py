#!/usr/bin/env python3
"""As pilhas do transportador, com o brilho do encantamento, o dano e o nome, no anfitrião e no convidado de LAN (frente `transportador-sobras`, Fase 4, decisão 215): dois clientes no mesmo
container (`abrir_dois.sh anfitriao MUNDO`, `horizon-ctl lan open 0 commands`, `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`), a linha de `cenas/transportador_visual.json`
(`componentes`), parada por `tick freeze`, e o mesmo ponto de vista tirado primeiro pelo anfitrião e depois pelo convidado (o outro jogador fica longe, fora do quadro).

    H_NAME=horizon2 h2_transportador_lan.py rodar DESTINO     # monta a linha, liga o puxão, anda PASSOS ticks e tira `anfitriao_<passo>.png` e `convidado_<passo>.png`
    H_NAME=horizon2 h2_transportador_lan.py comparar DESTINO  # conta os pixels diferentes de cada par, no recorte da linha

O convidado desenha o `transit` que o pacote do servidor traz (`PacketTransporterSync`, com os componentes do `ItemResource`): um pixel de diferença entre os dois é o transporte."""
import os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
os.environ.setdefault("H_NAME", "horizon2")
import h2_dois as d
import h2_blocos as blocos
import gerar_cena_transportador_visual as g

PASSOS_BASE = 8
TIROS = 4
POSE = "83.5 -56.0 83.0 0 90"  # a mesma câmera de cima de `componentes_*` em `transportador_visual.json`
LONGE = (200, -50, 200)


def host_cmd(texto):
    return d.host("command", texto)


def tirar(quem, destino, nome):
    caminho = f"/tmp/{nome}.png"
    (d.host if quem == "anfitriao" else d.guest)("screenshot", caminho)
    subprocess.run(["ssh", "desktop", f"mkdir -p {destino} && docker exec {os.environ['H_NAME']} cat {caminho} > {destino}/{quem}_{nome}.png"], check=True)


def montar():
    """A linha das pilhas com componentes (a do roteiro), com a cor e o puxão ligados pelo Configurator do anfitrião."""
    import h2_identity as h
    for regra in ("advance_time", "advance_weather", "spawn_mobs"):
        host_cmd(f"gamerule {regra} false")
    host_cmd("weather clear")
    host_cmd("time set noon")
    host_cmd("tick unfreeze")
    for texto in g.SETUP:
        host_cmd(texto)
    h.ctl("give", "mekanism:configurator", "1")
    cmds, primeiro = g.linha(80, g.Z, None, g.COMPONENTES)
    for texto in cmds:
        if texto.startswith("#esperar "):
            time.sleep(int(texto.split()[1]) / 20.0)
        elif texto.startswith("#cor "):
            blocos.cor(texto)
        else:
            host_cmd(texto)
    host_cmd("tick freeze")
    blocos.conexao(f"#conexao {primeiro[0]} {g.Y} {primeiro[1]} west=pull")


def hud():
    for quem in (d.host, d.guest):
        quem("key", "f1", "tap")


def rodar(destino):
    montar()
    # espectadores: sem gravidade, sem mão e sem barra, e o `tp` fica onde foi posto nos dois clientes (o anfitrião voa e o convidado cai, e os olhos ficariam em alturas diferentes)
    host_cmd("gamemode spectator Convidado")
    host_cmd("gamemode spectator @s")
    hud()
    host_cmd(f"tick step {PASSOS_BASE}")
    time.sleep(PASSOS_BASE / 20.0 + 2.0)
    for k in range(TIROS):
        nome = f"passo_{PASSOS_BASE + k}"
        for quem in ("anfitriao", "convidado"):
            if quem == "anfitriao":
                host_cmd("tp @s " + POSE)
                host_cmd("tp Convidado %d %d %d" % LONGE)
            else:
                host_cmd("tp Convidado " + POSE)
                host_cmd("tp @s %d %d %d" % LONGE)
            time.sleep(2.5)
            tirar(quem, destino, nome)
        print("par", nome)
        host_cmd("tick step 1")
        time.sleep(1.0)
    hud()
    host_cmd("gamemode creative Convidado")
    host_cmd("gamemode creative @s")
    host_cmd("tick unfreeze")


def comparar(destino):
    """Os pixels diferentes de cada par (`magick compare -metric AE`, sem tolerância) no recorte da linha (a câmera de cima mostra os 7 transportadores no meio da janela)."""
    for nome in sorted(f[len("anfitriao_"):-4] for f in os.listdir(destino) if f.startswith("anfitriao_")):
        a, b = f"{destino}/anfitriao_{nome}.png", f"{destino}/convidado_{nome}.png"
        saida = subprocess.run(["magick", "compare", "-metric", "AE", a, b, "null:"], capture_output=True, text=True)
        n = round(float((saida.stderr or "0").split()[0]))
        print(f"{nome}: {n} pixels diferentes de {1920 * 1080} ({100.0 * n / (1920 * 1080):.3f} %)")


if __name__ == "__main__":
    if sys.argv[1] == "rodar":
        rodar(sys.argv[2])
    else:
        comparar(sys.argv[2])
