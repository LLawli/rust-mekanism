#!/usr/bin/env python3
"""Em jogo (container `horizon2`, `H_NAME=horizon2`): o vídeo com som da linha azul de transportadores com as pilhas passando sobre a grama (frente `transportador-sobras`, quarta rodada, decisão 246).

    h2_pilhas_video.py rodar SOCKNOME ARQUIVO.mp4 [SEGUNDOS]

A linha é a de `gerar_cena_transportador_visual.py` (z 140, fora do `fill` de pedra do setup: o chão é grama), os sete transportadores `dark_blue` e o baú de saída puxando (`PULL`). O baú
tem a espada encantada e o graveto com nome alternados (o original lê o baú de baixo para cima, então a espada do último slot sai primeiro). Com a gravação aberta o jogador POE o baú de
chegada (o som do bloco posto), o baú de saída enche e as pilhas andam em tempo real; ao fim ele quebra um transportador do meio (o som do bloco quebrado e as pilhas caindo). O jogo tem de ter
nascido com `JOGO_PW_SOCK` e com `gamescope -r 30` (`FPS=30 abrir_horizon.sh`), e o volume só sobe depois de `audio_nulo.sh verificar` dar `ok`, como em `h2_desviador_video.py`."""
import os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
os.environ.setdefault("H_NAME", "horizon2")
sys.path.insert(0, TOOLS)
import h2_blocos as hb  # noqa: E402
import h2_identity as h  # noqa: E402

NAME = os.environ["H_NAME"]
Y = -60
X0, Z0 = 60, 140
CAMINHO = [(X0 + 1, Z0), (X0 + 2, Z0), (X0 + 3, Z0), (X0 + 3, Z0 + 1), (X0 + 3, Z0 + 2), (X0 + 4, Z0 + 2), (X0 + 5, Z0 + 2)]
CHEGADA = (X0 + 6, Z0 + 2)
ESPADA = "minecraft:diamond_sword[enchantments={sharpness:5}]"
GRAVETO = "minecraft:stick[custom_name='Nome']"
DESKTOP = "~/.cache/rust-mekanism"
LOCAL = os.environ.get("LOCAL", os.path.expanduser("~/.cache/rust-mekanism/wa/transportador-sobras"))


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    h.command(text)


def esperar(ticks):
    ctl("wait", str(ticks))


def montar():
    """O chão de grama limpo, a linha azul e a conexão `PULL`, sem itens no baú (nada anda ainda)."""
    for c in ("gamemode creative @s", "weather clear", "time set noon", "gamerule advance_time false", "gamerule advance_weather false", "gamerule spawn_mobs false",
              "kill @e[type=!minecraft:player]", "clear @s", "tick unfreeze", f"fill {X0 - 3} {Y} {Z0 - 3} {X0 + 10} {Y + 6} {Z0 + 6} minecraft:air",
              f"setblock {X0} {Y} {Z0} minecraft:chest"):
        cmd(c)
    for x, z in CAMINHO:
        cmd(f"setblock {x} {Y} {z} mekanism:basic_logistical_transporter")
    time.sleep(2.0)
    cmd('give @s mekanism:configurator[mekanism:configurator_mode="configurate_items"] 1')
    esperar(10)
    for x, z in CAMINHO:
        hb.cor(f"#cor {x} {Y} {z} 1")
    hb.conexao(f"#conexao {CAMINHO[0][0]} {Y} {CAMINHO[0][1]} west=pull")
    cmd("clear @s")
    ctl("options-set", "chatVisibility", "2")
    ctl("key", "f1", "tap")


def encher():
    """27 pilhas de uma peça, a espada e o graveto alternados, do último slot (que sai primeiro) para o primeiro."""
    for i in range(26, -1, -1):
        item = ESPADA if (26 - i) % 2 == 0 else GRAVETO
        cmd(f"item replace block {X0} {Y} {Z0} container.{i} with {item} 1")


def camera():
    cmd(f"tp @s {X0 + 3.0} {Y + 2.3} {Z0 - 3.2} 0 24")
    esperar(10)


def roteiro():
    # O baú de chegada posto por quem joga: o botão direito do mouse na face de cima do chão, onde ele fica (o som do bloco posto).
    cmd("give @s minecraft:chest 1")
    esperar(6)
    ctl("select", "minecraft:chest")
    cmd(f"tp @s {CHEGADA[0] - 0.5} {Y + 1.6} {CHEGADA[1] - 3.8} 0 20")
    esperar(10)
    ctl("look-at", f"{CHEGADA[0] + 0.5:.2f}", f"{Y:.2f}", f"{CHEGADA[1] + 0.5:.2f}")
    esperar(8)
    ctl("mouse", "right", "click")
    esperar(14)
    camera()
    encher()
    time.sleep(14)
    # Um transportador do meio quebrado como um jogador: o som do bloco e as pilhas no chão.
    meio = CAMINHO[2]
    cmd(f"tp @s {meio[0] + 0.5} {Y + 1.0} {meio[1] - 2.6} 0 20")
    esperar(8)
    ctl("look-at", f"{meio[0] + 0.5:.2f}", f"{Y + 0.5:.2f}", f"{meio[1] + 0.5:.2f}")
    esperar(6)
    ctl("mouse", "left", "click")
    time.sleep(3.0)


def rodar(sock, arquivo, segundos):
    montar()
    camera()
    # O som sobe só agora (o `verificar` deu `ok` antes): o mestre e as categorias do bloco e do jogador.
    ctl("options-set", "soundCategory_master", "1.0")
    for cat in ("block", "player"):
        ctl("options-set", f"soundCategory_{cat}", "1.0")
    proc = subprocess.Popen(["ssh", "desktop", f"{DESKTOP}/gravar-jogo rec {sock} {os.path.basename(arquivo)} {segundos} {NAME}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    try:
        roteiro()
    finally:
        proc.wait()
        ctl("options-set", "soundCategory_master", "0.0")
        cmd("kill @e[type=minecraft:item]")
    os.makedirs(LOCAL, exist_ok=True)
    subprocess.run(["scp", "-q", f"desktop:{DESKTOP}/gravador/saida/{os.path.basename(arquivo)}", f"{LOCAL}/{os.path.basename(arquivo)}"], check=True)
    subprocess.run(["ssh", "desktop", f"rm -f {DESKTOP}/gravador/saida/{os.path.basename(arquivo)}"], check=False)
    print("vídeo:", f"{LOCAL}/{os.path.basename(arquivo)}")


if __name__ == "__main__":
    if len(sys.argv) < 4 or sys.argv[1] != "rodar":
        sys.exit(__doc__)
    rodar(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else "34")
