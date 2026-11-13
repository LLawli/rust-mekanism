#!/usr/bin/env python3
"""O vídeo com som da sobreposição do Configurator no desviador (frente `desviador-overlay`, decisão 177), gravado de verdade pela captura PipeWire do gamescope num PipeWire
privado (`~/.cache/rust-mekanism/gravar-jogo`, ai-memory `procedures/gravar-video-real-do-jogo`), com o som do jogo juntado (`gravar-jogo rec ... CONTAINER`).

  H_NAME=horizon2 h2_desviador_video.py rodar [SOCKNOME]   o roteiro; com SOCKNOME grava o mp4 em `~/.cache/rust-mekanism/gravador/saida/desviador_<trecho>.mp4`
  H_NAME=horizon2 h2_desviador_video.py prints PASTA        só as ações, com um print a cada passo em PASTA (dentro do container), para conferir o roteiro sem gravar

O roteiro: o jogador põe o desviador no chão (o som do bloco posto), pega o Configurator e, mirando o lado sul do núcleo com `look-at` e o botão direito do mouse, troca o modo do
lado duas vezes (sempre, com sinal, sem sinal: o pó de pólvora, a tocha acesa e a tocha apagada), depois clica no lado leste e na face de cima. O jogo tem de ter nascido com
`JOGO_PW_SOCK` e com `gamescope -r 30` (`FPS=30 abrir_horizon.sh`), e o volume só sobe depois de `audio_nulo.sh verificar` dar `ok`."""
import os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
NAME = os.environ.get("H_NAME", "horizon2")
GRAVADOR = os.environ.get("GRAVADOR", "desviador-overlay")
X, Y, Z = 300, -61, 0


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    return ctl("command", text)


def arena():
    cmd("weather clear")
    ctl("time", "6000")
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    ctl("options-set", "chatVisibility", "2")
    ctl("options-set", "guiScale", "4")
    cmd("gamerule send_command_feedback false")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y + 20} 12 minecraft:air")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y - 1} 12 minecraft:grass_block")
    cmd(f"fill {X - 4} {Y - 1} -4 {X + 4} {Y - 1} 4 minecraft:white_concrete")
    cmd(f"fill {X - 8} {Y} -3 {X + 8} {Y + 10} -3 minecraft:black_concrete")


def mirar(x, y, z):
    ctl("look-at", f"{x:.3f}", f"{y:.3f}", f"{z:.3f}")


def clique():
    ctl("mouse", "right", "click")
    ctl("wait", "24")


def roteiro(print_pasta=None):
    n = [0]

    def foto(nome):
        if print_pasta:
            n[0] += 1
            ctl("screenshot", f"{print_pasta}/desviador_{n[0]:02d}_{nome}.png")

    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd(f"fill {X - 1} {Y} {Z - 1} {X + 1} {Y + 3} {Z + 1} minecraft:air")
    cmd("give @s mekanism:diversion_transporter 1")
    cmd('give @s mekanism:configurator[mekanism:configurator_mode="configurate_items"] 1')
    ctl("wait", "10")
    # Põe o desviador no chão, olhando para o chão de concreto: o som do bloco posto.
    cmd(f"tp @s {X + 0.5} {Y} 2.6 180 31")
    ctl("select", "mekanism:diversion_transporter")
    ctl("wait", "10")
    mirar(X + 0.5, Y, Z + 0.5)
    ctl("wait", "6")
    ctl("mouse", "right", "click")
    ctl("wait", "40")
    foto("posto")
    ctl("select", "mekanism:configurator")
    ctl("wait", "10")
    # Lado sul do núcleo: cada clique troca o modo (sempre, com sinal, sem sinal) e o sprite muda.
    mirar(X + 0.5, Y + 0.5, Z + 0.75)
    ctl("wait", "20")
    foto("sul_sempre")
    for nome in ("sul_com_sinal", "sul_sem_sinal"):
        clique()
        foto(nome)
    # O lado leste: a câmera vai para o leste, e o clique troca só aquele lado.
    cmd(f"tp @s {X + 2.1} {Y} 0.5 90 40")
    ctl("wait", "20")
    mirar(X + 0.75, Y + 0.5, Z + 0.5)
    ctl("wait", "10")
    for nome in ("leste_com_sinal",):
        clique()
        foto(nome)
    # A face de cima, com a rotação para a câmera.
    cmd(f"tp @s {X + 0.5} {Y} -0.6 0 38")
    ctl("wait", "20")
    mirar(X + 0.5, Y + 0.75, Z + 0.5)
    ctl("wait", "10")
    for nome in ("topo_com_sinal",):
        clique()
        foto(nome)
    ctl("wait", "20")


def gravado(sock):
    arena()
    # O som sobe só agora (o `verificar` deu `ok` antes): o mestre e a categoria dos blocos.
    ctl("options-set", "soundCategory_master", "1.0")
    ctl("options-set", "soundCategory_block", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {sock} desviador_cena.mp4 34 {NAME}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    try:
        roteiro()
    finally:
        proc.wait()
        ctl("options-set", "soundCategory_master", "0.0")


if __name__ == "__main__":
    modo = sys.argv[1] if len(sys.argv) > 1 else "rodar"
    if modo == "prints":
        arena()
        roteiro(sys.argv[2])
    else:
        gravado(sys.argv[2] if len(sys.argv) > 2 else GRAVADOR)
