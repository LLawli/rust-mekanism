#!/usr/bin/env python3
"""O lado do cliente dos transmissores e o upgrade pela liga no jogo (frente `cabo-cliente`, Fase 3, decisões 151 a 153).

    H_NAME=horizon2 DOIS_DIR=<pasta dos logs a.log e b.log> h2_cabo_cliente.py escala     # a escala de cada rede chega ao anfitrião e ao convidado, igual
    H_NAME=horizon2 DOIS_DIR=... h2_cabo_cliente.py liga cabo|cano                        # a liga sobe o tier da linha: blocos, capacidade da rede, energia, cliente
    H_NAME=horizon2 DOIS_DIR=... h2_cabo_cliente.py push                                  # o Configurator em PUSH e PULL: o contorno (sonda MBO) e os pacotes (MBG) do convidado

Dois clientes (`abrir_dois.sh anfitriao spike` e `abrir_dois.sh convidado 127.0.0.1:PORTA Convidado`), na mesma vaga. O anfitrião é quem age (o jogador); o convidado só observa, perto da linha.
A sonda (`TRANSMITTER_PROBE`, `enviar_fontes.sh`): `MBS` a escala que o cliente (`who` host ou guest) guardou de uma rede, `MBG` o pacote de atualização de um transmissor, `MBN` o que o servidor
mandou a cada convidado, `MBE` e `MBP` a rede do servidor, `MBO` a caixa do contorno do Configurator."""
import json, os, subprocess, sys, time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon2")
import h2_identity as h
import h2_ajustes_jogo as aj
import h2_dois as d

DOIS = os.environ.get("DOIS_DIR", "~/.cache/rust-mekanism/cabo-cliente/dois")
aj.PASTA = DOIS  # log_lines() do ajustes lê PASTA/game.log; aqui o do anfitrião é a.log
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/cabo-cliente"))
aj.LOCAL = LOCAL
aj.SHOTS = os.environ.get("SHOTS", "~/.cache/rust-mekanism/cabo-cliente/prints")
BASE = aj.BASE
LINHA = 5
BLOCOS = {
    "cabo": ["mekanism:basic_universal_cable", "mekanism:advanced_universal_cable", "mekanism:elite_universal_cable", "mekanism:ultimate_universal_cable"],
    "cano": ["mekanism:basic_mechanical_pipe", "mekanism:advanced_mechanical_pipe", "mekanism:elite_mechanical_pipe", "mekanism:ultimate_mechanical_pipe"],
}
LIGAS = ["mekanism:alloy_infused", "mekanism:alloy_reinforced", "mekanism:alloy_atomic"]
cmd, wait, at, txt = aj.cmd, aj.wait, aj.at, aj.txt


def log(file, prefix):
    out = subprocess.run(["ssh", "desktop", f"grep -a '^{prefix} ' {DOIS}/{file}.log | cut -c{len(prefix) + 2}-"], capture_output=True, text=True, timeout=120).stdout
    rows = []
    for line in out.splitlines():
        try:
            rows.append(json.loads(line))
        except ValueError:
            pass
    return rows


def observer_near(x=2, z=-3):
    cmd("tp Convidado %d %d %d" % (BASE[0] + x, BASE[1], BASE[2] + z))


def escala():
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    observer_near()
    for x in range(LINHA):
        cmd("setblock %s %s" % (txt(at(x)), BLOCOS["cabo"][0]))
    cmd("setblock %s minecraft:air" % txt(at(0, 0, 1)))
    cmd("setblock %s mekanism:creative_energy_cube" % txt(at(-1)))
    cmd("setblock %s mekanism:enrichment_chamber" % txt(at(LINHA)))
    wait(10)
    cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:1b},energy_amount:9223372036854775807L}" % txt(at(-1)))
    cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(at(LINHA)))
    wait(160)
    time.sleep(8)
    host = log("a", "MBS")  # com a LAN aberta a janela do anfitrião também recebe os pacotes (who=guest); sem LAN, who=host
    guest = log("b", "MBS")
    sent = log("a", "MBN")
    print("o anfitrião guardou", len(host), "escalas;", "o convidado", len(guest), "; o servidor mandou ao convidado", len(sent))
    print("anfitrião:", [(r["scale"], r.get("cap"), r.get("n")) for r in host[-8:]])
    print("convidado:", [(r["scale"], r.get("cap"), r.get("n")) for r in guest[-8:]])
    gg = [r["scale"] for r in guest]
    # Com a LAN aberta a janela do anfitrião também é um jogador do servidor: `MBN` traz as duas sequências (uma por jogador, em `to`), e o convidado tem de ter recebido uma delas inteira.
    por_jogador = {}
    for r in sent:
        por_jogador.setdefault(r["to"], []).append(r["scale"])
    igual_a_algum = gg in por_jogador.values()
    ok = bool(gg) and igual_a_algum and [r["scale"] for r in host] == gg
    print("anfitrião == convidado:", [r["scale"] for r in host] == gg, "| convidado == o que o servidor mandou a um jogador:", igual_a_algum, "| capacidade da rede no convidado:", guest[-1].get("cap") if guest else None, "(esperado", LINHA * 8000, ")")
    print("fim", "OK" if ok and guest[-1].get("cap") == LINHA * 8000 else "DIFF")


def usar_liga(item, pos):
    """O botão direito do mouse com a liga na mão, mirando o braço leste do transmissor em `pos`: o raio do jogo (`pick`) só acerta os braços de um transmissor, e não o centro, quando
    o Configurator não está na mão (o clique no centro não chega ao `use_hand`; observação da frente `cabo-cliente`, decisão 152)."""
    cmd("tp @s %.2f %.2f %.2f" % (pos[0] + 0.5, pos[1] + 1.0, pos[2] - 2.6))
    wait(12)
    h.ctl("give", item, "1")
    h.ctl("select", item)
    h.ctl("look-at", *("%.3f" % (pos[i] + aj.ARM["east"][i]) for i in range(3)))
    wait(8)
    print("mira:", aj.looking())
    h.ctl("mouse", "right", "click")
    wait(8)


def verify_blocks(kind, level):
    ok = True
    for x in range(LINHA):
        out = cmd("execute if block %s %s" % (txt(at(x)), BLOCOS[kind][level]))
        ok &= "Test passed" in out
    return ok


def liga(kind):
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    h.ctl("options-set", "chatVisibility", "2")
    observer_near()
    for x in range(LINHA):
        cmd("setblock %s %s" % (txt(at(x)), BLOCOS[kind][0]))
    if kind == "cabo":
        cmd("setblock %s mekanism:creative_energy_cube" % txt(at(-1)))
        cmd("setblock %s mekanism:enrichment_chamber" % txt(at(LINHA)))
        wait(10)
        cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:1b},energy_amount:9223372036854775807L}" % txt(at(-1)))
        cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(at(LINHA)))
    wait(60)
    mid, eye = aj.camera(LINHA)
    aj.enquadrar(LINHA)
    antes = aj.shot(f"liga_{kind}_antes")
    ok = verify_blocks(kind, 0)
    print("antes: todos básicos:", ok)
    for nivel in (1, 2, 3):
        usar_liga(LIGAS[nivel - 1], at(1))
        wait(30)
        time.sleep(1.5)
        good = verify_blocks(kind, nivel)
        ok &= good
        cap = [r for r in log("a", "MBE" if kind == "cabo" else "MBP")][-1:]
        guest = [r for r in log("b", "MBG")][-1:]
        print(("OK   " if good else "DIFF "), "nível", nivel, "todos subiram:", good, "| rede no servidor:", cap, "| convidado:", guest)
        aj.enquadrar(LINHA)
        depois = aj.shot(f"liga_{kind}_n{nivel}")
        print("empilhado:", aj.stack(antes, depois, f"{LOCAL}/liga_{kind}_antes_n{nivel}.png", "antes (básico)", f"depois (nível {nivel})"))
    print("fim", "OK" if ok else "DIFF")


def push():
    aj.clear()
    cmd("gamemode creative @s")
    cmd("tick unfreeze")
    observer_near()
    # O braço leste do segundo cabo dá numa máquina: só aí o tipo cru (`PUSH`, `PULL`) é o que o lado mostra (entre dois cabos o lado mostra `NORMAL`).
    for x in range(2):
        cmd("setblock %s %s" % (txt(at(x)), BLOCOS["cabo"][0]))
    cmd("setblock %s mekanism:creative_energy_cube" % txt(at(2)))
    wait(10)
    cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:0b},energy_amount:9223372036854775807L}" % txt(at(2)))
    wait(40)
    time.sleep(1.0)
    h.ctl("give", "mekanism:configurator", "1")
    middle = at(1)
    eye = (middle[0] + 0.5, middle[1] + 1.0, middle[2] - 2.6)
    for rotulo, cliques in (("normal", 0), ("push", 1), ("pull", 2)):
        for _ in range(1 if cliques else 0):
            pass
        if cliques:
            aj.click_at(middle, aj.ARM["east"], eye)
            wait(10)
        # Só olha o braço leste com o Configurator na mão (a sonda MBO escreve a caixa do contorno).
        h.ctl("select", "mekanism:configurator")
        h.ctl("tp", "%.2f" % eye[0], "%.2f" % eye[1], "%.2f" % eye[2]) if False else cmd("tp @s %.2f %.2f %.2f" % eye)
        wait(12)
        h.ctl("look-at", *("%.3f" % (middle[i] + aj.ARM["east"][i]) for i in range(3)))
        wait(10)
        time.sleep(1.0)
        mbo = [r for r in log("a", "MBO") if r["p"] == list(middle)][-1:]
        kinds = [r for r in log("a", "MBV") if r["p"] == list(middle)][-1:]
        cliente = [r for r in log("a", "MBG") if r["p"] == list(middle) and True][-1:]
        convidado = [r for r in log("b", "MBG") if r["p"] == list(middle)][-1:]
        print(rotulo, "| contorno:", mbo, "| servidor:", kinds and kinds[0].get("kind"), "| cliente anfitrião:", cliente and cliente[0].get("kind"), "| convidado:", convidado and convidado[0].get("kind"))


def roteiro_video(pausa, marca):
    """O que o vídeo mostra: a linha de cabos ligada (a Enrichment Chamber trabalha) sobe do básico ao supremo com as três ligas, a linha de canos também, e o Configurator põe
    `PUSH` e `PULL` no braço de um cabo que dá no cubo (o contorno do braço). `marca(nome)` anota o instante para tirar os quadros do vídeo."""
    aj.clear()
    cmd("gamemode creative @s")
    for x in range(LINHA):
        cmd("setblock %s %s" % (txt(at(x)), BLOCOS["cabo"][0]))
        cmd("setblock %s %s" % (txt(at(x, 0, 6)), BLOCOS["cano"][0]))
    cmd("setblock %s mekanism:creative_energy_cube" % txt(at(-1)))
    cmd("setblock %s mekanism:enrichment_chamber" % txt(at(3, 0, 1)))
    # O cabo com o braço no cubo, para o Configurator.
    for x in (8, 9):
        cmd("setblock %s %s" % (txt(at(x)), BLOCOS["cabo"][0]))
    cmd("setblock %s mekanism:creative_energy_cube" % txt(at(10)))
    wait(10)
    for cube in (at(-1), at(10)):
        cmd("data merge block %s {component_config:{config0:[I;4,4,4,4,4,4],eject0:%s},energy_amount:9223372036854775807L}" % (txt(cube), "1b" if cube == at(-1) else "0b"))
    cmd("data merge block %s {Items:[{Slot:0b,id:\"minecraft:iron_ore\",count:64}]}" % txt(at(3, 0, 1)))
    h.ctl("give", "mekanism:configurator", "1")
    for item in LIGAS:
        h.ctl("give", item, "1")
    wait(60)
    observer_near()
    # A linha de cabos.
    for dz, kind, rotulo in ((0, "cabo", "cabo"), (6, "cano", "cano")):
        pos = at(1, 0, dz)
        cmd("tp @s %.2f %.2f %.2f" % (pos[0] + 0.5, pos[1] + 1.0, pos[2] - 2.6))
        wait(12)
        h.ctl("look-at", "%.2f" % (pos[0] + 1.0), "%.2f" % (pos[1] + 0.5), "%.2f" % (pos[2] + 0.5))
        wait(6)
        marca(f"{rotulo}_basico")
        pausa(3)
        for nivel, item in enumerate(LIGAS, 1):
            h.ctl("select", item)
            h.ctl("look-at", *("%.3f" % (pos[i] + aj.ARM["east"][i]) for i in range(3)))
            wait(6)
            h.ctl("mouse", "right", "click")
            wait(6)
            marca(f"{rotulo}_nivel{nivel}")
            pausa(2)
    # O Configurator num braço que dá no cubo: normal, PUSH, PULL.
    mid = at(9)
    eye = (mid[0] + 0.5, mid[1] + 1.0, mid[2] - 2.6)
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
        h.ctl("look-at", *("%.3f" % (mid[i] + aj.ARM["east"][i]) for i in range(3)))
        wait(6)
        marca(f"contorno_{nome}")
        pausa(3)


def video():
    """O vídeo com som: `NOME` (o `gravar-jogo start`), `SEGUNDOS` (padrão 60), `ARQUIVO` (padrão cabo-cliente.mp4). O volume mestre só sobe depois de o stream do container estar no sink nulo."""
    import threading
    import h2_nota_instrumento as n
    nome, segundos, arquivo = os.environ["NOME"], os.environ.get("SEGUNDOS", "60"), os.environ.get("ARQUIVO", "cabo-cliente.mp4")
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

        roteiro_video(lambda s: time.sleep(s), marca)
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
    {"escala": escala, "liga": lambda: liga(sys.argv[2] if len(sys.argv) > 2 else "cabo"), "push": push, "video": video}[sys.argv[1]]()
