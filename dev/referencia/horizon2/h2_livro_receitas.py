#!/usr/bin/env python3
"""A aba do livro de receitas em jogo (frente `livro-receitas`, `H_NAME=horizon`, mundo plano de teste): uma Enrichment Chamber com a aba em (179, 66), o tooltip `Recipe Book`, e o clique nela
com um item no cursor: o item não solta (`over_element`) e o botão toca o som de clique (`GuiInsetElement`).

    NOME=<nome do gravar-jogo start> [SEGUNDOS=30] [ARQUIVO=livro_receitas.mp4] h2_livro_receitas.py        grava o vídeo com som e confere cada passo (a saída diz `ok` ou `FALHA`)
    h2_livro_receitas.py                                                                                     só confere, sem gravar

O clique é o do botão esquerdo no centro do botão da aba, em (188, 75). O som que fica ligado é o `master` e a categoria `ui` (o clique do botão), e só depois
de o stream do container estar no sink nulo (como `h2_aba_redstone.py`)."""

import os
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import h2_identity as h
import h2_nota_instrumento as n

POS = (40, -60, 40)
BLOCK = "mekanism:enrichment_chamber"
ESCALA = 3
BOTAO = (188, 75)
FORA = (100, 40)
falhas = []


def confere(nome, obtido, esperado):
    ok = obtido == esperado
    print(("ok    " if ok else "FALHA ") + f"{nome}: {obtido!r}" + ("" if ok else f" (esperado {esperado!r})"), flush=True)
    if not ok:
        falhas.append(nome)


def tela():
    gui_l, gui_a = -(-1920 // ESCALA), -(-1080 // ESCALA)
    return (gui_l - 176) // 2, (gui_a - 166) // 2


def sobre(ponto=BOTAO):
    esquerda, topo = tela()
    h.ctl("scroll-at", str(esquerda + ponto[0]), str(topo + ponto[1]), "0")


def clica(direito=False):
    esquerda, topo = tela()
    h.ctl("click-at", str(esquerda + BOTAO[0]), str(topo + BOTAO[1]), *(["right"] if direito else []))


def monta():
    n.sky()
    h.command("tick unfreeze")
    x, y, z = POS
    h.command(f"fill {x - 3} {y} {z - 6} {x + 3} {y + 4} {z + 3} minecraft:air")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.command("clear @s")
    h.ctl("give", BLOCK, "1")
    saida = h.ctl("place", str(x), str(y), str(z), "--item", BLOCK)
    if '"error"' in saida:
        raise SystemExit(f"place: {saida}")
    h.command("clear @s")
    h.ctl("wait", "4")
    h.command("kill @e[type=minecraft:item]")
    h.command("clear @s")
    h.ctl("give", "minecraft:cobblestone", "8")
    h.ctl("tp", f"{x}.0", f"{y}.0", f"{z - 3}.0")
    h.ctl("look", "0", "10")
    h.ctl("options-set", "guiScale", str(ESCALA))
    h.ctl("options-set", "chatVisibility", "2")
    h.ctl("options-set", "showSubtitles", "false")


def itens_no_chao():
    texto = " ".join(h.command("execute if entity @e[type=minecraft:item]")).lower()
    return "passed" in texto


def roteiro(pausa):
    x, y, z = POS
    h.ctl("wait", "10")
    h.ctl("interact", str(x), str(y), str(z))
    pausa(1.5)
    sobre(FORA)
    pausa(1.0)
    # O mouse no botão: o fundo em foco e o tooltip `Recipe Book`.
    sobre()
    pausa(3.0)
    sobre(FORA)
    pausa(1.0)
    # Um item no cursor (o primeiro slot do hotbar), e o clique na aba: o item não solta e o clique toca o som do botão.
    h.ctl("click-slot", "0")
    pausa(0.8)
    sobre()
    pausa(1.5)
    clica()
    pausa(1.5)
    confere("o clique na aba com item no cursor não solta o item", itens_no_chao(), False)
    clica()
    pausa(1.5)
    confere("o segundo clique também não", itens_no_chao(), False)
    h.ctl("click-slot", "0")
    pausa(1.0)
    sobre(FORA)
    pausa(1.0)
    h.ctl("close")
    pausa(1.0)


def main():
    nome = os.environ.get("NOME")
    segundos, arquivo = os.environ.get("SEGUNDOS", "30"), os.environ.get("ARQUIVO", "livro_receitas.mp4")
    monta()
    if not nome:
        roteiro(time.sleep)
        print("falhas:", falhas)
        return
    for categoria in n.SOUND_OFF + ("record",):
        h.ctl("options-set", f"soundCategory_{categoria}", "0")
    # O clique de UI do Horizon toca na categoria `ui` (`audio.play_button_click`), que `SOUND_OFF` desliga: só ela e o master ficam ligados.
    h.ctl("options-set", "soundCategory_ui", "1.0")
    n.mute()
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

    def vigia():
        while not parar.is_set():
            if not n.desktop(f"seguro {n.CONTAINER}").stdout.startswith("ok"):
                print("stream fora dos sinks de teste: volume mestre em 0", flush=True)
                n.mute()
            parar.wait(0.5)

    threading.Thread(target=vigia, daemon=True).start()
    rec = None
    try:
        h.ctl("options-set", "soundCategory_master", "1.0")
        time.sleep(3.0)
        rec = subprocess.Popen(["ssh", "desktop", f"{n.GRAVAR} rec {nome} {arquivo} {segundos} {n.CONTAINER}"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        time.sleep(4.0)
        roteiro(time.sleep)
        out, err = rec.communicate(timeout=int(segundos) + 120)
        print(out, err, flush=True)
    finally:
        n.mute()
        parar.set()
        if rec is not None and rec.poll() is None:
            rec.wait(timeout=60)
        h.ctl("close")
        h.command(f"setblock {POS[0]} {POS[1]} {POS[2]} minecraft:air")
        h.command("kill @e[type=minecraft:item]")
        h.command("clear @s")
        h.ctl("options-set", "chatVisibility", "0")
        n.restore()
    print("falhas:", falhas)


if __name__ == "__main__":
    main()
