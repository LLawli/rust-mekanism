#!/usr/bin/env python3
"""A mira nos transmissores no jogo (frente `cliente-mira`, Fase 3, decisões 166 a 168): o clique no centro de um cabo com um item que não é o Configurator, e o contorno do
Configurator num braço `PUSH` e `PULL`.

    H_NAME=horizon2 PASTA=<pasta do game.log> h2_cabo_mira.py centro            # graveto, liga e Configurator mirando o centro (e a face de cima) de um cabo: onde a mira cai, se o clique chega ao item
    H_NAME=horizon2 NOME=... h2_cabo_mira.py video                              # o vídeo com som: o contorno em NORMAL, PUSH e PULL e a liga no centro de um cabo
    H_NAME=horizon2 h2_cabo_mira.py estado                                      # a posição, o yaw e o pitch do jogador (para repetir a câmera no original)

A sonda (`TRANSMITTER_PROBE`, `enviar_fontes.sh`) escreve no stderr do jogo (`PASTA/game.log`): `MBL` cada `use_item_on` (o item e o bloco) e `MBO` a caixa do contorno do Configurator.
O contorno do bloco só aparece na captura PipeWire do gamescope (o `horizon-ctl screenshot` não o desenha): os prints saem do vídeo, nas marcas."""
import json, os, subprocess, sys, threading, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h
import h2_ajustes_jogo as aj

PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/cliente-mira/jogo")
aj.PASTA = PASTA
cmd, wait, at, txt = aj.cmd, aj.wait, aj.at, aj.txt
CABO = "mekanism:basic_universal_cable"
CUBO = "mekanism:creative_energy_cube"
LIGA = "mekanism:alloy_infused"
LIGAS = ["mekanism:alloy_infused", "mekanism:alloy_reinforced", "mekanism:alloy_atomic"]
NIVEIS = ["mekanism:advanced_universal_cable", "mekanism:elite_universal_cable", "mekanism:ultimate_universal_cable"]


def log_count(prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -ac '^{prefix} ' {PASTA}/game.log"], capture_output=True, text=True, timeout=60).stdout.strip()
    return int(out or 0)


def log_tail(prefix, count):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {PASTA}/game.log | tail -n {count}"], capture_output=True, text=True, timeout=60).stdout
    return [line for line in out.splitlines() if line]


def eye_of(block, dz=-2.6):
    return (block[0] + 0.5, block[1] + 1.0, block[2] + dz)


def linha(count=3):
    aj.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    h.ctl("options-set", "chatVisibility", "2")
    for x in range(count):
        cmd("setblock %s %s" % (txt(at(x)), CABO))
    wait(40)


def mirar(item, ponto, de=None):
    """O jogador com `item` na mão, olhando o ponto `ponto` (coordenadas do bloco do meio) de `de` (a posição dos pés; padrão: ao norte do bloco, a 2,6 de distância)."""
    mid = at(1)
    cmd("tp @s %.2f %.2f %.2f" % (de or eye_of(mid)))
    wait(12)
    cmd("clear @s")
    h.ctl("give", item, "1")
    h.ctl("select", item)
    h.ctl("look-at", *("%.3f" % (mid[i] + ponto[i]) for i in range(3)))
    wait(8)
    return aj.looking()


def centro():
    """O que o port faz hoje com o clique no centro de um cabo: para cada item, onde a mira cai (`looking-at`), se o `use_item_on` do mod recebeu o clique (a sonda `MBL`) e se o cabo subiu."""
    linha(3)
    mid = at(1)
    pontos = {"centro": (0.5, 0.5, 0.5), "centro_alto": (0.5, 0.62, 0.5), "centro_baixo": (0.5, 0.38, 0.5)}
    for item in ("minecraft:stick", LIGA, "mekanism:configurator"):
        for nome, ponto in pontos.items():
            cmd("setblock %s %s" % (txt(mid), CABO))
            wait(10)
            aimed = mirar(item, ponto)
            bruto = h.ctl("looking-at").strip()
            antes = log_count("MBL")
            h.ctl("mouse", "right", "click")
            wait(10)
            time.sleep(1.0)
            depois = log_count("MBL")
            subiu = "Test passed" in cmd("execute if block %s %s" % (txt(mid), NIVEIS[0]))
            print(f"{item:28} {nome:13} mira={aimed} MBL +{depois - antes} subiu={subiu}")
            print("   ", bruto[:240])
    for linha_ in log_tail("MBL", 12):
        print(linha_)


def topo():
    """O mesmo, de cima: o jogador sobre o cabo olhando o centro da face de cima."""
    linha(3)
    mid = at(1)
    for item in ("minecraft:stick", LIGA):
        cmd("setblock %s %s" % (txt(mid), CABO))
        wait(10)
        aimed = mirar(item, (0.5, 0.6875, 0.5), (mid[0] + 0.5, mid[1] + 2.0, mid[2] + 0.5))
        antes = log_count("MBL")
        h.ctl("mouse", "right", "click")
        wait(10)
        time.sleep(1.0)
        subiu = "Test passed" in cmd("execute if block %s %s" % (txt(mid), NIVEIS[0]))
        print(f"{item:28} topo mira={aimed} MBL +{log_count('MBL') - antes} subiu={subiu}")


def estado():
    print(h.ctl("state"))


def montar_contorno():
    """Dois cabos e o cubo de energia criativo: o braço leste do cabo do meio dá no cubo, e é ele que o Configurator cicla (`NORMAL`, `PUSH`, `PULL`, `NONE`)."""
    aj.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd("tick unfreeze")
    for x in range(2):
        cmd("setblock %s %s" % (txt(at(x)), CABO))
    cmd("setblock %s %s" % (txt(at(2)), CUBO))
    wait(10)
    cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:0b},energy_amount:9223372036854775807L}" % txt(at(2)))
    wait(40)
    h.ctl("give", "mekanism:configurator", "1")


def roteiro_contorno(pausa, marca):
    """O contorno do Configurator no braço leste do cabo do meio, em `NORMAL`, `PUSH` e `PULL`. A câmera é a mesma nos três: a do `click_at`."""
    montar_contorno()
    mid = at(1)
    eye = eye_of(mid)
    cmd("tp @s %.2f %.2f %.2f" % eye)
    wait(12)
    h.ctl("select", "mekanism:configurator")
    h.ctl("look-at", *("%.3f" % (mid[i] + aj.ARM["east"][i]) for i in range(3)))
    wait(8)
    marca("contorno_normal")
    pausa(3)
    for nome in ("push", "pull"):
        aj.click_at(mid, aj.ARM["east"], eye)
        wait(8)
        h.ctl("select", "mekanism:configurator")
        h.ctl("look-at", *("%.3f" % (mid[i] + aj.ARM["east"][i]) for i in range(3)))
        wait(6)
        marca(f"contorno_{nome}")
        pausa(3)


def roteiro_centro(pausa, marca):
    """A liga mirando o centro de um cabo (o botão direito do mouse): a linha de 5 cabos sobe do básico ao supremo, uma liga por vez."""
    aj.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    for x in range(5):
        cmd("setblock %s %s" % (txt(at(x, 0, 6)), CABO))
    wait(40)
    mid = at(1, 0, 6)
    cmd("tp @s %.2f %.2f %.2f" % eye_of(mid))
    wait(12)
    h.ctl("look-at", *("%.3f" % (mid[i] + (0.5, 0.5, 0.5)[i]) for i in range(3)))
    wait(6)
    marca("centro_basico")
    pausa(2)
    for nivel, liga in enumerate(LIGAS, 1):
        h.ctl("give", liga, "1")
        h.ctl("select", liga)
        h.ctl("look-at", *("%.3f" % (mid[i] + (0.5, 0.5, 0.5)[i]) for i in range(3)))
        wait(6)
        h.ctl("mouse", "right", "click")
        wait(8)
        marca(f"centro_nivel{nivel}")
        pausa(2)


def video():
    """O vídeo com som: `NOME` (o `gravar-jogo start`), `SEGUNDOS` (padrão 50), `ARQUIVO` (padrão cabo-mira.mp4), `ROTEIRO` (`contorno`, `centro` ou `ambos`, o padrão). O volume mestre só sobe
    depois de o stream do container estar no sink nulo; uma thread confere a cada meio segundo e o volta a 0 se o stream sair dos sinks de teste."""
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "50"), os.environ.get("ARQUIVO", "cabo-mira.mp4")
    roteiro = os.environ.get("ROTEIRO", "ambos")
    n.sky()
    cmd("tick unfreeze")
    n.mute()
    h.ctl("options-set", "chatVisibility", "2")
    for categoria in n.SOUND_OFF:
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    for categoria in ("block", "record"):
        h.ctl("options-set", f"soundCategory_{categoria}", "1.0")
    ligado = False
    for _ in range(60):
        if "movidos 0" not in n.desktop(f"ligar {n.CONTAINER}").stdout and n.desktop(f"verificar {n.CONTAINER}").stdout.startswith("ok"):
            ligado = True
            break
        time.sleep(1)
    if not ligado:
        print("o stream do jogo não foi para o sink nulo: nada gravado, volume mestre em 0")
        sys.exit(1)
    parar = threading.Event()
    grav = f"som_grav_{nome}"
    ate = [None]

    def vigia():
        while not parar.is_set():
            if ate[0] is not None and time.time() < ate[0]:
                subprocess.run(["ssh", "desktop", f"AUDIO_SINK={grav} {n.AUDIO_NULO} ligar {n.CONTAINER}"], capture_output=True, text=True)
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    thread = threading.Thread(target=vigia, daemon=True)
    thread.start()
    rec = None
    marcas = []
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(2.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        t0 = time.time()
        time.sleep(3.0)
        ate[0] = time.time() + int(segundos) - 8

        def marca(rotulo):
            marcas.append((rotulo, round(time.time() - t0, 1)))

        if roteiro in ("contorno", "ambos"):
            roteiro_contorno(lambda s: time.sleep(s), marca)
        if roteiro in ("centro", "ambos"):
            roteiro_centro(lambda s: time.sleep(s), marca)
        out, err = rec.communicate(timeout=int(segundos) + 90)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        thread.join(timeout=5)
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        n.restore()
    print("marcas", json.dumps(marcas))


if __name__ == "__main__":
    {"centro": centro, "topo": topo, "estado": estado, "video": video}[sys.argv[1]]()
