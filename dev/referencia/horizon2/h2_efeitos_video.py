#!/usr/bin/env python3
"""Vídeos em jogo (`H_NAME=horizon2` ou `horizon`) dos efeitos de tela do Configurator e do Configurator com o cartão: gravação de verdade, pela captura PipeWire do gamescope num
PipeWire privado (`~/.cache/rust-mekanism/gravar-jogo`, ai-memory `procedures/gravar-video-real-do-jogo`), enquanto este script age pelo `horizon-ctl`. O jogo tem de ter nascido com
`JOGO_PW_SOCK` (o container) e com `gamescope -r 30` (`FPS=30 abrir_horizon.sh`).

  h2_efeitos_video.py capturar efeitos|integrado SOCKNOME   grava um mp4 por trecho em `~/.cache/rust-mekanism/gravador/saida/<vídeo>_<trecho>.mp4` (SOCKNOME: o nome do `gravar-jogo start`)
  h2_efeitos_video.py montar efeitos|integrado SAIDA.mp4    copia os trechos, põe a legenda de cada um e junta (H.264, 1280 por 720, `-c copy` na junção)

Os cliques saem pelo caminho do mouse (`look-at block` e `mouse right click`): o `horizon-ctl interact` com o jogador agachado deixa passar o clique de cada segunda vez para a tela do
bloco (medido: um graveto numa mesa de crafting, agachado, abre a tela a cada segundo `interact`, e o mouse real, nunca em 40 cliques de 4 modos do Configurator)."""
import os, subprocess, sys, time

TOOLS = os.path.dirname(os.path.abspath(__file__))
NAME = os.environ.get("H_NAME", "horizon2")
SHOTS = "/home/luka/.local/share/jogo-isolado/horizon/data/shots"
HOST_SHOTS = f"~/.local/share/jogo-isolado/{NAME}/data/shots"
X, Y, Z = 300, -61, 0
MODOS = ["configurate_items", "configurate_fluids", "configurate_chemicals", "configurate_energy", "configurate_heat", "empty", "rotate", "wrench"]


def ctl(*args):
    return subprocess.run([f"{TOOLS}/ctl.sh", *args], capture_output=True, text=True, timeout=180).stdout


def cmd(text):
    return ctl("command", text)


GRAVADOR = os.environ.get("GRAVADOR", "efeitos-configurator")


def rajada(video, trecho, segundos, acao):
    """Grava `segundos` de vídeo (captura do gamescope) e roda `acao` enquanto isso."""
    proc = subprocess.Popen(["ssh", "desktop", f"~/.cache/rust-mekanism/gravar-jogo rec {GRAVADOR} {video}_{trecho}.mp4 {segundos}"], stdout=subprocess.DEVNULL)
    time.sleep(2.0)
    acao()
    proc.wait()


def clique(x, y, z, face):
    """O clique direito pelo caminho do mouse."""
    ctl("look-at", "block", str(x), str(y), str(z), face)
    ctl("mouse", "right", "down")
    ctl("wait", "2")
    ctl("mouse", "right", "up")


def arena():
    # Clima limpo e meio-dia, com os ciclos parados (`advance_time` e `advance_weather`, os nomes de agora das regras do dia e do clima).
    cmd("weather clear")
    ctl("time", "6000")  # meio-dia
    cmd("gamerule advance_time false")
    cmd("gamerule advance_weather false")
    ctl("options-set", "chatVisibility", "2")
    ctl("options-set", "guiScale", "4")
    cmd("gamerule send_command_feedback false")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y + 20} 12 minecraft:air")
    cmd(f"fill {X - 8} {Y - 1} -8 {X + 8} {Y - 1} 12 minecraft:grass_block")
    cmd(f"fill {X - 8} {Y} -3 {X + 8} {Y + 10} -3 minecraft:black_concrete")


def configurator(modo="configurate_items", inicial=None):
    cmd("gamemode creative @s")
    cmd("clear @s")
    cmd(f'item replace entity @s hotbar.0 with mekanism:configurator[mekanism:configurator_mode="{inicial or modo}"] 1')
    ctl("select", "1")


def maquina(x, z, config="1,4,7,3,5,9"):
    cmd(f"setblock {x} {Y} {z} mekanism:enrichment_chamber[facing=north]")
    time.sleep(0.5)
    cmd(f"data merge block {x} {Y} {z} {{component_config:{{config3:[I;{config}],config0:[I;0,0,0,0,0,0]}}}}")


def efeitos():
    arena()
    configurator()
    maquina(X, Z)
    cmd(f"tp @s {X + 0.5} {Y} 3.5 180 24")
    ctl("wait", "40")

    def volta():
        # Em volta da máquina: a face de trás, o lado, a de cima, de volta ao lado.
        for yaw, pitch, x, z in [(180, 24, X + 0.5, 3.5), (150, 24, X + 2.2, 2.8), (120, 24, X + 3.0, 1.6), (90, 24, X + 3.5, 0.5), (90, 40, X + 3.0, 0.5), (90, 55, X + 2.2, 0.5), (0, 60, X + 0.5, -1.4), (0, 40, X + 0.5, -1.6), (180, 24, X + 0.5, 3.5)]:
            cmd(f"tp @s {x} {Y} {z} {yaw} {pitch}")
            ctl("wait", "6")
    rajada("efeitos", "1_sobreposicao", 12, volta)
    # Os modos que mostram outro tipo: o fluido (a câmara não tem: sem sobreposição) e a energia.
    def modos_de_configurar():
        for modo in ["configurate_energy", "configurate_fluids", "configurate_items"]:
            configurator(modo)
            ctl("wait", "14")
    cmd(f"tp @s {X + 0.5} {Y} 3.5 180 24")
    rajada("efeitos", "2_tipos", 7, modos_de_configurar)

    def texto_do_modo():
        configurator("configurate_fluids", inicial="configurate_items")
        ctl("wait", "20")
        for _ in range(3):
            ctl("key", "shift", "down")
            ctl("scroll-at", "240", "135", "1")
            ctl("key", "shift", "up")
            ctl("wait", "30")
        ctl("wait", "60")
    cmd(f"tp @s {X + 0.5} {Y} -1.5 180 0")
    rajada("efeitos", "3_texto_do_modo", 14, texto_do_modo)

    def radial():
        configurator("configurate_items")
        ctl("wait", "20")
        ctl("key", "n", "down")
        ctl("wait", "10")
        for dx, dy in [(0, -60), (50, -45), (65, 0), (50, 50), (0, 65), (-50, 50), (-65, 0), (-50, -45), (0, -60)]:
            ctl("scroll-at", str(240 + dx), str(135 + dy), "0")
            ctl("wait", "8")
        ctl("scroll-at", "305", "135", "0")
        ctl("wait", "6")
        ctl("scroll-at", "305", "135", "1")
        ctl("wait", "10")
        ctl("key", "n", "up")
        ctl("wait", "20")
    rajada("efeitos", "4_menu_radial", 22, radial)


def integrado():
    arena()
    configurator("configurate_items")
    maquina(X, Z, "0,0,0,0,0,0")
    maquina(X + 3, Z, "0,0,0,0,0,0")
    cmd(f"tp @s {X + 1.5} {Y} 3.5 180 20")
    ctl("wait", "40")

    def lados():
        ctl("close")
        ctl("key", "shift", "down")
        ctl("wait", "4")
        for _ in range(3):
            clique(X, Y, Z, "south")
            ctl("wait", "14")
        ctl("key", "shift", "up")
        ctl("wait", "10")
    rajada("integrado", "1_configurar_lados", 12, lados)

    def girar():
        ctl("close")
        configurator("rotate")
        ctl("wait", "10")
        for _ in range(2):
            clique(X, Y, Z, "south")
            ctl("wait", "16")
    rajada("integrado", "2_girar", 8, girar)

    def cartao():
        ctl("close")
        cmd("clear @s")
        cmd("give @s mekanism:configuration_card 1")
        ctl("wait", "6")
        ctl("select", "mekanism:configuration_card")
        ctl("key", "shift", "down")
        ctl("wait", "4")
        clique(X, Y, Z, "south")
        ctl("key", "shift", "up")
        ctl("close")
        ctl("wait", "24")
        clique(X + 3, Y, Z, "south")
        ctl("wait", "24")
    cmd(f"tp @s {X + 1.5} {Y} 3.5 180 20")
    rajada("integrado", "3_cartao", 14, cartao)
    # A máquina colada com o Configurator na mão: a sobreposição mostra o que veio.
    def conferir():
        configurator("configurate_items")
        ctl("wait", "10")
        cmd(f"tp @s {X + 3.5} {Y} 3.5 180 22")
        ctl("wait", "30")
    rajada("integrado", "4_conferir", 7, conferir)


LEGENDAS = {
    "efeitos": {
        "1_sobreposicao": "Sobreposicao colorida da face (Configurator em configurar itens)",
        "2_tipos": "Energia mostra a face; fluidos nao (a camara nao tem fluido)",
        "3_texto_do_modo": "Rolagem agachado: o texto do modo sobre a barra, que some em 5 s",
        "4_menu_radial": "Tecla N segurada: menu radial; a rolagem escolhe a fatia e soltar a tecla fecha",
    },
    "integrado": {
        "1_configurar_lados": "Agachado: cada clique troca o DataType da face e avisa na barra",
        "2_girar": "Modo girar: o bloco vira para a face clicada",
        "3_cartao": "Cartao: agachado copia a configuracao, sem agachar cola na outra maquina",
        "4_conferir": "A segunda maquina ficou com a configuracao da primeira",
    },
}


def capturar(video):
    {"efeitos": efeitos, "integrado": integrado}[video]()


def montar(video, saida):
    base = os.path.join(os.path.dirname(os.path.abspath(saida)), f"_{video}")
    os.makedirs(base, exist_ok=True)
    partes = []
    for trecho, legenda in LEGENDAS[video].items():
        bruto = os.path.join(base, f"{trecho}.mp4")
        subprocess.run(["scp", "-q", f"desktop:~/.cache/rust-mekanism/gravador/saida/{video}_{trecho}.mp4", bruto], check=True)
        texto = legenda.replace("'", "").replace(":", " -")
        legendado = os.path.join(base, f"{trecho}_l.mp4")
        filtro = f"scale=1280:720,drawtext=fontfile=/usr/share/fonts/noto/NotoSans-Regular.ttf:text='{texto}':fontcolor=white:fontsize=34:box=1:boxcolor=black@0.6:boxborderw=10:x=(w-text_w)/2:y=40"
        subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", bruto, "-vf", filtro, "-r", "30", "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "26", "-preset", "medium", "-an", legendado], check=True)
        partes.append(f"file '{legendado}'")
    lista = os.path.join(base, "lista.txt")
    open(lista, "w").write("\n".join(partes) + "\n")
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", lista, "-c", "copy", saida], check=True)
    print(saida, os.path.getsize(saida) // 1024, "KB")


if __name__ == "__main__":
    if sys.argv[1] == "capturar":
        if len(sys.argv) > 3:
            GRAVADOR = sys.argv[3]
        capturar(sys.argv[2])
    else:
        montar(sys.argv[2], sys.argv[3])
