#!/usr/bin/env python3
"""O modo de depuração no jogo (frente `mek-debug`, Fase 3): o `/mek debug`, o leitor de rede agachado com a depuração ligada e os logs do registro.

    H_NAME=horizon PASTA=<pasta do game.log> h2_mek_debug.py textos   # o chat do `/mek debug` (liga e desliga), o do leitor agachado numa pedra contra as redes do original, o log do registro
    H_NAME=horizon PASTA=<pasta do game.log> h2_mek_debug.py prints   # o chat do port e o do original (`CenaLeitor`, `cenas/leitor_debug.txt`), recortados no mesmo trecho e empilhados
    H_NAME=horizon NOME=... SOCK=... h2_mek_debug.py video            # o vídeo com som: o comando, o leitor na pedra e no cabo, o comando de novo

O texto vem de `horizon-ctl messages`, com a tradução do mod. O rastro do original é `rastros/network_reader_debug.jsonl`: as dez redes de `built/debug_off` (duas por família, na mesma geometria da cena daqui) dão as
linhas que o leitor tem de escrever, em qualquer ordem (a do port é sorteada como a da JVM; a ordem é conferida contra o rastro por `transmission_debug_trace.rs`).
"""
import json, os, subprocess, sys, time
from collections import Counter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
os.environ.setdefault("H_NAME", "horizon")
import h2_ajustes_jogo as a
import h2_identity as h
import h2_network_reader as nr

TOOLS = os.path.dirname(os.path.abspath(__file__))
RASTRO = os.path.join(TOOLS, "..", "rastros", "network_reader_debug.jsonl")
LOCAL = os.path.expanduser(os.environ.get("LOCAL", "~/.cache/rust-mekanism/wa/mek-debug"))
ORIGINAIS = os.path.join(LOCAL, "original")
PASTA = os.environ.get("PASTA", "~/.cache/rust-mekanism/restos")
cmd, wait, at, txt = a.cmd, a.wait, a.at, a.txt
FAMILIAS = [
    ("mekanism:basic_universal_cable", 0),
    ("mekanism:basic_mechanical_pipe", 2),
    ("mekanism:basic_pressurized_tube", 4),
    ("mekanism:basic_thermodynamic_conductor", 6),
    ("mekanism:basic_logistical_transporter", 8),
]
BORDA = "-------------"


def ctl(*args):
    return h.ctl(*args)


def linhas(n=40):
    return [l.strip() for l in nr.messages(n)]


def esperadas():
    """As linhas de rede do original na cena das dez redes (a nota `built/debug_off` do rastro), como o jogador as lê."""
    table = nr.lang()
    with open(RASTRO) as file:
        for line in file:
            row = json.loads(line)
            if row.get("case") == "built/debug_off":
                return [nr.render(network["text"], table) for network in row["registry"]]
    raise SystemExit("sem a nota built/debug_off")


def limpar():
    a.clear()
    cmd("gamemode creative @s")
    cmd("clear @s")
    ctl("options-set", "chatVisibility", "0")
    ctl("options-set", "guiScale", "3")
    cmd("weather clear")
    ctl("time", "6000")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")


def dez_redes():
    """Duas linhas por família (x = 1..2 e 4..6) em fileiras de z 0, 2, 4, 6 e 8: a cena do rastro."""
    for block, dz in FAMILIAS:
        for dx in (1, 2, 4, 5, 6):
            cmd("setblock %s %s" % (txt(at(dx, 0, dz)), block))
    wait(60)


def depuracao(estado):
    """`/mek debug` como o jogador digita; devolve as linhas novas do chat."""
    antes = linhas(60)
    ctl("chat", "/mek debug")
    wait(10)
    time.sleep(1.0)
    depois = linhas(60)
    return depois[len(antes):] if depois[:len(antes)] == antes else depois[-3:]


def apos_tp(linhas_):
    """As linhas do chat depois do último `tp` (o clique do leitor começa por um): o que o clique escreveu."""
    for i in range(len(linhas_) - 1, -1, -1):
        if linhas_[i].startswith("Teleported"):
            return linhas_[i + 1:]
    return linhas_


def textos():
    ok = True

    def confere(nome, got, want):
        nonlocal ok
        igual = got == want
        ok &= igual
        print("[%s] %s" % ("igual" if igual else "DIFERE", nome))
        if not igual:
            print("  jogo:", got)
            print("  original:", want)

    limpar()
    dez_redes()
    ctl("give", nr.READER, "1")
    wait(6)
    ctl("select", nr.READER)
    # O comando: liga, desliga, liga (o texto é o de `command.mekanism.debug` com `boolean.mekanism.on` e `off`).
    confere("comando liga", depuracao(True)[-1:], ["Toggled debug mode: on."])
    confere("comando desliga", depuracao(False)[-1:], ["Toggled debug mode: off."])
    # Com a depuração desligada o clique agachado na pedra não escreve nada.
    pedra = at(3, -1, -3)
    depois = nr.clicar(pedra, (0.5, 1.0, 0.5), sneak=True, stand=(at(3)[0] + 0.5, at(3)[1] + 2.5, at(3)[2] - 4.4))
    confere("pedra agachado sem depuracao", apos_tp(depois), [])
    confere("comando liga de novo", depuracao(True)[-1:], ["Toggled debug mode: on."])
    depois = nr.clicar(pedra, (0.5, 1.0, 0.5), sneak=True, stand=(at(3)[0] + 0.5, at(3)[1] + 2.5, at(3)[2] - 4.4))
    novas = apos_tp(depois)
    # A borda, uma linha por rede e a borda final.
    inicio = max(i for i, l in enumerate(novas) if "Mekanism Debug" in l)
    fim = max(i for i, l in enumerate(novas) if "[=======]" in l)
    corpo = [l for l in novas[inicio + 1:fim]]
    print("borda:", novas[inicio], "|", novas[fim])
    confere("redes do registro (sem ordem)", sorted(corpo), sorted(esperadas()))
    # Sem agachar, nada.
    depois = nr.clicar(pedra, (0.5, 1.0, 0.5), sneak=False, stand=(at(3)[0] + 0.5, at(3)[1] + 2.5, at(3)[2] - 4.4))
    confere("pedra sem agachar", apos_tp(depois), [])
    # O log do registro: um cabo solto longe das outras redes.
    cmd("setblock %s %s" % (txt(at(10, 0, 0)), FAMILIAS[0][0]))
    wait(30)
    log = subprocess.run(["ssh", "desktop", f"grep -a 'orphan Transmitters\\|No networks found\\|Merging\\|single found\\|invalid Transmitters' {PASTA}/game.log | tail -20"], capture_output=True, text=True).stdout
    print(log)
    ok &= "No networks found. Creating new network for 1 transmitters" in log
    cmd("setblock %s minecraft:air" % txt(at(10, 0, 0)))
    wait(30)
    ok &= "Dealing with 1 invalid Transmitters" in subprocess.run(["ssh", "desktop", f"grep -a 'invalid Transmitters' {PASTA}/game.log | tail -3"], capture_output=True, text=True).stdout
    # Desliga: o cabo seguinte não escreve linha nova.
    depuracao(False)
    antes_log = subprocess.run(["ssh", "desktop", f"grep -ac 'Dealing with' {PASTA}/game.log"], capture_output=True, text=True).stdout.strip()
    cmd("setblock %s %s" % (txt(at(10, 0, 0)), FAMILIAS[0][0]))
    wait(30)
    depois_log = subprocess.run(["ssh", "desktop", f"grep -ac 'Dealing with' {PASTA}/game.log"], capture_output=True, text=True).stdout.strip()
    confere("log com depuracao desligada", depois_log, antes_log)
    cmd("setblock %s minecraft:air" % txt(at(10, 0, 0)))
    print("RESULTADO", "igual" if ok else "DIFERE")
    return ok


# O nome do print do original (`CenaLeitor`, `cenas/leitor_debug.txt`), quantas linhas do chat o clique escreve.
LINHAS = {"debug_ligado": 1, "debug_desligado": 1, "pedra_debug": 3, "cabo_debug": 8}


def corte(nome):
    altura = LINHAS[nome] * nr.PASSO + 4
    return "1000x%d+0+%d" % (altura, 960 - altura)


def empilhar(original, port, saida, nome):
    partes = []
    for arquivo, rotulo in ((original, "original"), (port, "port")):
        recorte = arquivo + ".corte.png"
        subprocess.run(["magick", arquivo, "-crop", corte(nome), "+repage", "-gravity", "NorthEast", "-fill", "white", "-undercolor", "#000000aa", "-pointsize", "28", "-annotate", "+8+8", rotulo, recorte], check=True)
        partes.append(recorte)
    subprocess.run(["magick", *partes, "-append", saida], check=True)
    for parte in partes:
        os.remove(parte)


def prints():
    a.SHOTS = "/home/luka/.cache/rust-mekanism/mek-debug/prints"
    a.LOCAL = LOCAL
    limpar()
    ctl("give", nr.READER, "1")
    wait(6)
    ctl("select", nr.READER)
    cmd("setblock %s %s" % (txt(at(1)), FAMILIAS[0][0]))
    wait(30)
    a.enquadrar(3)
    for nome in ("debug_ligado", "debug_desligado"):
        ctl("chat", "/mek debug")
        wait(20)
        time.sleep(1.0)
        arquivo = a.shot("leitor_" + nome)
        empilhar(os.path.join(ORIGINAIS, "leitor_oficial_%s.png" % nome), arquivo, os.path.join(LOCAL, "comparacao_%s.png" % nome), nome)
    # A linha de três cabos e a pedra à direita (X + 3), com a depuração ligada.
    limpar()
    ctl("give", nr.READER, "1")
    wait(6)
    ctl("select", nr.READER)
    for i in range(3):
        cmd("setblock %s %s" % (txt(at(i)), FAMILIAS[0][0]))
    cmd("setblock %s minecraft:stone" % txt(at(3)))
    wait(40)
    ctl("chat", "/mek debug")
    wait(20)
    stand = (at(1)[0] + 0.5 + 1.0, at(1)[1] + 2.5, at(1)[2] - 1.4)
    a.enquadrar(4)
    nr.clicar(at(3), (0.5, 0.5, 0.5), sneak=True, stand=stand)
    arquivo = a.shot("leitor_pedra_debug")
    empilhar(os.path.join(ORIGINAIS, "leitor_oficial_pedra_debug.png"), arquivo, os.path.join(LOCAL, "comparacao_pedra_debug.png"), "pedra_debug")
    stand = (at(1)[0] + 0.5, at(1)[1] + 2.5, at(1)[2] - 1.4)
    a.enquadrar(3)
    nr.clicar(at(1), (0.5, 0.5, 0.5), sneak=True, stand=stand)
    arquivo = a.shot("leitor_cabo_debug")
    empilhar(os.path.join(ORIGINAIS, "leitor_oficial_cabo_debug.png"), arquivo, os.path.join(LOCAL, "comparacao_cabo_debug.png"), "cabo_debug")
    ctl("chat", "/mek debug")
    wait(10)


def preparar_video():
    """Tudo o que escreve no chat sem ser o assunto do vídeo (o `give`, o `fill`, o `tp`): roda antes, e o chat some sozinho em 10 s."""
    cmd("fill %s %s minecraft:air" % (txt(at(0, 0, 0)), txt(at(5, 3, 3))))
    cmd("setblock %s minecraft:stone" % txt(at(3)))
    ctl("give", "mekanism:basic_universal_cable", "3")
    ctl("give", nr.READER, "1")
    wait(10)
    cmd("tp @s %.2f %.2f %.2f" % (at(1)[0] + 1.5, at(1)[1] + 2.5, at(1)[2] - 1.4))
    wait(12)


def roteiro_video():
    """O vídeo: o jogador põe três cabos (o som do bloco posto), liga o `/mek debug`, lê a pedra agachado, lê o cabo agachado, desliga e lê a pedra de novo."""
    ctl("select", "mekanism:basic_universal_cable")
    for i in range(3):
        ctl("look-at", *("%.3f" % v for v in (at(i)[0] + 0.5, at(i)[1], at(i)[2] + 0.5)))
        wait(6)
        ctl("mouse", "right", "click")
        wait(14)
    ctl("select", nr.READER)
    wait(20)
    ctl("chat", "/mek debug")
    wait(50)
    nr.clicar(at(3), (0.5, 0.5, 0.5), sneak=True)
    wait(50)
    nr.clicar(at(1), (0.5, 0.5, 0.5), sneak=True)
    wait(50)
    ctl("chat", "/mek debug")
    wait(40)
    nr.clicar(at(3), (0.5, 0.5, 0.5), sneak=True)
    wait(40)


def video():
    sock = os.environ["SOCK"]
    nome = os.environ.get("NOME", "mek_debug")
    limpar()
    ctl("options-set", "guiScale", "4")
    preparar_video()
    # O chat dos comandos de preparo some depois de 10 s: o vídeo começa com ele já apagado.
    wait(240)
    ctl("options-set", "soundCategory_master", "1.0")
    ctl("options-set", "soundCategory_block", "1.0")
    ctl("options-set", "soundCategory_player", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} {nome}.mp4 50 {os.environ['H_NAME']}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    try:
        roteiro_video()
    finally:
        proc.wait()
        ctl("options-set", "soundCategory_master", "0.0")


def main():
    modo = sys.argv[1] if len(sys.argv) > 1 else "textos"
    if modo == "textos":
        sys.exit(0 if textos() else 1)
    if modo == "prints":
        prints()
    elif modo == "video":
        video()
    else:
        raise SystemExit("modo desconhecido: " + modo)


if __name__ == "__main__":
    main()
